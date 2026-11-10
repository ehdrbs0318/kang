// 열린 파일의 해시로 그 파일이 어느 workspace 의 어느 kang 문서인지 찾는다.
// 파일 읽기는 `fs` 인자로 받으므로 이 모듈은 순수하고, 테스트에서 가짜 파일 시스템을 쓴다.
import { createHash } from 'node:crypto'
import { join, resolve } from 'node:path'
import { definitionsOf, parseIndex } from './graph-index'
import type { GraphDoc, GraphEdge, GraphKeyword, Locate } from './protocol'
import { hashInput } from './normalize'
import { trim } from './ws'

/** `workspace.list` 가 돌려주는 workspace 한 개다. */
export type Workspace = {
	id: string
	kind: 'worktree' | 'folder'
	name: string
	path: string
	branch: string | null
	hostId: string | null
}

/** `workspace.readContext` 가 돌려주는 focus 된 workspace 정보 중 비교에 쓰는 부분이다. */
export type Focused = { branch: string; displayName: string }

/**
 * workspace 가 이 Orca host 의 로컬 workspace 인지 본다. Orca 는 로컬 workspace 의 `hostId` 를 null 이나 'local' 로 준다.
 * @param workspace `workspace.list` 의 한 항목
 * @returns 로컬이면 true, 원격(ssh 등)이면 false
 */
export function isLocal(workspace: Workspace): boolean {
	return workspace.hostId === null || workspace.hostId === 'local'
}

/**
 * 파일 읽기 인터페이스다.
 * `read` 는 루트와 그 안의 상대 경로를 받는다. 파일이 없거나 읽을 수 없으면 null 을 돌려준다. 한 루트에서 읽은 양이 한도를 넘으면 ReadLimit 을 던질 수 있다.
 * `dirs` 는 루트 바로 아래의 디렉토리 이름을 돌려준다. symlink 와 `.` 으로 시작하는 이름은 빼고, 읽을 수 없으면 빈 목록이다.
 */
export type FileReader = { read(root: string, path: string): string | null; dirs(root: string): string[] }

/** FileReader 가 한 workspace 에서 읽은 양이 한도를 넘었다는 오류다. locate 는 그 후보를 버리고 다시 고른다. 버린 후보가 focus 된 workspace 이면 `notIndexed` 로 답한다. */
export class ReadLimit extends Error {}

/** 읽어서 검사를 통과한 그래프 인덱스다. `text` 는 파일 원문이다. */
export type GraphIndex = { text: string; docs: GraphDoc[]; keywords: GraphKeyword[]; edges: GraphEdge[] }

/**
 * 원문이 맞은 (kang 프로젝트, 문서) 한 쌍이다.
 * `root` 는 kang 프로젝트 루트의 workspace 기준 상대 경로이고, workspace 루트이면 빈 문자열이다. `path` 는 graph 이면 null 이다.
 */
export type Candidate = { workspace: Workspace; root: string; path: string | null; gitCommonDir: string | null }

/** workspace 안의 kang 프로젝트 하나다. `dir` 는 그 루트의 절대 경로다. */
type Project = { workspace: Workspace; root: string; dir: string }

/**
 * workspace 안의 kang 프로젝트 루트 후보를 모은다. kang 컴파일러처럼 git 저장소 루트를 프로젝트 루트로 본다.
 * 후보는 workspace 루트와, 그 바로 아래에 있는 git 저장소 루트다. 여러 저장소를 한 상위 폴더에 두고 그 폴더를 workspace 로 연 경우를 찾는다.
 * shortcut: 두 단계 이상 아래의 저장소는 찾지 않는다. 그런 구성이 생기면 깊이를 늘린다.
 * @param workspace 로컬 workspace
 * @param fs 파일 읽기
 * @returns workspace 루트가 먼저이고, 그다음 하위 저장소가 이름 순서로 온다
 * @throws ReadLimit 하위 디렉토리의 `.git` 을 읽다 한도를 넘으면 던진다
 */
function projects(workspace: Workspace, fs: FileReader): Project[] {
	const nested = fs
		.dirs(workspace.path)
		.sort()
		// git 저장소가 아닌 하위 디렉토리는 kang 프로젝트 루트가 아니다.
		.filter((name) => gitCommonDir(join(workspace.path, name), fs) !== null)
		.map((name) => ({ workspace, root: name, dir: join(workspace.path, name) }))
	return [{ workspace, root: '', dir: workspace.path }, ...nested]
}

/**
 * 후보가 속한 kang 프로젝트를 구별하는 key 다.
 * @param c 후보
 * @returns workspace id 와 프로젝트 루트를 JSON 배열로 이은 글자
 */
function projectKey(c: { workspace: Workspace; root: string }): string {
	return JSON.stringify([c.workspace.id, c.root])
}

/** 그래프 인덱스 파일의 workspace 루트 기준 경로다. */
const INDEX_PATH = '.kang/graph.kang.json'


