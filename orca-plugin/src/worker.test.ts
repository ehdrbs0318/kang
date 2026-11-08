// worker 를 mock host 위에서 실행한다. workspace 경로는 이 저장소 루트라서 실제 파일을 읽는다.
import { fileURLToPath } from 'node:url'
import { assert, mockHost, mountWorker, test } from 'orca/testing'
import { readFileSync, rmSync, truncateSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import * as main from './worker'
import { realFs } from './worker'

/** 이 파일에서 두 단계 위가 저장소 루트다. */
const ROOT = fileURLToPath(new URL('../../', import.meta.url))

/**
 * 저장소 루트를 로컬 workspace 하나로 갖는 mock host 를 만든다.
 * @returns mock host
 */
function host() {
	return mockHost({
		capabilities: ['workspace:read', 'workspace:write'],
		results: {
			'workspace.list': { workspaces: [{ id: 'w1', kind: 'worktree', name: 'kang', path: ROOT, branch: 'main', hostId: 'local' }], truncated: false },
			'workspace.readContext': null,
			'editor.open': { opened: true },
		},
	})
}

test('kang.locate 는 열린 문서의 workspace 와 경로를 답한다', async () => {
	const worker = await mountWorker(main, { host: host() })
	const result = await worker.request('kang.locate', { text: readFileSync(`${ROOT}CONTEXT.kang`, 'utf8') })
	assert.equal(result.status, 'ok')
	assert.equal(result.status === 'ok' && result.path, 'CONTEXT')
	assert.deepEqual(worker.refusals, [])
	await worker.deactivate()
})

test('kang.open 은 인덱스에 있는 문서만 연다', async () => {
	const h = host()
	const worker = await mountWorker(main, { host: h })
	assert.deepEqual(await worker.request('kang.open', { workspaceId: 'w1', path: 'CONTEXT' }), { opened: true })
	assert.deepEqual(await worker.request('kang.open', { workspaceId: 'w1', path: '../etc/passwd' }), { opened: false })
	assert.deepEqual(await worker.request('kang.open', { workspaceId: 'nope', path: 'CONTEXT' }), { opened: false })
	assert.deepEqual(h.callsOf('editor.open'), [{ workspaceId: 'w1', path: 'CONTEXT.kang' }])
	await worker.deactivate()
})

test('realFs 는 보통 파일이 아니거나 32 MiB 보다 큰 파일을 읽지 않는다', () => {
	const big = join(tmpdir(), `kang-worker-test-${Date.now()}.kang`)
	writeFileSync(big, '')
	// 디스크를 쓰지 않는 sparse 파일로 크기만 키운다.
	truncateSync(big, 32 * 1024 * 1024 + 1)
	try {
		assert.equal(realFs.read(ROOT), null)
		assert.equal(realFs.read('/dev/null'), null)
		assert.equal(realFs.read(big), null)
		assert.ok(realFs.read(`${ROOT}CONTEXT.kang`))
	} finally {
		rmSync(big)
	}
})
