// 열린 파일의 원문으로 그 파일이 어느 workspace 의 어느 kang 문서인지 찾는다.
// 파일 읽기는 `fs` 인자로 받으므로 이 모듈은 순수하고, 테스트에서 가짜 파일 시스템을 쓴다.
import { createHash } from 'node:crypto'
import { join, resolve } from 'node:path'
import type { GraphDoc, GraphEdge, GraphKeyword, Locate } from './protocol'

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

/** 파일 읽기 인터페이스다. 파일이 없거나 읽을 수 없으면 null 을 돌려준다. */
export type FileReader = { read(path: string): string | null }

/** 읽어서 검사를 통과한 그래프 인덱스다. `text` 는 파일 원문이다. */
export type GraphIndex = { text: string; docs: GraphDoc[]; keywords: GraphKeyword[]; edges: GraphEdge[] }

/** 원문이 맞은 (workspace, 문서) 한 쌍이다. `path` 는 graph 이면 null 이다. */
export type Candidate = { workspace: Workspace; path: string | null; gitCommonDir: string | null }

/** 그래프 인덱스 파일의 workspace 루트 기준 경로다. */
export const INDEX_PATH = '.kang/graph.kang.json'

/** Rust `char::is_whitespace` 와 같은 공백 문자 묶음이다. U+FEFF 는 넣지 않는다. */
const WS = '\\t\\n\\v\\f\\r \\u0085\\u00A0\\u1680\\u2000-\\u200A\\u2028\\u2029\\u202F\\u205F\\u3000'
const TRAILING = new RegExp(`[${WS}]+$`)
const LEADING = new RegExp(`^[${WS}]+`)

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
		const trimmed = line.replace(TRAILING, '')
		if (trimmed === '' && lines.length > 0 && lines[lines.length - 1] === '') continue
		lines.push(trimmed)
	}
	return lines.join('\n').replace(TRAILING, '').replace(LEADING, '')
}

/**
 * 정규화한 텍스트의 SHA-256 hex 값을 계산한다.
 * @param text 원문
 * @returns 소문자 hex 64 자
 */
export function docHash(text: string): string {
	return createHash('sha256').update(normalize(text), 'utf8').digest('hex')
}

/**
 * workspace 의 그래프 인덱스를 읽는다.
 * @param root workspace 루트 절대 경로
 * @param fs 파일 읽기
 * @returns 인덱스. 파일이 없거나, JSON 이 깨졌거나, `kang` 표지가 `graph/1` 이 아니면 null
 */
export function readIndex(root: string, fs: FileReader): GraphIndex | null {
	const text = fs.read(join(root, INDEX_PATH))
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
	// kang 이 만든 파일이 아니거나 목록이 없으면 인덱스가 없는 것으로 본다.
	if (!index || index.kang !== 'graph/1' || !Array.isArray(index.docs) || !Array.isArray(index.keywords) || !Array.isArray(index.edges)) {
		return null
	}
	return { text, docs: index.docs, keywords: index.keywords, edges: index.edges }
}

/**
 * workspace 의 git 공통 디렉토리를 찾는다. 같은 저장소의 worktree 는 같은 값을 갖는다.
 * `.git` 이 디렉토리이면 그 경로이고, 파일이면 `gitdir:` 값에서 `/worktrees/<이름>` 을 뗀 경로다.
 * @param root workspace 루트 절대 경로
 * @param fs 파일 읽기
 * @returns 절대 경로. git 저장소가 아니면 null
 */
function gitCommonDir(root: string, fs: FileReader): string | null {
	const dotGit = join(root, '.git')
	const file = fs.read(dotGit)
	// `.git` 이 파일이면 linked worktree 다. gitdir 값을 루트 기준으로 푼다.
	if (file !== null) {
		const match = /^gitdir:\s*(.+?)\s*$/m.exec(file)
		if (!match) return null
		return resolve(root, match[1]).replace(/[\\/]worktrees[\\/][^\\/]+$/, '')
	}
	// `.git` 디렉토리는 읽을 수 없으므로 그 안의 HEAD 로 있는지 본다.
	return fs.read(join(dotGit, 'HEAD')) !== null ? resolve(dotGit) : null
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
 * @param text 열린 파일의 원문
 * @param workspaces `workspace.list` 의 workspace 목록
 * @param focused focus 된 workspace. 없으면 null
 * @param fs 파일 읽기
 * @returns 찾은 결과. 맞는 인덱스가 없으면 `notIndexed`, 원격 workspace 만 있으면 `remoteOnly`,
 *   하나를 고를 수 없으면 `ambiguous`
 */
export function locate(text: string, workspaces: Workspace[], focused: Focused | null, fs: FileReader): Locate {
	const local = workspaces.filter((w) => w.hostId === null)
	// 로컬 workspace 가 하나도 없고 원격만 있으면 지원하지 않는 경우다.
	if (local.length === 0 && workspaces.length > 0) return { status: 'remoteOnly' }

	const hash = docHash(text)
	const indexes = new Map<string, GraphIndex>()
	const candidates: Candidate[] = []
	// workspace 마다 인덱스를 읽고, 그래프 원문이나 문서 해시가 맞는 후보를 모은다.
	for (const workspace of local) {
		const index = readIndex(workspace.path, fs)
		if (!index) continue
		indexes.set(workspace.id, index)
		const paths = index.text === text ? [null] : index.docs.filter((d) => d.hash === hash).map((d) => d.path)
		for (const path of paths) candidates.push({ workspace, path, gitCommonDir: gitCommonDir(workspace.path, fs) })
	}
	if (candidates.length === 0) return { status: 'notIndexed' }

	const picked = pickWorkspace(candidates, focused)
	if (picked === 'ambiguous') return { status: 'ambiguous' }

	const root = picked.workspace.path
	const index = indexes.get(picked.workspace.id)!
	// 지금 파일의 해시가 인덱스와 다르거나 파일이 없는 문서는 stale 이다.
	const stale = index.docs
		.filter((d) => {
			const current = fs.read(join(root, `${d.path}.kang`))
			return current === null || docHash(current) !== d.hash
		})
		.map((d) => d.path)
	const definitions: Record<string, string> = {}
	// stale 문서가 선언한 keyword 는 정의가 바뀌었을 수 있으므로 뺀다.
	for (const k of index.keywords) {
		if (!stale.includes(k.path)) definitions[`${k.path}.${k.name}`] = k.definition
	}
	return {
		status: 'ok',
		kind: picked.path === null ? 'graph' : 'doc',
		workspaceId: picked.workspace.id,
		path: picked.path,
		docs: index.docs,
		edges: index.edges,
		definitions,
		stale,
	}
}