/**
 * 문서 해시를 계산한다. 맨 앞 BOM(U+FEFF) 하나를 떼고 정규화한 텍스트의 SHA-256 hex 값이다.
 * `crates/kang/src/hash.rs` 의 `doc_hash` 와 같다. 입력 텍스트는 `hashInput` 이 만든다.
 * @param text 문서 원문
 * @returns 소문자 hex 64 자
 */
export function docHash(text: string): string {
	return createHash('sha256').update(hashInput(text), 'utf8').digest('hex')
}

/**
 * workspace 의 그래프 인덱스를 읽는다.
 * @param root workspace 루트 절대 경로
 * @param fs 파일 읽기
 * @returns 인덱스. 파일이 없거나 `parseIndex` 가 거부하면 null
 * @throws ReadLimit 인덱스 파일을 읽다 이 루트에서 읽은 양이 한도를 넘으면 던진다
 */
export function readIndex(root: string, fs: FileReader): GraphIndex | null {
	const text = fs.read(root, INDEX_PATH)
	// 파일이 없으면 인덱스가 없다.
	if (text === null) return null
	const lists = parseIndex(text)
	// 검사를 통과하지 못하면 인덱스가 없는 것으로 본다.
	return lists && { text, ...lists }
}

/**
 * workspace 의 git 공통 디렉토리를 찾는다. 같은 저장소의 worktree 는 같은 값을 갖는다.
 * `.git` 이 디렉토리이면 그 경로이고, 파일이면 첫 줄의 `gitdir:` 값에서 `/worktrees/<이름>` 을 뗀 경로다.
 * @param root workspace 루트 절대 경로
 * @param fs 파일 읽기
 * @returns 절대 경로. git 저장소가 아니면 null
 * @throws ReadLimit `.git` 파일을 읽다 이 루트에서 읽은 양이 한도를 넘으면 던진다
 */
function gitCommonDir(root: string, fs: FileReader): string | null {
	const file = fs.read(root, '.git')
	// `.git` 이 파일이면 linked worktree 다. gitdir 값을 루트 기준으로 푼다.
	if (file !== null) {
		// git 처럼 첫 줄만 읽는다. 정규식으로 줄 끝 공백을 떼면 긴 공백 줄에서 느리다.
		const line = file.split('\n', 1)[0]
		const dir = trim(line.slice('gitdir:'.length))
		// `gitdir:` 줄이 아니거나 값이 없으면 git 저장소로 보지 않는다.
		if (!line.startsWith('gitdir:') || dir === '') return null
		return resolve(root, dir).replace(/[\\/]worktrees[\\/][^\\/]+$/, '')
	}
	// `.git` 디렉토리는 읽을 수 없으므로 그 안의 HEAD 로 있는지 본다.
	return fs.read(root, '.git/HEAD') !== null ? resolve(root, '.git') : null
}

/**
 * 여러 후보 중 하나를 고른다.
 * focus 된 workspace 와 이름, branch 가 같은 후보를 먼저 남긴다.
 * 남은 후보가 모두 같은 git 저장소이면 첫 번째를 고른다.
 * @param candidates `workspace.list` 순서의 후보들. 하나 이상이어야 한다
 * @param focused focus 된 workspace. 없으면 null
 * @returns 고른 후보. 고를 수 없으면 'ambiguous'
 */
export function pickWorkspace(candidates: Candidate[], focused: Focused | null): Candidate | 'ambiguous' {
	const inFocus = focused
		? candidates.filter((c) => c.workspace.name === focused.displayName && c.workspace.branch === focused.branch)
		: []
	// focus 와 맞는 후보가 있으면 그 후보들만 본다.
	const rest = inFocus.length > 0 ? inFocus : candidates
	if (rest.length === 1) return rest[0]
	const dir = rest[0].gitCommonDir
	// 모두 같은 git 저장소이면 첫 번째다. git 저장소가 아닌 후보가 있으면 고를 수 없다.
	return dir !== null && rest.every((c) => c.gitCommonDir === dir) ? rest[0] : 'ambiguous'
}

/**
 * 열린 파일의 해시로 workspace 와 문서를 찾고, 그 workspace 의 그래프와 정의를 돌려준다.
 * 인덱스만 모든 workspace 에서 읽고, stale 검사는 고른 workspace 하나에서만 한다.
 * 그 검사에서 읽은 양이 한도를 넘으면 그 workspace 의 후보를 버리고 나머지에서 다시 고른다.
 * @param hash 열린 파일의 `docHash` 값. 그래프 인덱스 파일이면 그 원문의 `docHash` 값이다
 * @param workspaces `workspace.list` 의 workspace 목록
 * @param focused focus 된 workspace. 없으면 null
 * @param fs 파일 읽기
 * @returns 찾은 결과. 맞는 인덱스가 없거나 고른 workspace 의 읽은 양이 한도를 넘었고 남은 후보가 없으면 `notIndexed`,
 *   한도를 넘은 workspace 가 focus 된 workspace 이면 다른 저장소를 고르지 않고 `notIndexed`,
 *   원격 workspace 만 있거나 로컬 후보가 없고 focus 된 workspace 가 원격이면 `remoteOnly`,
 *   하나를 고를 수 없거나 고른 workspace 안에 같은 해시의 문서가 둘 이상이면 `ambiguous`
 * @throws Error 파일 읽기 오류 가운데 ReadLimit 이 아닌 것은 그대로 던진다
 */
