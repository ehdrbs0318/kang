// worker 를 mock host 위에서 실행한다. workspace 경로는 이 저장소 루트라서 실제 파일을 읽는다.
import { fileURLToPath } from 'node:url'
import { assert, mockHost, mountWorker, test } from 'orca/testing'
import { mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, truncateSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import * as main from './worker'
import { realFs } from './worker'

/** 이 파일에서 두 단계 위가 저장소 루트다. */
const ROOT = fileURLToPath(new URL('../../', import.meta.url))

/**
 * 로컬 workspace 하나를 갖는 mock host 를 만든다.
 * @param path workspace 루트 경로. 기본값은 저장소 루트다
 * @returns mock host
 */
function host(path = ROOT) {
	return mockHost({
		capabilities: ['workspace:read', 'workspace:write'],
		results: {
			'workspace.list': { workspaces: [{ id: 'w1', kind: 'worktree', name: 'kang', path, branch: 'main', hostId: 'local' }], truncated: false },
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
	const dir = mkdtempSync(join(tmpdir(), 'kang-worker-test-'))
	try {
		writeFileSync(join(dir, 'big.kang'), '')
		// 디스크를 쓰지 않는 sparse 파일로 크기만 키운다.
		truncateSync(join(dir, 'big.kang'), 32 * 1024 * 1024 + 1)
		assert.equal(realFs.read(ROOT, ''), null)
		assert.equal(realFs.read('/dev', 'null'), null)
		assert.equal(realFs.read(dir, 'big.kang'), null)
		assert.ok(realFs.read(ROOT, 'CONTEXT.kang'))
	} finally {
		rmSync(dir, { recursive: true })
	}
})

test('workspace 밖을 가리키는 symlink 는 읽지 않고 열지 않는다', async () => {
	const outside = mkdtempSync(join(tmpdir(), 'kang-worker-outside-'))
	const root = mkdtempSync(join(tmpdir(), 'kang-worker-ws-'))
	let worker: Awaited<ReturnType<typeof mountWorker>> | undefined
	try {
		writeFileSync(join(outside, 'secret.kang'), 'secret')
		writeFileSync(join(root, 'real.kang'), 'real')
		mkdirSync(join(root, '.kang'))
		symlinkSync(join(outside, 'secret.kang'), join(root, 'x.kang'))
		symlinkSync(join(root, 'real.kang'), join(root, 'y.kang'))
		const doc = (path: string) => ({ path, description: '', hash: '0'.repeat(64), scope: [] })
		writeFileSync(join(root, '.kang/graph.kang.json'), JSON.stringify({ kang: 'graph/1', docs: [doc('x'), doc('y')], keywords: [], edges: [] }))
		assert.equal(realFs.read(root, 'x.kang'), null)
		assert.equal(realFs.read(root, 'y.kang'), 'real')
		const h = host(root)
		worker = await mountWorker(main, { host: h })
		assert.deepEqual(await worker.request('kang.open', { workspaceId: 'w1', path: 'x' }), { opened: false })
		assert.deepEqual(await worker.request('kang.open', { workspaceId: 'w1', path: 'y' }), { opened: true })
		assert.deepEqual(h.callsOf('editor.open'), [{ workspaceId: 'w1', path: 'y.kang' }])
	} finally {
		// 검사가 실패해도 worker 를 내리고 임시 폴더를 지운다.
		await worker?.deactivate()
		rmSync(outside, { recursive: true })
		rmSync(root, { recursive: true })
	}
})
