// Task 1 이 쓴 저장소의 그래프 인덱스 해시가 TypeScript 의 docHash 와 같은지 본다.
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { assert, test } from 'orca/testing'
import { docHash } from './locate'

/** 이 파일에서 두 단계 위가 저장소 루트다. */
const ROOT = fileURLToPath(new URL('../../', import.meta.url))

test('저장소 그래프 인덱스의 모든 문서 hash 가 원문의 docHash 와 같다', () => {
	const index = JSON.parse(readFileSync(join(ROOT, '.kang/graph.kang.json'), 'utf8')) as { docs: { path: string; hash: string }[] }
	assert.ok(index.docs.length > 0)
	// 문서마다 원문을 읽어 해시를 비교한다.
	for (const doc of index.docs) {
		assert.equal(docHash(readFileSync(join(ROOT, `${doc.path}.kang`), 'utf8')), doc.hash, doc.path)
	}
})
