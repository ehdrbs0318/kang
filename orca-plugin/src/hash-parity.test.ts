// 저장소 그래프 인덱스의 해시가 worker 의 docHash, frame 의 frameHash 와 같은지 본다.
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { assert, test } from 'orca/testing'
import { frameHash } from './frame'
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

test('frame 의 frameHash 는 worker 의 docHash 와 같다', async () => {
	const index = readFileSync(join(ROOT, '.kang/graph.kang.json'), 'utf8')
	const docs = (JSON.parse(index) as { docs: { path: string }[] }).docs.map((d) => readFileSync(join(ROOT, `${d.path}.kang`), 'utf8'))
	// 실제 문서, 그래프 인덱스, BOM·CRLF·한글·빈 원문을 모두 비교한다.
	for (const text of [...docs, index, '\uFEFFa\r\n\r\n\r\nb  \n', '한글 \u3000\n', '']) {
		assert.equal(await frameHash(text), docHash(text), JSON.stringify(text.slice(0, 40)))
	}
})
