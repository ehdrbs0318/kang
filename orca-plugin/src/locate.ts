// 열린 파일의 원문으로 그 파일이 어느 workspace 의 어느 kang 문서인지 찾는다.
// 파일 읽기는 `fs` 인자로 받으므로 이 모듈은 순수하고, 테스트에서 가짜 파일 시스템을 쓴다.
import { createHash } from 'node:crypto'
import { isAbsolute, resolve } from 'node:path'
import type { GraphDoc, GraphEdge, GraphKeyword, Locate } from './protocol'
import { trim, trimEnd, trimStart } from './ws'

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

/** 파일 읽기 인터페이스다. workspace 루트와 그 안의 상대 경로를 받는다. 파일이 없거나 읽을 수 없으면 null 을 돌려준다. 한 workspace 에서 읽은 양이 한도를 넘으면 ReadLimit 을 던질 수 있다. */
export type FileReader = { read(root: string, path: string): string | null }

/** FileReader 가 한 workspace 에서 읽은 양이 한도를 넘었다는 오류다. locate 는 그 후보를 버리고 다시 고른다. 버린 후보가 focus 된 workspace 이면 `notIndexed` 로 답한다. */
export class ReadLimit extends Error {}

/** 읽어서 검사를 통과한 그래프 인덱스다. `text` 는 파일 원문이다. */
export type GraphIndex = { text: string; docs: GraphDoc[]; keywords: GraphKeyword[]; edges: GraphEdge[] }

/** 원문이 맞은 (workspace, 문서) 한 쌍이다. `path` 는 graph 이면 null 이다. */
export type Candidate = { workspace: Workspace; path: string | null; gitCommonDir: string | null }

/** 그래프 인덱스 파일의 workspace 루트 기준 경로다. */
const INDEX_PATH = '.kang/graph.kang.json'

/** 인덱스 하나가 가질 수 있는 문서 수의 최대값이다. 문서마다 파일을 읽어 해시를 계산하므로 수를 제한한다. */
const MAX_DOCS = 10_000

/**
 * 해시 입력 텍스트를 `crates/kang/src/hash.rs` 의 `normalize` 와 같은 규칙으로 정규화한다.
 * 줄 끝 공백을 지우고, 연속 빈 줄을 하나로 합치고, 앞뒤 공백을 지운다.
 * CRLF 의 CR 은 줄 끝 공백이므로 지워진다.
 * @param text 정규화할 원문
 * @returns 정규화한 텍스트
 */
export function normalize(text: string): string {
	const lines: string[] = []
	// 줄마다 줄 끝 공백을 지우고, 앞 줄도 빈 줄인 빈 줄은 건너뛴다.
	for (const line of text.split('\n')) {
		const trimmed = trimEnd(line)
		if (trimmed === '' && lines.length > 0 && lines[lines.length - 1] === '') continue
		lines.push(trimmed)
	}
	return trimStart(trimEnd(lines.join('\n')))
}

/**
 * 문서 해시를 계산한다. 맨 앞 BOM(U+FEFF) 하나를 떼고 정규화한 텍스트의 SHA-256 hex 값이다.
 * `crates/kang/src/hash.rs` 의 `doc_hash` 와 같다. 컴파일러가 BOM 을 떼고 읽으므로 같이 뗀다.
 * @param text 문서 원문
 * @returns 소문자 hex 64 자
 */
export function docHash(text: string): string {
	return createHash('sha256').update(normalize(text.replace(/^\uFEFF/, '')), 'utf8').digest('hex')
}

/**
 * 값이 문자열인지 본다.
 * @param value 볼 값
 * @returns 문자열이면 true
 */
function isString(value: unknown): value is string {
	return typeof value === 'string'
}

/**
 * 값이 객체이고 주어진 필드가 모두 문자열인지 본다.
 * @param value 볼 값
 * @param keys 문자열이어야 하는 필드 이름들
 * @returns 모두 문자열이면 true. null 이나 객체가 아닌 값은 false
 */
function hasStrings(value: unknown, keys: string[]): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null && keys.every((k) => isString((value as Record<string, unknown>)[k]))
}