export function locate(hash: string, workspaces: Workspace[], focused: Focused | null, fs: FileReader): Locate {
	const local = workspaces.filter(isLocal)
	// 로컬 workspace 가 하나도 없고 원격만 있으면 지원하지 않는 경우다.
	if (local.length === 0 && workspaces.length > 0) return { status: 'remoteOnly' }

	const found = new Map<string, GraphIndex>()
	let candidates: Candidate[] = []
	// workspace 마다 그 안의 kang 프로젝트를 모은다. 하위 저장소를 찾다 한도를 넘으면 그 workspace 만 건너뛴다.
	const all: Project[] = []
	for (const workspace of local) {
		try {
			all.push(...projects(workspace, fs))
		} catch (error) {
			// 한도를 넘은 것이 아니면 그대로 던진다.
			if (!(error instanceof ReadLimit)) throw error
		}
	}
	// 프로젝트마다 인덱스를 읽고, 그래프 원문의 해시나 문서 해시가 맞는 후보를 모은다. 문서 파일은 아직 읽지 않는다.
	for (const project of all) {
		try {
			const index = readIndex(project.dir, fs)
			// 인덱스가 없는 프로젝트는 건너뛴다.
			if (!index) continue
			const paths = docHash(index.text) === hash ? [null] : index.docs.filter((d) => d.hash === hash).map((d) => d.path)
			// 맞는 문서가 없는 프로젝트는 후보가 아니다.
			if (paths.length === 0) continue
			const dir = gitCommonDir(project.dir, fs)
			found.set(projectKey(project), index)
			for (const path of paths) candidates.push({ workspace: project.workspace, root: project.root, path, gitCommonDir: dir })
		} catch (error) {
			// 인덱스를 읽다 한도를 넘었으면 이 프로젝트만 건너뛴다. 그 밖의 오류는 그대로 던진다.
			if (!(error instanceof ReadLimit)) throw error
		}
	}

	// 고른 workspace 의 stale 검사가 한도를 넘으면 그 workspace 를 버리고 다시 고른다.
	while (true) {
		// 후보가 없을 때 focus 된 workspace 가 원격이면, 그 문서는 원격 workspace 의 것일 수 있다.
		if (candidates.length === 0) {
			const remote = focused && workspaces.some((w) => !isLocal(w) && w.name === focused.displayName && w.branch === focused.branch)
			return { status: remote ? 'remoteOnly' : 'notIndexed' }
		}
		const picked = pickWorkspace(candidates, focused)
		// 고를 수 없거나, 고른 프로젝트 안에 내용이 같은 문서가 둘 이상이면 어느 문서인지 모른다.
		if (picked === 'ambiguous' || candidates.filter((c) => projectKey(c) === projectKey(picked)).length > 1) return { status: 'ambiguous' }

		const index = found.get(projectKey(picked))!
		const dir = join(picked.workspace.path, picked.root)
		try {
			// 지금 파일의 해시가 인덱스와 다르거나 파일이 없는 문서는 stale 이다.
			const stale = new Set(index.docs.filter((d) => {
				const current = fs.read(dir, `${d.path}.kang`)
				return current === null || docHash(current) !== d.hash
			}).map((d) => d.path))
			// stale 문서가 선언한 keyword 는 정의가 바뀌었을 수 있으므로 뺀다.
			const definitions = definitionsOf(index.keywords, stale)
			return {
				status: 'ok',
				kind: picked.path === null ? 'graph' : 'doc',
				workspaceId: picked.workspace.id,
				root: picked.root,
				path: picked.path,
				docs: index.docs,
				edges: index.edges,
				definitions,
				scope: index.docs.find((d) => d.path === picked.path)?.scope ?? [],
				stale: [...stale],
			}
		} catch (error) {
			// 한도를 넘은 것이 아니면 그대로 던진다.
			if (!(error instanceof ReadLimit)) throw error
			// focus 된 workspace 를 읽지 못하면 다른 저장소의 답을 대신 내지 않는다.
			if (focused && picked.workspace.name === focused.displayName && picked.workspace.branch === focused.branch) return { status: 'notIndexed' }
			// 이 프로젝트의 후보를 버리고 나머지에서 다시 고른다.
			candidates = candidates.filter((c) => projectKey(c) !== projectKey(picked))
		}
	}
}
