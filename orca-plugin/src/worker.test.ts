// worker 를 mock host 위에서 실행한다. workspace 경로는 이 저장소 루트라서 실제 파일을 읽는다.
import { fileURLToPath } from 'node:url'
import { assert, mockHost, mountWorker, test } from 'orca/testing'
import { mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, truncateSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import * as main from './worker'
import { realFs } from './worker'
import { docHash } from './locate'

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
	assert.deepEqual(await worker.request('kang.open', { workspaceId: 'w1', root: '', path: 'CONTEXT' }), { opened: true })
	assert.deepEqual(await worker.request('kang.open', { workspaceId: 'w1', root: '', path: '../etc/passwd' }), { opened: false })
	assert.deepEqual(await worker.request('kang.open', { workspaceId: 'nope', root: '', path: 'CONTEXT' }), { opened: false })
	assert.deepEqual(h.callsOf('editor.open'), [{ workspaceId: 'w1', path: 'CONTEXT.kang' }])
	await worker.deactivate()
})

test('realFs 는 보통 파일이 아니거나 32 MiB 보다 큰 파일을 읽지 않는다', () => {
	const dir = mkdtempSync(join(tmpdir(), 'kang-worker-test-'))
	try {
		writeFileSync(join(dir, 'big.kang'), '')
		// 디스크를 쓰지 않는 sparse 파일로 크기만 키운다.
		truncateSync(join(dir, 'big.kang'), 32 * 1024 * 1024 + 1)
		const fs = realFs()
		assert.equal(fs.read(ROOT, ''), null)
		assert.equal(fs.read('/dev', 'null'), null)
		assert.equal(fs.read(dir, 'big.kang'), null)
		assert.ok(fs.read(ROOT, 'CONTEXT.kang'))
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
		assert.equal(realFs().read(root, 'x.kang'), null)
		assert.equal(realFs().read(root, 'y.kang'), 'real')
		const h = host(root)
		worker = await mountWorker(main, { host: h })
		assert.deepEqual(await worker.request('kang.open', { workspaceId: 'w1', root: '', path: 'x' }), { opened: false })
		assert.deepEqual(await worker.request('kang.open', { workspaceId: 'w1', root: '', path: 'y' }), { opened: true })
		assert.deepEqual(h.callsOf('editor.open'), [{ workspaceId: 'w1', path: 'y.kang' }])
	} finally {
		// 검사가 실패해도 worker 를 내리고 임시 폴더를 지운다.
		await worker?.deactivate()
		rmSync(outside, { recursive: true })
		rmSync(root, { recursive: true })
	}
})

test('kang.open 은 하위 저장소의 kang 프로젝트 문서를 workspace 기준 경로로 열고, symlink 디렉토리는 프로젝트로 받지 않는다', async () => {
	const outside = mkdtempSync(join(tmpdir(), 'kang-worker-outside-'))
	const root = mkdtempSync(join(tmpdir(), 'kang-worker-ws-'))
	let worker: Awaited<ReturnType<typeof mountWorker>> | undefined
	try {
		const doc = (path: string) => ({ path, description: '', hash: '0'.repeat(64), scope: [] })
		const index = JSON.stringify({ kang: 'graph/1', docs: [doc('A')], keywords: [], edges: [] })
		// 상위 폴더 workspace 아래의 문서 저장소다.
		mkdirSync(join(root, 'doc'))
		mkdirSync(join(root, 'doc/.kang'))
		writeFileSync(join(root, 'doc/.kang/graph.kang.json'), index)
		writeFileSync(join(root, 'doc/A.kang'), 'a')
		// workspace 밖의 저장소를 가리키는 symlink 디렉토리다.
		mkdirSync(join(outside, '.kang'))
		writeFileSync(join(outside, '.kang/graph.kang.json'), index)
		writeFileSync(join(outside, 'A.kang'), 'secret')
		symlinkSync(outside, join(root, 'link'))
		const h = host(root)
		worker = await mountWorker(main, { host: h })
		assert.deepEqual(await worker.request('kang.open', { workspaceId: 'w1', root: 'doc', path: 'A' }), { opened: true })
		assert.deepEqual(await worker.request('kang.open', { workspaceId: 'w1', root: 'link', path: 'A' }), { opened: false })
		assert.deepEqual(await worker.request('kang.open', { workspaceId: 'w1', root: '..', path: 'A' }), { opened: false })
		assert.deepEqual(h.callsOf('editor.open'), [{ workspaceId: 'w1', path: 'doc/A.kang' }])
	} finally {
		// 검사가 실패해도 worker 를 내리고 임시 폴더를 지운다.
		await worker?.deactivate()
		rmSync(outside, { recursive: true })
		rmSync(root, { recursive: true })
	}
})

