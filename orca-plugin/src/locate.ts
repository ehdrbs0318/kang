// 열린 파일의 원문으로 그 파일이 어느 workspace 의 어느 kang 문서인지 찾는다.
// 파일 읽기는 `fs` 인자로 받으므로 이 모듈은 순수하고, 테스트에서 가짜 파일 시스템을 쓴다.
import { createHash } from 'node:crypto'
import { isAbsolute, resolve } from 'node:path'
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

/**
 * workspace 가 이 Orca host 의 로컬 workspace 인지 본다. Orca 는 로컬 workspace 의 `hostId` 를 null 이나 'local' 로 준다.
 * @param workspace `workspace.list` 의 한 항목
 * @returns 로컬이면 true, 원격(ssh 등)이면 false
 */
export function isLocal(workspace: Workspace): boolean {
	return workspace.hostId === null || workspace.hostId === 'local'
}

/** 파일 읽기 인터페이스다. workspace 루트와 그 안의 상대 경로를 받는다. 파일이 없거나 읽을 수 없으면 null 을 돌려준다. */
export type FileReader = { read(root: string, path: string): string | null }

/** 읽어서 검사를 통과한 그래프 인덱스다. `text` 는 파일 원문이다. */
export type GraphIndex = { text: string; docs: GraphDoc[]; keywords: GraphKeyword[]; edges: GraphEdge[] }

/** 원문이 맞은 (workspace, 문서) 한 쌍이다. `path` 는 graph 이면 null 이다. */
export type Candidate = { workspace: Workspace; path: string | null; gitCommonDir: string | null }

/** 그래프 인덱스 파일의 workspace 루트 기준 경로다. */
const INDEX_PATH = '.kang/graph.kang.json'

/** Rust `char::is_whitespace` 와 같은 공백 문자 묶음이다. U+FEFF 는 넣지 않는다. */
const WS = '\\t\\n\\v\\f\\r \\u0085\\u00A0\\u1680\\u2000-\\u200A\\u2028\\u2029\\u202F\\u205F\\u3000'
const SPACE = new RegExp(`[${WS}]`)
const LEADING = new RegExp(`^[${WS}]+`)

/**
 * 끝의 공백 문자를 지운다. `[공백]+$` 정규식은 공백 뒤에 글자가 오는 긴 줄에서 자리마다 다시 훑어 느리다. 그래서 끝에서부터 한 번만 훑는다.
 * @param text 지울 글자
 * @returns 끝의 공백 문자를 뗀 글자
 */
function trimEnd(text: string): string {
	let end = text.length
	// 끝에서부터 공백 문자가 아닌 글자를 만날 때까지 줄인다. WS 의 문자는 모두 UTF-16 한 단위다.
	while (end > 0 && SPACE.test(text[end - 1])) end--
	return text.slice(0, end)
}

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
	return trimEnd(lines.join('\n')).replace(LEADING, '')
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
 *   문서 경로 하나라도 workspace 밖을 가리키거나, `docs` 에 같은 경로가 둘 이상이거나, `edges` 에 같은 (from, to) 쌍이 둘 이상이어도 null
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
 * `.git` 이 디렉토리이면 그 경로이고, 파일이면 `gitdir:` 값에서 `/worktrees/<이름>` 을 뗀 경로다.
 * @param root workspace 루트 절대 경로
 * @param fs 파일 읽기
 * @returns 절대 경로. git 저장소가 아니면 null
 */
function gitCommonDir(root: string, fs: FileReader): string | null {
	const file = fs.read(root, '.git')
	// `.git` 이 파일이면 linked worktree 다. gitdir 값을 루트 기준으로 푼다.
	if (file !== null) {
		const match = /^gitdir:\s*(.+?)\s*$/m.exec(file)
		if (!match) return null
		return resolve(root, match[1]).replace(/[\\/]worktrees[\\/][^\\/]+$/, '')
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
 * @param text 열린 파일의 원문
 * @param workspaces `workspace.list` 의 workspace 목록
 * @param focused focus 된 workspace. 없으면 null
 * @param fs 파일 읽기
 * @returns 찾은 결과. 맞는 인덱스가 없으면 `notIndexed`, 원격 workspace 만 있거나 로컬 후보가 없고 focus 된 workspace 가 원격이면 `remoteOnly`,
 *   하나를 고를 수 없거나 고른 workspace 안에 같은 해시의 문서가 둘 이상이면 `ambiguous`
 */
export function locate(text: string, workspaces: Workspace[], focused: Focused | null, fs: FileReader): Locate {
	const local = workspaces.filter(isLocal)
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
	// 로컬 후보가 없을 때 focus 된 workspace 가 원격이면, 그 문서는 원격 workspace 의 것일 수 있다.
	if (candidates.length === 0) {
		const remote = focused && workspaces.some((w) => !isLocal(w) && w.name === focused.displayName && w.branch === focused.branch)
		return { status: remote ? 'remoteOnly' : 'notIndexed' }
	}

	const picked = pickWorkspace(candidates, focused)
	// 고를 수 없거나, 고른 workspace 안에 내용이 같은 문서가 둘 이상이면 어느 문서인지 모른다.
	if (picked === 'ambiguous' || candidates.filter((c) => c.workspace.id === picked.workspace.id).length > 1) return { status: 'ambiguous' }

	const index = indexes.get(picked.workspace.id)!
	// 지금 파일의 해시가 인덱스와 다르거나 파일이 없는 문서는 stale 이다.
	const stale = new Set(index.docs
		.filter((d) => {
			const current = fs.read(picked.workspace.path, `${d.path}.kang`)
			return current === null || docHash(current) !== d.hash
		})
		.map((d) => d.path))
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
}
