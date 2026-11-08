// model.ts 가 그래프 인덱스의 문서와 import 관계를 cytoscape 요소로 바꾸는지 검사한다.
import { assert, test } from 'orca/testing'
import { toElements } from './model'

/**
 * 테스트용 문서 한 줄을 만든다.
 * @param path 문서 경로
 * @returns description 과 hash 를 채우고 scope 는 비운 문서
 */
function doc(path: string) {
	return { path, description: `${path} 설명`, hash: '0'.repeat(64), scope: [] }
}

test('문서마다 점 하나, 연결마다 화살표 하나를 만들고 weight 는 들어오는 연결 수다', () => {
	const elements = toElements(
		[doc('A'), doc('B'), doc('C')],
		[
			{ from: 'A', to: 'C', symbols: ['C.결제'] },
			{ from: 'B', to: 'C', symbols: ['C.결제', 'C#topic'] },
			{ from: 'C', to: 'A', symbols: ['A.주문'] },
		],
	)
	assert.deepEqual(elements, [
		{ group: 'nodes', data: { id: 'A', label: 'A', weight: 1 } },
		{ group: 'nodes', data: { id: 'B', label: 'B', weight: 0 } },
		{ group: 'nodes', data: { id: 'C', label: 'C', weight: 2 } },
		{ group: 'edges', data: { source: 'A', target: 'C', symbols: 'C.결제' } },
		{ group: 'edges', data: { source: 'B', target: 'C', symbols: 'C.결제\nC#topic' } },
		{ group: 'edges', data: { source: 'C', target: 'A', symbols: 'A.주문' } },
	])
})

test('인덱스의 문서 목록에 없는 문서를 가리키는 연결은 버린다', () => {
	const elements = toElements([doc('A')], [{ from: 'A', to: 'Z', symbols: ['Z.x'] }])
	assert.deepEqual(elements, [{ group: 'nodes', data: { id: 'A', label: 'A', weight: 0 } }])
})