/**
 * 그래프 인덱스의 목록 세 개가 모두 계약의 타입을 지키는지 본다.
 * @param docs `docs` 값
 * @param keywords `keywords` 값
 * @param edges `edges` 값
 * @returns 모두 배열이고 모든 항목의 필드 타입이 맞으면 true
 */
function validLists(docs: unknown, keywords: unknown, edges: unknown): boolean {
	return (
		Array.isArray(docs) &&
		docs.every(
			(d) => hasStrings(d, ['path', 'description', 'hash']) && Array.isArray(d.scope) && d.scope.every((e: unknown) => hasStrings(e, ['local', 'address', 'doc'])),
		) &&
		Array.isArray(keywords) &&
		keywords.every((k) => hasStrings(k, ['path', 'name', 'definition'])) &&
		Array.isArray(edges) &&
		edges.every((e) => hasStrings(e, ['from', 'to']) && Array.isArray(e.symbols) && e.symbols.every(isString))
	)
}

/**
 * 인덱스의 문서 경로가 workspace 안을 가리키는지 본다. 인덱스는 커밋된 파일이라 어느 저장소에서든 올 수 있다.
 * 조각이 모두 비지 않고 `.` 나 `..` 가 아니면, 루트에 이은 경로가 루트 밖으로 나갈 수 없다.
 * @param path `/` 로 나눈 문서 경로
 * @returns 상대 경로이고 역슬래시가 없고 모든 조각이 보통 이름이면 true
 */
function insidePath(path: string): boolean {
	return !isAbsolute(path) && !path.includes('\\') && path.split('/').every((s) => s !== '' && s !== '.' && s !== '..')
}

/**
 * workspace 의 그래프 인덱스를 읽는다.
 * @param root workspace 루트 절대 경로
 * @param fs 파일 읽기
 * @returns 인덱스. 파일이 없거나, JSON 이 깨졌거나, `kang` 표지가 `graph/1` 이 아니거나, 항목 하나라도 타입이 틀리면 null.
 *   문서가 MAX_DOCS 개를 넘거나, 문서 경로 하나라도 workspace 밖을 가리키거나, `docs` 에 같은 경로가 둘 이상이거나, `edges` 에 같은 (from, to) 쌍이 둘 이상이어도 null
 * @throws ReadLimit 인덱스 파일을 읽다 이 루트에서 읽은 양이 한도를 넘으면 던진다
 */
