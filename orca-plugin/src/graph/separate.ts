// 그래프 배치 뒤에 겹친 점을 떼어 놓는다. Gephi 의 Noverlap 과 같이, 겹친 두 사각형을 덜 겹친 축으로 밀어낸다.
// cose 배치는 같은 이웃을 많이 공유하는 허브 점을 서로 끌어당겨 겹치게 둔다. 그러면 점을 누를 수 없다.
// DOM 과 cytoscape 를 읽지 않는 순수 함수라 테스트할 수 있다.

/** 점 하나의 영역이다. 라벨까지 포함한 사각형이고, 좌표는 그래프 모델 좌표다. */
export type Box = { id: string; x1: number; y1: number; x2: number; y2: number }

/**
 * 겹친 사각형들을 서로 밀어내 떼어 놓는다.
 * 겹친 두 사각형마다 겹친 길이가 짧은 축으로, 겹친 길이에 간격을 더한 만큼을 반씩 나눠 민다. 겹침이 없어지거나 반복 수가 다하면 멈춘다.
 * shortcut: 쌍마다 비교하므로 O(n²) 이다. 문서가 수천 개로 늘면 격자로 나눠 가까운 쌍만 비교한다.
 * @param boxes 점마다의 영역
 * @param gap 사각형 사이에 남길 최소 간격
 * @param maxRounds 최대 반복 수
 * @returns 점 id 마다 옮길 거리. 옮기지 않은 점은 넣지 않는다
 */
export function separate(boxes: Box[], gap: number, maxRounds: number): Map<string, { dx: number; dy: number }> {
	const items = boxes.map((b) => ({ id: b.id, x: (b.x1 + b.x2) / 2, y: (b.y1 + b.y2) / 2, w: b.x2 - b.x1, h: b.y2 - b.y1, dx: 0, dy: 0 }))
	// 한 바퀴마다 모든 쌍을 보고, 겹친 쌍이 없으면 끝낸다.
	for (let round = 0; round < maxRounds; round++) {
		let moved = false
		// 쌍마다 겹친 길이를 재고, 겹쳤으면 짧은 축으로 민다.
		for (let i = 0; i < items.length; i++) {
			for (let j = i + 1; j < items.length; j++) {
				const a = items[i]
				const b = items[j]
				const cx = b.x - a.x
				const cy = b.y - a.y
				const ox = (a.w + b.w) / 2 + gap - Math.abs(cx)
				const oy = (a.h + b.h) / 2 + gap - Math.abs(cy)
				// 한 축이라도 떨어져 있으면 겹치지 않는다.
				if (ox <= 0 || oy <= 0) continue
				moved = true
				// 중심이 같으면 방향이 없으므로 순서로 정한다. 그래야 같은 입력에 같은 결과가 나온다.
				const sx = cx === 0 ? (i % 2 === 0 ? -1 : 1) : Math.sign(cx)
				const sy = cy === 0 ? (i % 2 === 0 ? -1 : 1) : Math.sign(cy)
				// 덜 겹친 축으로 밀어야 가장 적게 움직인다.
				if (ox < oy) {
					a.x -= (sx * ox) / 2
					b.x += (sx * ox) / 2
				} else {
					a.y -= (sy * oy) / 2
					b.y += (sy * oy) / 2
				}
			}
		}
		if (!moved) break
	}
	const result = new Map<string, { dx: number; dy: number }>()
	// 처음 자리와 달라진 점만 돌려준다.
	for (const [k, item] of items.entries()) {
		const b = boxes[k]
		const dx = item.x - (b.x1 + b.x2) / 2
		const dy = item.y - (b.y1 + b.y2) / 2
		if (dx !== 0 || dy !== 0) result.set(item.id, { dx, dy })
	}
	return result
}
