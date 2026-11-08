// 그래프 인덱스의 문서와 import 관계를 cytoscape 요소로 바꾼다. DOM 을 쓰지 않으므로 Node 테스트로 검사한다.
import type { ElementDefinition } from '../../vendor/cytoscape.esm.min.mjs'
import type { GraphDoc, GraphEdge } from '../protocol'

/**
 * 문서마다 점 하나, import 관계마다 화살표 하나를 만든다.
 * 점의 `weight` 는 그 문서로 들어오는 화살표 수다. 화살표는 import 하는 문서에서 import 되는 문서로 간다.
 * 인덱스가 같은 문서 쌍의 관계를 이미 하나로 합쳤으므로, 쌍마다 화살표는 하나다. 화살표 id 는 cytoscape 가 만든다.
 * @param docs 그래프 인덱스의 문서 목록
 * @param edges 그래프 인덱스의 import 관계 목록
 * @returns 점을 먼저, 화살표를 뒤에 둔 요소 목록. 화살표의 `symbols` 는 심볼 주소를 줄바꿈으로 이은 문자열이다
 */
export function toElements(docs: GraphDoc[], edges: GraphEdge[]): ElementDefinition[] {
	const known = new Set(docs.map((d) => d.path))
	// 문서 목록에 없는 문서를 가리키는 관계는 cytoscape 가 거부하므로 버린다.
	const kept = edges.filter((e) => known.has(e.from) && known.has(e.to))
	const incoming = new Map<string, number>()
	// 문서마다 들어오는 화살표를 센다.
	for (const e of kept) incoming.set(e.to, (incoming.get(e.to) ?? 0) + 1)
	return [
		...docs.map((d): ElementDefinition => ({ group: 'nodes', data: { id: d.path, label: d.path, weight: incoming.get(d.path) ?? 0 } })),
		...kept.map((e): ElementDefinition => ({
			group: 'edges',
			data: { source: e.from, target: e.to, symbols: e.symbols.join('\n') },
		})),
	]
}
