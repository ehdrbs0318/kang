// frame.ts 의 tooltip 위치 계산을 검사한다.
import { assert, test } from 'orca/testing'
import { placeTooltip } from './frame'

const viewport = { width: 800, height: 600 }
const size = { width: 200, height: 100 }

test('아래에 들어가면 링크 아래에 놓는다', () => {
	const pos = placeTooltip({ left: 100, top: 100, right: 160, bottom: 120 }, size, viewport)
	assert.deepEqual(pos, { left: 100, top: 124 })
})

test('아래에 자리가 모자라면 링크 위에 놓는다', () => {
	const pos = placeTooltip({ left: 100, top: 560, right: 160, bottom: 580 }, size, viewport)
	assert.deepEqual(pos, { left: 100, top: 456 })
})

test('오른쪽 끝을 넘으면 왼쪽으로 당긴다', () => {
	const pos = placeTooltip({ left: 750, top: 100, right: 790, bottom: 120 }, size, viewport)
	assert.equal(pos.left, 592)
})

test('왼쪽 끝을 넘으면 오른쪽으로 민다', () => {
	const pos = placeTooltip({ left: -50, top: 100, right: -10, bottom: 120 }, size, viewport)
	assert.equal(pos.left, 8)
})

test('위아래 어느 쪽에도 안 들어가면 화면 안으로 제한한다', () => {
	const pos = placeTooltip({ left: 100, top: 40, right: 160, bottom: 60 }, { width: 200, height: 580 }, viewport)
	assert.equal(pos.top, 12)
})