export function readIndex(root: string, fs: FileReader): GraphIndex | null {
	const text = fs.read(root, INDEX_PATH)
	// 파일이 없으면 인덱스가 없다.
	if (text === null) return null
	let value: unknown
	try {
		value = JSON.parse(text)
	} catch {
		// 깨진 JSON 은 인덱스가 없는 것으로 본다.
		return null
	}
	const index = value as { kang?: unknown; docs?: unknown; keywords?: unknown; edges?: unknown } | null
	// kang 이 만든 파일이 아니거나 항목 하나라도 타입이 틀리면 인덱스가 없는 것으로 본다. 그래야 깨진 인덱스 하나가 다른 workspace 를 막지 않는다.
	if (!index || index.kang !== 'graph/1' || !validLists(index.docs, index.keywords, index.edges)) return null
	const docs = index.docs as GraphDoc[]
	// 문서가 너무 많으면 stale 검사가 파일을 너무 많이 읽으므로 인덱스가 없는 것으로 본다.
	if (docs.length > MAX_DOCS) return null
	const keywords = index.keywords as GraphKeyword[]
	const edges = index.edges as GraphEdge[]
	const paths = [...docs.flatMap((d) => [d.path, ...d.scope.map((e) => e.doc)]), ...keywords.map((k) => k.path), ...edges.flatMap((e) => [e.from, e.to])]
	// workspace 밖을 가리키는 경로가 있으면 그 파일을 읽거나 열 수 있으므로 인덱스 전체를 버린다.
	if (!paths.every(insidePath)) return null
	// 같은 문서 경로가 둘이면 그래프 화면의 점 id 가 겹치므로 인덱스 전체를 버린다.
	if (new Set(docs.map((d) => d.path)).size !== docs.length) return null
	// 같은 (from, to) 쌍이 둘이면 kang build 가 만든 인덱스가 아니므로 인덱스 전체를 버린다. JSON 배열로 이어 경로 안의 글자와 섞이지 않게 한다.
	if (new Set(edges.map((e) => JSON.stringify([e.from, e.to]))).size !== edges.length) return null
	return { text, docs, keywords, edges }
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
 * 열린 파일의 원문으로 workspace 와 문서를 찾고, 그 workspace 의 그래프와 정의를 돌려준다.
 * 인덱스만 모든 workspace 에서 읽고, stale 검사는 고른 workspace 하나에서만 한다.
 * 그 검사에서 읽은 양이 한도를 넘으면 그 workspace 의 후보를 버리고 나머지에서 다시 고른다.
 * @param text 열린 파일의 원문
 * @param workspaces `workspace.list` 의 workspace 목록
 * @param focused focus 된 workspace. 없으면 null
 * @param fs 파일 읽기
 * @returns 찾은 결과. 맞는 인덱스가 없거나 고른 workspace 의 읽은 양이 한도를 넘었고 남은 후보가 없으면 `notIndexed`,
 *   한도를 넘은 workspace 가 focus 된 workspace 이면 다른 저장소를 고르지 않고 `notIndexed`,
 *   원격 workspace 만 있거나 로컬 후보가 없고 focus 된 workspace 가 원격이면 `remoteOnly`,
 *   하나를 고를 수 없거나 고른 workspace 안에 같은 해시의 문서가 둘 이상이면 `ambiguous`
 * @throws Error 파일 읽기 오류 가운데 ReadLimit 이 아닌 것은 그대로 던진다
 */
export function locate(text: string, workspaces: Workspace[], focused: Focused | null, fs: FileReader): Locate {
	const local = workspaces.filter(isLocal)
	// 로컬 workspace 가 하나도 없고 원격만 있으면 지원하지 않는 경우다.
	if (local.length === 0 && workspaces.length > 0) return { status: 'remoteOnly' }

	const hash = docHash(text)
	const found = new Map<string, GraphIndex>()
	let candidates: Candidate[] = []
	// workspace 마다 인덱스를 읽고, 그래프 원문이나 문서 해시가 맞는 후보를 모은다. 문서 파일은 아직 읽지 않는다.
	for (const workspace of local) {
		try {
			const index = readIndex(workspace.path, fs)
			// 인덱스가 없는 workspace 는 건너뛴다.
			if (!index) continue
			const paths = index.text === text ? [null] : index.docs.filter((d) => d.hash === hash).map((d) => d.path)
			// 맞는 문서가 없는 workspace 는 후보가 아니다.
			if (paths.length === 0) continue
			const dir = gitCommonDir(workspace.path, fs)
			found.set(workspace.id, index)
			for (const path of paths) candidates.push({ workspace, path, gitCommonDir: dir })
		} catch (error) {
			// 인덱스를 읽다 한도를 넘었으면 이 workspace 만 건너뛴다. 그 밖의 오류는 그대로 던진다.
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
		// 고를 수 없거나, 고른 workspace 안에 내용이 같은 문서가 둘 이상이면 어느 문서인지 모른다.
		if (picked === 'ambiguous' || candidates.filter((c) => c.workspace.id === picked.workspace.id).length > 1) return { status: 'ambiguous' }

		const index = found.get(picked.workspace.id)!
		try {
			// 지금 파일의 해시가 인덱스와 다르거나 파일이 없는 문서는 stale 이다.
			const stale = new Set(index.docs.filter((d) => {
				const current = fs.read(picked.workspace.path, `${d.path}.kang`)
				return current === null || docHash(current) !== d.hash
			}).map((d) => d.path))
			const definitions: Record<string, string> = {}
			// stale 문서가 선언한 keyword 는 정의가 바뀌었을 수 있으므로 뺀다.
			for (const k of index.keywords) {
				if (!stale.has(k.path)) definitions[`${k.path}.${k.name}`] = k.definition
			}
			return {
				status: 'ok',
				kind: picked.path === null ? 'graph' : 'doc',
				workspaceId: picked.workspace.id,
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
			// 이 workspace 의 후보를 버리고 나머지에서 다시 고른다.
			candidates = candidates.filter((c) => c.workspace.id !== picked.workspace.id)
		}
	}
}
