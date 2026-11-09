// model.ts 가 그래프 인덱스를 화면 모델과 cytoscape 요소로 바꾸는지 검사한다.
import { assert, test } from 'orca/testing'
import { buildModel, folderOf, matches, shortLabel, toElements } from './model'

/**
 * 테스트용 문서 한 줄을 만든다.
 * @param path 문서 경로
 * @returns description 과 hash 를 채우고 scope 는 비운 문서
 */
function doc(path: string) {
	return { path, description: `${path} 설명`, hash: '0'.repeat(64), scope: [] }
}

test('shortLabel 은 경로의 마지막 부분을 쓰고, - 와 _ 를 공백으로 바꾸고, 길면 줄여서 … 를 붙인다', () => {
	assert.equal(shortLabel('examples/ts-consumer/docs/proration'), 'proration')
	assert.equal(shortLabel('CONTEXT'), 'CONTEXT')
	assert.equal(shortLabel('docs/adr/0002-flatten-on-read-import-on-write'), '0002 flatten on read import on write')
	assert.equal(shortLabel('docs/a_b-c'), 'a b c')
	assert.equal(shortLabel('docs/abcdef', 4), 'abc…')
	// 한글은 글자 단위로 센다.
	assert.equal(shortLabel('docs/환불정책문서', 3), '환불…')
})

test('folderOf 는 마지막 / 앞을 돌려주고, 루트 문서는 / 다', () => {
	assert.equal(folderOf('docs/adr/0001-x'), 'docs/adr')
	assert.equal(folderOf('CONTEXT'), '/')
})

test('buildModel 은 연결, 역할, 폴더, keyword 를 문서마다 모은다', () => {
	const model = buildModel(
		[doc('docs/adr/a'), doc('docs/adr/b'), doc('CONTEXT'), doc('docs/x/lonely')],
		[
			{ from: 'docs/adr/a', to: 'CONTEXT', symbols: ['CONTEXT.문서', 'CONTEXT.topic'] },
			{ from: 'docs/adr/b', to: 'CONTEXT', symbols: ['CONTEXT.문서'] },
			{ from: 'CONTEXT', to: 'docs/adr/a', symbols: ['docs/adr/a.결정'] },
			// 인덱스에 없는 문서를 가리키는 연결은 버린다.
			{ from: 'docs/adr/a', to: 'docs/none', symbols: ['docs/none.x'] },
		],
		{ 'CONTEXT.문서': '문서 정의', 'CONTEXT.topic': 'topic 정의', 'docs/adr/a.결정': '결정 정의' },
	)
	assert.deepEqual(model.folders, ['/', 'docs/adr', 'docs/x'])
	assert.equal(model.edges.length, 3)
	const byPath = new Map(model.nodes.map((n) => [n.path, n]))
	const context = byPath.get('CONTEXT')!
	assert.equal(context.role, 'hub')
	assert.equal(context.folderIndex, 0)
	assert.deepEqual(context.incoming, [
		{ path: 'docs/adr/a', symbols: ['CONTEXT.문서', 'CONTEXT.topic'] },
		{ path: 'docs/adr/b', symbols: ['CONTEXT.문서'] },
	])
	assert.deepEqual(context.outgoing, [{ path: 'docs/adr/a', symbols: ['docs/adr/a.결정'] }])
	assert.deepEqual(context.keywords, [
		{ name: '문서', definition: '문서 정의' },
		{ name: 'topic', definition: 'topic 정의' },
	])
	const a = byPath.get('docs/adr/a')!
	assert.equal(a.role, 'normal')
	assert.equal(a.label, 'a')
	assert.equal(a.folder, 'docs/adr')
	assert.deepEqual(a.keywords, [{ name: '결정', definition: '결정 정의' }])
	assert.equal(byPath.get('docs/x/lonely')!.role, 'orphan')
})

test('keyword 의 주인은 키 앞부분과 가장 길게 맞는 문서다', () => {
	const model = buildModel([doc('a'), doc('a.b')], [], { 'a.b.c': 'c 정의', 'a.d': 'd 정의' })
	const byPath = new Map(model.nodes.map((n) => [n.path, n]))
	assert.deepEqual(byPath.get('a.b')!.keywords, [{ name: 'c', definition: 'c 정의' }])
	assert.deepEqual(byPath.get('a')!.keywords, [{ name: 'd', definition: 'd 정의' }])
})

test('연결이 없으면 허브가 없다', () => {
	const model = buildModel([doc('a'), doc('b')], [], {})
	assert.deepEqual(
		model.nodes.map((n) => n.role),
		['orphan', 'orphan'],
	)
})

test('matches 는 이름, 경로, description 에서 대소문자 없이 찾는다', () => {
	const [node] = buildModel([{ ...doc('docs/Refunds'), description: '환불 정책' }], [], {}).nodes
	assert.equal(matches(node, 'refund'), true)
	assert.equal(matches(node, 'DOCS/'), true)
	assert.equal(matches(node, '환불'), true)
	assert.equal(matches(node, '결제'), false)
	assert.equal(matches(node, ''), true)
})

test('toElements 는 점과 화살표를 만들고 화살표에 심볼 수를 단다', () => {
	const model = buildModel([doc('a'), doc('b')], [{ from: 'a', to: 'b', symbols: ['b.x', 'b.y'] }], {})
	assert.deepEqual(toElements(model), [
		{ group: 'nodes', data: { id: 'a', label: 'a', weight: 0, folderIndex: 0, role: 'normal' } },
		{ group: 'nodes', data: { id: 'b', label: 'b', weight: 1, folderIndex: 0, role: 'hub' } },
		{ group: 'edges', data: { source: 'a', target: 'b', symbols: 'b.x\nb.y', count: 2 } },
	])
})
