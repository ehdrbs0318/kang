// find.ts 가 찾는 말의 위치를 바르게 세는지 검사한다.
import { assert, test } from 'orca/testing'
import { findOffsets, shortcut } from './find'

test('대소문자를 가리지 않고 모든 결과를 찾는다', () => {
	assert.deepEqual(findOffsets('Kang 과 kang 과 KANG', 'kang'), [[0, 4], [7, 11], [14, 18]])
})

test('한글을 찾는다', () => {
	assert.deepEqual(findOffsets('환불 기간 안이면 환불 한다', '환불'), [[0, 2], [10, 12]])
})

test('정규식 문자를 글자 그대로 찾는다', () => {
	assert.deepEqual(findOffsets('a.b axb (a.b)', 'a.b'), [[0, 3], [9, 12]])
})

test('빈 말은 아무것도 찾지 않는다', () => {
	assert.deepEqual(findOffsets('abc', ''), [])
})

test('겹치는 결과는 세지 않는다', () => {
	assert.deepEqual(findOffsets('aaaa', 'aa'), [[0, 2], [2, 4]])
})

/** 수정 키가 모두 꺼진 키 입력을 만든다. */
const key = (code: string, key: string, mods: Partial<Record<'metaKey' | 'ctrlKey' | 'shiftKey' | 'altKey', boolean>> = {}) => ({
	code,
	key,
	metaKey: false,
	ctrlKey: false,
	shiftKey: false,
	altKey: false,
	...mods,
})

test('한글 입력 상태의 Cmd+F 도 막대를 연다', () => {
	assert.equal(shortcut(key('KeyF', 'ㄹ', { metaKey: true }), false), 'open')
	assert.equal(shortcut(key('KeyF', 'f', { ctrlKey: true }), false), 'open')
})

test('Cmd+G 와 Escape 는 막대가 열려 있을 때만 쓴다', () => {
	assert.equal(shortcut(key('KeyG', 'ㅎ', { metaKey: true }), true), 'next')
	assert.equal(shortcut(key('KeyG', 'G', { metaKey: true, shiftKey: true }), true), 'previous')
	assert.equal(shortcut(key('Escape', 'Escape'), true), 'close')
	assert.equal(shortcut(key('KeyG', 'g', { metaKey: true }), false), null)
	assert.equal(shortcut(key('Escape', 'Escape'), false), null)
})

test('수정 키가 없거나 다른 조합의 F 는 넘긴다', () => {
	assert.equal(shortcut(key('KeyF', 'f'), false), null)
	assert.equal(shortcut(key('KeyF', 'f', { metaKey: true, altKey: true }), false), null)
	assert.equal(shortcut(key('KeyF', 'F', { metaKey: true, shiftKey: true }), false), null)
})
