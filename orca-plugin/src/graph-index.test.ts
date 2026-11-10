// graph-index.ts 의 인덱스 검사와 정의 표를 검사한다.
import { assert, test } from 'orca/testing'
import { definitionsOf, parseIndex } from './graph-index'

/**
 * 문서 하나와 keyword 하나가 있는 인덱스 원문을 만든다.
 * @param patch 기본 인덱스에 덮어쓸 필드
 * @returns JSON 원문
 */
function index(patch: Record<string, unknown> = {}): string {
	return JSON.stringify({
		kang: 'graph/1',
		docs: [{ path: 'docs/a', description: 'A', hash: 'h', scope: [] }],
		keywords: [{ path: 'docs/a', name: '결제', definition: '돈을 내는 일' }],
		edges: [],
		...patch,
	})
}

test('parseIndex 는 kang build 의 인덱스를 읽는다', () => {
	const lists = parseIndex(index())
	assert.equal(lists?.docs[0].path, 'docs/a')
	assert.equal(lists?.keywords.length, 1)
})

test('parseIndex 는 깨진 JSON, 다른 표지, 프로젝트 밖 경로, 겹친 경로를 거부한다', () => {
	const doc = (path: string) => ({ path, description: '', hash: 'h', scope: [] })
	for (const text of [
		'{',
		index({ kang: 'graph/2' }),
		index({ docs: [doc('../a')] }),
		index({ docs: [doc('/a')] }),
		index({ docs: [doc('a\\b')] }),
		index({ docs: [doc('a'), doc('a')] }),
		index({ edges: [{ from: 'docs/a', to: 'docs/a', symbols: [] }, { from: 'docs/a', to: 'docs/a', symbols: [] }] }),
	]) {
		assert.equal(parseIndex(text), null, text)
	}
})

test('definitionsOf 는 문서 경로와 이름으로 key 를 만들고 뺄 문서를 뺀다', () => {
	const keywords = [
		{ path: 'docs/a', name: '결제', definition: '돈을 내는 일' },
		{ path: 'docs/b', name: '환불', definition: '돈을 돌려주는 일' },
	]
	assert.deepEqual(definitionsOf(keywords), { 'docs/a.결제': '돈을 내는 일', 'docs/b.환불': '돈을 돌려주는 일' })
	assert.deepEqual(definitionsOf(keywords, new Set(['docs/b'])), { 'docs/a.결제': '돈을 내는 일' })
})