test('한 workspace 에서 읽는 파일 크기의 합이 한도를 넘으면 kang.locate 는 그 workspace 를 건너뛴다', async () => {
	const root = mkdtempSync(join(tmpdir(), 'kang-worker-budget-'))
	let worker: Awaited<ReturnType<typeof mountWorker>> | undefined
	try {
		const text = '---\ndescription: d\n---\n\n## t\n'
		writeFileSync(join(root, 'A.kang'), text)
		const doc = (path: string, hash: string) => ({ path, description: '', hash, scope: [] })
		const big = ['b1', 'b2', 'b3', 'b4']
		// 디스크를 쓰지 않는 sparse 파일로 20 MiB 문서 네 개를 만든다. 넷을 다 읽으면 64 MiB 를 넘는다.
		for (const path of big) {
			writeFileSync(join(root, `${path}.kang`), '')
			truncateSync(join(root, `${path}.kang`), 20 * 1024 * 1024)
		}
		mkdirSync(join(root, '.kang'))
		const index = { kang: 'graph/1', docs: [doc('A', docHash(text)), ...big.map((p) => doc(p, '0'.repeat(64)))], keywords: [], edges: [] }
		writeFileSync(join(root, '.kang/graph.kang.json'), JSON.stringify(index))
		worker = await mountWorker(main, { host: host(root) })
		assert.deepEqual(await worker.request('kang.locate', { text }), { status: 'notIndexed' })
		// 문서가 64 MiB 안이면 같은 인덱스로 찾는다.
		truncateSync(join(root, 'b4.kang'), 0)
		assert.equal((await worker.request('kang.locate', { text })).status, 'ok')
	} finally {
		// 검사가 실패해도 worker 를 내리고 임시 폴더를 지운다.
		await worker?.deactivate()
		rmSync(root, { recursive: true })
	}
})

test('읽은 양이 한도를 넘은 workspace 만 건너뛰고 다른 workspace 에서 찾는다', async () => {
	const big = mkdtempSync(join(tmpdir(), 'kang-worker-big-'))
	const small = mkdtempSync(join(tmpdir(), 'kang-worker-small-'))
	let worker: Awaited<ReturnType<typeof mountWorker>> | undefined
	try {
		const text = '---\ndescription: d\n---\n\n## t\n'
		const doc = (path: string, hash: string) => ({ path, description: '', hash, scope: [] })
		const paths = ['b1', 'b2', 'b3', 'b4']
		// big 은 20 MiB sparse 문서 네 개를 가리켜 한도를 넘는다. small 은 문서 하나뿐이다.
		for (const path of paths) {
			writeFileSync(join(big, `${path}.kang`), '')
			truncateSync(join(big, `${path}.kang`), 20 * 1024 * 1024)
		}
		for (const [root, extra] of [[big, paths], [small, []]] as const) {
			// 두 workspace 는 같은 저장소의 worktree 다.
			writeFileSync(join(root, '.git'), `gitdir: /repo/.git/worktrees/${root === big ? 'big' : 'small'}\n`)
			writeFileSync(join(root, 'A.kang'), text)
			mkdirSync(join(root, '.kang'))
			const index = { kang: 'graph/1', docs: [doc('A', docHash(text)), ...extra.map((p) => doc(p, '0'.repeat(64)))], keywords: [], edges: [] }
			writeFileSync(join(root, '.kang/graph.kang.json'), JSON.stringify(index))
		}
		const h = mockHost({
			capabilities: ['workspace:read', 'workspace:write'],
			results: {
				'workspace.list': {
					workspaces: [
						{ id: 'big', kind: 'worktree', name: 'big', path: big, branch: 'main', hostId: 'local' },
						{ id: 'small', kind: 'worktree', name: 'small', path: small, branch: 'main', hostId: 'local' },
					],
					truncated: false,
				},
				'workspace.readContext': null,
			},
		})
		worker = await mountWorker(main, { host: h })
		const result = await worker.request('kang.locate', { text })
		assert.equal(result.status === 'ok' && result.workspaceId, 'small')
	} finally {
		// 검사가 실패해도 worker 를 내리고 임시 폴더를 지운다.
		await worker?.deactivate()
		rmSync(big, { recursive: true })
		rmSync(small, { recursive: true })
	}
})
