// 그래프 인덱스 원문을 검사해 읽는다. worker(Node)와 그래프 화면(브라우저)이 함께 쓰므로 Node 모듈을 import 하지 않는다.
// 그래프 화면은 worker 가 파일을 찾지 못할 때(원격 workspace 등) 이 모듈로 열린 원문을 직접 읽어 그린다.
import type { GraphDoc, GraphEdge, GraphKeyword } from './protocol'

/** 인덱스 하나가 가질 수 있는 문서 수의 최대값이다. worker 는 문서마다 파일을 읽어 해시를 계산하므로 수를 제한한다. */
const MAX_DOCS = 10_000

/** 검사를 통과한 그래프 인덱스의 목록 세 개다. */
export type IndexLists = { docs: GraphDoc[]; keywords: GraphKeyword[]; edges: GraphEdge[] }

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
 * 인덱스의 문서 경로가 프로젝트 안을 가리키는지 본다. 인덱스는 커밋된 파일이라 어느 저장소에서든 올 수 있다.
 * 조각이 모두 비지 않고 `.` 나 `..` 가 아니면, 루트에 이은 경로가 루트 밖으로 나갈 수 없다.
 * worker 는 macOS 와 Linux 에서만 돌므로 `/` 로 시작하는 경로만 절대 경로로 본다.
 * @param path `/` 로 나눈 문서 경로
 * @returns 상대 경로이고 역슬래시가 없고 모든 조각이 보통 이름이면 true
 */
function insidePath(path: string): boolean {
	return !path.startsWith('/') && !path.includes('\\') && path.split('/').every((s) => s !== '' && s !== '.' && s !== '..')
}

/**
 * 그래프 인덱스 원문을 검사해 읽는다.
 * @param text `.kang/graph.kang.json` 원문
 * @returns 목록 세 개. JSON 이 깨졌거나, `kang` 표지가 `graph/1` 이 아니거나, 항목 하나라도 타입이 틀리면 null.
 *   문서가 MAX_DOCS 개를 넘거나, 경로 하나라도 프로젝트 밖을 가리키거나, `docs` 에 같은 경로가 둘 이상이거나, `edges` 에 같은 (from, to) 쌍이 둘 이상이어도 null
 */
export function parseIndex(text: string): IndexLists | null {
	let value: unknown
	try {
		value = JSON.parse(text)
	} catch {
		// 깨진 JSON 은 인덱스가 아니다.
		return null
	}
	const index = value as { kang?: unknown; docs?: unknown; keywords?: unknown; edges?: unknown } | null
	// kang 이 만든 파일이 아니거나 항목 하나라도 타입이 틀리면 인덱스가 아니다. 그래야 깨진 인덱스 하나가 다른 workspace 를 막지 않는다.
	if (!index || index.kang !== 'graph/1' || !validLists(index.docs, index.keywords, index.edges)) return null
	const docs = index.docs as GraphDoc[]
	// 문서가 너무 많으면 stale 검사가 파일을 너무 많이 읽으므로 인덱스가 아닌 것으로 본다.
	if (docs.length > MAX_DOCS) return null
	const keywords = index.keywords as GraphKeyword[]
	const edges = index.edges as GraphEdge[]
	const paths = [...docs.flatMap((d) => [d.path, ...d.scope.map((e) => e.doc)]), ...keywords.map((k) => k.path), ...edges.flatMap((e) => [e.from, e.to])]
	// 프로젝트 밖을 가리키는 경로가 있으면 그 파일을 읽거나 열 수 있으므로 인덱스 전체를 버린다.
	if (!paths.every(insidePath)) return null
	// 같은 문서 경로가 둘이면 그래프 화면의 점 id 가 겹치므로 인덱스 전체를 버린다.
	if (new Set(docs.map((d) => d.path)).size !== docs.length) return null
	// 같은 (from, to) 쌍이 둘이면 kang build 가 만든 인덱스가 아니므로 인덱스 전체를 버린다. JSON 배열로 이어 경로 안의 글자와 섞이지 않게 한다.
	if (new Set(edges.map((e) => JSON.stringify([e.from, e.to]))).size !== edges.length) return null
	return { docs, keywords, edges }
}

/**
 * keyword 목록으로 정의 표를 만든다. key 는 `<문서 경로>.<keyword 이름>` 이다.
 * @param keywords 인덱스의 keyword 목록
 * @param skip 정의를 빼는 문서 경로. 인덱스가 오래된 문서가 선언한 keyword 는 정의가 바뀌었을 수 있다
 * @returns 정의 표
 */
export function definitionsOf(keywords: GraphKeyword[], skip: ReadonlySet<string> = new Set()): Record<string, string> {
	const definitions: Record<string, string> = {}
	// 빼는 문서가 선언한 것을 뺀 keyword 마다 정의를 넣는다.
	for (const k of keywords) {
		if (!skip.has(k.path)) definitions[`${k.path}.${k.name}`] = k.definition
	}
	return definitions
}
