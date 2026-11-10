// separate.ts 의 겹침 제거를 검사한다.
import { assert, test } from 'orca/testing'
import { type Box, separate } from './separate'

/**
 * 옮긴 거리를 적용한 영역 목록을 만든다.
 * @param boxes 처음 영역
 * @param moves `separate` 의 결과
 * @returns 옮긴 뒤의 영역
 */
function apply(boxes: Box[], moves: Map<string, { dx: number; dy: number }>): Box[] {
	return boxes.map((b) => {
		const m = moves.get(b.id) ?? { dx: 0, dy: 0 }
		return { id: b.id, x1: b.x1 + m.dx, y1: b.y1 + m.dy, x2: b.x2 + m.dx, y2: b.y2 + m.dy }
	})
}

/**
 * 두 영역이 간격을 두고 떨어져 있는지 본다.
 * @param a 영역
 * @param b 영역
 * @param gap 최소 간격. 계산 오차를 위해 조금 덜어 본다
 * @returns 떨어져 있으면 true
 */
function apart(a: Box, b: Box, gap: number): boolean {
	return a.x2 + gap <= b.x1 + 1e-6 || b.x2 + gap <= a.x1 + 1e-6 || a.y2 + gap <= b.y1 + 1e-6 || b.y2 + gap <= a.y1 + 1e-6
}

test('겹친 허브 점들을 간격을 두고 떼어 놓는다', () => {
	// 화면의 photo skill, emotions, scene types 처럼 거의 같은 자리에 큰 점 셋이 있다.
	const boxes: Box[] = [
		{ id: 'photo skill', x1: 0, y1: 0, x2: 64, y2: 84 },
		{ id: 'emotions', x1: 10, y1: 5, x2: 74, y2: 89 },
		{ id: 'scene types', x1: -20, y1: 20, x2: 44, y2: 104 },
		{ id: 'same', x1: 0, y1: 0, x2: 64, y2: 84 },
	]
	const out = apply(boxes, separate(boxes, 8, 200))
	for (let i = 0; i < out.length; i++) {
		for (let j = i + 1; j < out.length; j++) assert.ok(apart(out[i], out[j], 8), `${out[i].id} 와 ${out[j].id} 가 겹친다`)
	}
})

test('겹치지 않은 점은 옮기지 않는다', () => {
	const boxes: Box[] = [
		{ id: 'a', x1: 0, y1: 0, x2: 10, y2: 10 },
		{ id: 'b', x1: 100, y1: 0, x2: 110, y2: 10 },
	]
	assert.equal(separate(boxes, 8, 200).size, 0)
})
