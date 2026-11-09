// 그래프 인덱스를 그래프 화면의 모델과 cytoscape 요소로 바꾼다. DOM 을 쓰지 않으므로 Node 테스트로 검사한다.
import type { ElementDefinition } from '../../vendor/cytoscape.esm.min.mjs'
import type { GraphDoc, GraphEdge } from '../protocol'

/** 한 문서와 다른 문서 사이의 import 관계 하나다. `path` 는 상대 문서, `symbols` 는 import 한 심볼 주소다. */
export type Link = { path: string; symbols: string[] }

/** 문서의 역할이다. 들어오는 연결이 가장 많으면 `hub`, 연결이 하나도 없으면 `orphan` 이다. */
export type Role = 'hub' | 'orphan' | 'normal'

/** 그래프 화면이 문서 하나에 대해 보여 주는 값이다. */
export type DocNode = {
	path: string
	/** 점 이름. `shortLabel` 의 결과다. */
	label: string
	folder: string
	/** `GraphModel.folders` 안의 위치. 점 색을 고를 때 쓴다. */
	folderIndex: number
	description: string
	role: Role
	/** 이 문서를 import 하는 문서들 */
	incoming: Link[]
	/** 이 문서가 import 하는 문서들 */
	outgoing: Link[]
	/** 이 문서가 선언한 keyword. `name` 은 문서 경로를 뺀 이름이다. */
	keywords: { name: string; definition: string }[]
}

/** 그래프 화면 전체의 모델이다. `folders` 는 정렬한 폴더 목록이다. */
export type GraphModel = { nodes: DocNode[]; edges: GraphEdge[]; folders: string[] }

/**
 * 점 이름을 만든다. 경로의 마지막 부분을 쓰고 `-` 와 `_` 를 공백으로 바꾼다. 그래야 그래프가 이름을 여러 줄로 감쌀 수 있다.
 * `max` 자보다 길면 줄여서 `…` 를 붙인다.
 * @param path 문서 경로
 * @param max 이름의 최대 글자 수. 글자는 code point 로 센다
 * @returns `max` 글자 이하의 이름
 */
export function shortLabel(path: string, max = 48): string {
	const chars = [...path.slice(path.lastIndexOf('/') + 1).replace(/[-_]+/g, ' ')]
	// 길이가 넘으면 마지막 한 글자 자리에 … 를 넣는다.
	return chars.length > max ? `${chars.slice(0, max - 1).join('')}…` : chars.join('')
}

/**
 * 문서가 있는 폴더를 돌려준다.
 * @param path 문서 경로
 * @returns 마지막 `/` 앞부분. 루트의 문서이면 `'/'`
 */
export function folderOf(path: string): string {
	const cut = path.lastIndexOf('/')
	return cut < 0 ? '/' : path.slice(0, cut)
}

/**
 * 그래프 인덱스로 화면 모델을 만든다.
 * 문서 목록에 없는 문서를 가리키는 연결은 버린다. keyword 의 주인은 `definitions` 키 앞부분과 가장 길게 맞는 문서다.
 * @param docs 그래프 인덱스의 문서 목록
 * @param edges 그래프 인덱스의 import 관계 목록
 * @param definitions `<문서 경로>.<keyword 이름>` 을 정의로 잇는 표. `kang.locate` 답의 값이다
 * @returns 문서 순서를 지킨 모델
 */
export function buildModel(docs: GraphDoc[], edges: GraphEdge[], definitions: Record<string, string>): GraphModel {
	const known = new Set(docs.map((d) => d.path))
	// cytoscape 는 없는 점을 가리키는 화살표를 거부하므로 버린다.
	const kept = edges.filter((e) => known.has(e.from) && known.has(e.to))
	const folders = [...new Set(docs.map((d) => folderOf(d.path)))].sort()
	const incoming = new Map<string, Link[]>()
	const outgoing = new Map<string, Link[]>()
	// 연결마다 양쪽 문서에 상대 문서를 적는다.
	for (const e of kept) {
		incoming.set(e.to, [...(incoming.get(e.to) ?? []), { path: e.from, symbols: e.symbols }])
		outgoing.set(e.from, [...(outgoing.get(e.from) ?? []), { path: e.to, symbols: e.symbols }])
	}
	const keywords = new Map<string, { name: string; definition: string }[]>()
	// 경로가 긴 문서부터 맞춰 보아야 `a.b.c` 가 `a` 가 아니라 `a.b` 의 keyword 가 된다.
	const longestFirst = [...known].sort((x, y) => y.length - x.length)
	for (const [key, definition] of Object.entries(definitions)) {
		const owner = longestFirst.find((p) => key.startsWith(`${p}.`))
		// 주인 문서가 인덱스에 없으면 보여 줄 곳이 없으므로 버린다.
		if (owner) keywords.set(owner, [...(keywords.get(owner) ?? []), { name: key.slice(owner.length + 1), definition }])
	}
	const top = Math.max(0, ...docs.map((d) => incoming.get(d.path)?.length ?? 0))
	return {
		folders,
		edges: kept,
		nodes: docs.map((d): DocNode => {
			const ins = incoming.get(d.path) ?? []
			const outs = outgoing.get(d.path) ?? []
			const folder = folderOf(d.path)
			// 허브는 들어오는 연결이 가장 많은 문서다. 연결이 하나도 없으면 허브가 없다.
			const role: Role = top > 0 && ins.length === top ? 'hub' : ins.length + outs.length === 0 ? 'orphan' : 'normal'
			return {
				path: d.path,
				label: shortLabel(d.path),
				folder,
				folderIndex: folders.indexOf(folder),
				description: d.description,
				role,
				incoming: ins,
				outgoing: outs,
				keywords: keywords.get(d.path) ?? [],
			}
		}),
	}
}

/**
 * 검색어가 문서의 이름, 경로, description 에 있는지 본다. 대소문자를 가리지 않는다.
 * @param node 문서
 * @param query 검색어. 비어 있으면 모든 문서가 맞는다
 * @returns 맞으면 true
 */
export function matches(node: DocNode, query: string): boolean {
	const q = query.trim().toLowerCase()
	return [node.label, node.path, node.description].some((s) => s.toLowerCase().includes(q))
}

/**
 * 화면 모델을 cytoscape 요소로 바꾼다. 점의 `weight` 는 들어오는 연결 수, 화살표의 `count` 는 심볼 수다.
 * @param model `buildModel` 의 결과
 * @returns 점을 먼저, 화살표를 뒤에 둔 요소 목록. 화살표의 `symbols` 는 심볼 주소를 줄바꿈으로 이은 문자열이다
 */
export function toElements(model: GraphModel): ElementDefinition[] {
	return [
		...model.nodes.map((n): ElementDefinition => ({
			group: 'nodes',
			data: { id: n.path, label: n.label, weight: n.incoming.length, folderIndex: n.folderIndex, role: n.role },
		})),
		...model.edges.map((e): ElementDefinition => ({
			group: 'edges',
			data: { source: e.from, target: e.to, symbols: e.symbols.join('\n'), count: e.symbols.length },
		})),
	]
}
