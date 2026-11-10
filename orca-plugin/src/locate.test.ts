// locate.ts 의 정규화, 해시, workspace 찾기를 가짜 파일 시스템으로 검사한다.
import { join } from 'node:path'
import { assert, test } from 'orca/testing'
import { docHash, isLocal, locate, pickWorkspace, ReadLimit, type Workspace } from './locate'
import { normalize } from './normalize'

/**
 * 경로와 내용의 표로 가짜 파일 시스템을 만든다.
 * @param files 절대 경로를 key 로, 파일 내용을 값으로 갖는 표
 * @returns 루트와 상대 경로를 이은 경로가 표에 없으면 null 을 돌려주는 `read`
 */
function fakeFs(files: Record<string, string>) {
	return {
		read: (root: string, path: string) => files[join(root, path)] ?? null,
		// 표의 경로에서 루트 바로 아래의 디렉토리 이름을 모은다.
		dirs: (root: string) => [...new Set(Object.keys(files).filter((f) => f.startsWith(`${root}/`)).map((f) => f.slice(root.length + 1).split('/')).filter((parts) => parts.length > 1 && !parts[0].startsWith('.')).map((parts) => parts[0]))],
	}
}

/**
 * 로컬 workspace 를 만든다.
 * @param id workspace id
 * @param path workspace 루트 경로
 * @param name 표시 이름
 * @param branch branch 이름
 * @returns `workspace.list` 의 한 항목
 */
function ws(id: string, path: string, name = id, branch: string | null = 'main'): Workspace {
	return { id, kind: 'worktree', name, path, branch, hostId: null }
}

/**
 * 문서 원문들로 그래프 인덱스 원문을 만든다. Task 1 의 형식처럼 한 항목을 한 줄에 쓴다.
 * @param docs 문서 경로를 key 로, 원문을 값으로 갖는 표
 * @param keywords keyword 목록
 * @returns 그래프 인덱스 파일 원문
 */
function graph(docs: Record<string, string>, keywords: { path: string; name: string; definition: string }[] = []) {
	const docLines = Object.entries(docs).map(([path, text]) => JSON.stringify({ path, description: path, hash: docHash(text), scope: SCOPE[path] ?? [] }))
	const keywordLines = keywords.map((k) => JSON.stringify(k))
	return `{"kang":"graph/1",\n"docs":[\n${docLines.join(',\n')}\n],\n"keywords":[\n${keywordLines.join(',\n')}\n],\n"edges":[\n]}\n`
}

/** 테스트 인덱스의 문서별 scope 다. 없는 문서는 빈 목록이다. */
const SCOPE: Record<string, { local: string; address: string; doc: string }[]> = {
	'docs/A': [{ local: '카드', address: 'B.결제.카드', doc: 'B' }],
}

const DOC_A = '# A 문서\n\n본문\n'
const DOC_B = '# B 문서\n'

test('normalize 는 줄 끝 공백을 지우고 빈 줄을 합치고 앞뒤 공백을 지운다', () => {
	assert.equal(normalize('  \n\na  \t\n\n\n\nb\n\n'), 'a\n\nb')
	assert.equal(normalize(''), '')
})

test('normalize 는 CRLF 의 CR 을 지운다', () => {
	assert.equal(normalize('a\r\nb\r\n\r\n\r\nc\r\n'), 'a\nb\n\nc')
	assert.equal(docHash('a\r\nb\r\n'), docHash('a\nb'))
})

test('normalize 는 BOM 을 공백으로 보지 않고 Rust 의 공백 문자는 지운다', () => {
	assert.equal(normalize('﻿a﻿\n'), '﻿a﻿')
	assert.equal(normalize('a 　\u0085 \nb'), 'a\nb')
})

test('docHash 는 맨 앞 BOM 하나만 떼고 계산한다', () => {
	assert.equal(docHash('\uFEFFa\n'), docHash('a\n'))
	assert.notEqual(docHash('\uFEFF\uFEFFa'), docHash('a'))
})

test('docHash 는 정규화한 텍스트의 SHA-256 hex 64 자다', () => {
	// 빈 문자열의 SHA-256 이다.
	assert.equal(docHash('  \n'), 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855')
})

test('문서가 하나의 workspace 에만 맞으면 그 workspace 와 문서를 고른다', () => {
	const fs = fakeFs({
		'/r/a/.git/HEAD': 'ref: refs/heads/main\n',
		'/r/a/.kang/graph.kang.json': graph({ 'docs/A': DOC_A, B: DOC_B }, [{ path: 'B', name: '결제.카드', definition: '카드 결제' }]),
		'/r/a/docs/A.kang': DOC_A,
		'/r/a/B.kang': DOC_B,
	})
	const result = locate(docHash(DOC_A), [ws('w1', '/r/a')], null, fs)
	assert.equal(result.status, 'ok')
	if (result.status !== 'ok') return
	assert.equal(result.kind, 'doc')
	assert.equal(result.workspaceId, 'w1')
	assert.equal(result.root, '')
	assert.equal(result.path, 'docs/A')
	assert.deepEqual(result.docs.map((d) => d.path), ['docs/A', 'B'])
	assert.deepEqual(result.definitions, { 'B.결제.카드': '카드 결제' })
	assert.deepEqual(result.scope, [{ local: '카드', address: 'B.결제.카드', doc: 'B' }])
	assert.deepEqual(result.stale, [])
})

test('workspace 바로 아래의 git 저장소에 있는 kang 프로젝트를 찾는다', () => {
	const fs = fakeFs({
		'/r/top/app/src/main.ts': '',
		'/r/top/doc/.git/HEAD': 'ref: refs/heads/main\n',
		'/r/top/doc/.kang/graph.kang.json': graph({ 'docs/A': DOC_A, B: DOC_B }, [{ path: 'B', name: '결제.카드', definition: '카드 결제' }]),
		'/r/top/doc/docs/A.kang': DOC_A,
		'/r/top/doc/B.kang': DOC_B,
	})
	const result = locate(docHash(DOC_A), [ws('w1', '/r/top')], null, fs)
	assert.equal(result.status, 'ok')
	if (result.status !== 'ok') return
	assert.equal(result.workspaceId, 'w1')
	assert.equal(result.root, 'doc')
	assert.equal(result.path, 'docs/A')
	assert.deepEqual(result.definitions, { 'B.결제.카드': '카드 결제' })
	assert.deepEqual(result.scope, [{ local: '카드', address: 'B.결제.카드', doc: 'B' }])
	assert.deepEqual(result.stale, [])
})

test('git 저장소가 아닌 하위 디렉토리의 인덱스는 보지 않는다', () => {
	const fs = fakeFs({ '/r/top/doc/.kang/graph.kang.json': graph({ A: DOC_A }), '/r/top/doc/A.kang': DOC_A })
	assert.deepEqual(locate(docHash(DOC_A), [ws('w1', '/r/top')], null, fs), { status: 'notIndexed' })
})

test('하위 저장소를 따로 연 workspace 와 상위 폴더 workspace 가 함께 맞으면 같은 저장소이므로 첫 번째를 고른다', () => {
	const fs = fakeFs({
		'/r/top/doc/.git/HEAD': 'ref: refs/heads/main\n',
		'/r/top/doc/.kang/graph.kang.json': graph({ A: DOC_A }),
		'/r/top/doc/A.kang': DOC_A,
	})
	const result = locate(docHash(DOC_A), [ws('w1', '/r/top'), ws('w2', '/r/top/doc')], null, fs)
	assert.equal(result.status === 'ok' && `${result.workspaceId}:${result.root}`, 'w1:doc')
})

test('원문이 그래프 인덱스 파일과 같으면 kind 는 graph 이고 path 는 null 이다', () => {
	const index = graph({ A: DOC_A })
	const fs = fakeFs({ '/r/a/.kang/graph.kang.json': index, '/r/a/A.kang': DOC_A })
	const result = locate(docHash(index), [ws('w1', '/r/a')], null, fs)
	assert.equal(result.status, 'ok')
	if (result.status !== 'ok') return
	assert.equal(result.kind, 'graph')
	assert.equal(result.path, null)
	assert.deepEqual(result.scope, [])
})

test('한 workspace 안에서 같은 해시의 문서가 둘이면 ambiguous 다', () => {
	const fs = fakeFs({ '/r/a/.git/HEAD': 'x', '/r/a/.kang/graph.kang.json': graph({ A: DOC_A, C: DOC_A }) })
	assert.deepEqual(locate(docHash(DOC_A), [ws('w1', '/r/a')], null, fs), { status: 'ambiguous' })
})

test('항목의 타입이 틀린 인덱스는 없는 것으로 보고, 다른 workspace 는 그대로 찾는다', () => {
	const fs = fakeFs({
		'/r/bad/.kang/graph.kang.json': '{"kang":"graph/1","docs":[null],"keywords":[],"edges":[]}',
		'/r/bad2/.kang/graph.kang.json': JSON.stringify({ kang: 'graph/1', docs: [{ path: 1, description: '', hash: docHash(DOC_A), scope: [] }], keywords: [], edges: [] }),
		'/r/bad3/.kang/graph.kang.json': JSON.stringify({ kang: 'graph/1', docs: [{ path: 'A', description: '', hash: docHash(DOC_A), scope: [{ local: 1 }] }], keywords: [], edges: [] }),
		'/r/good/.kang/graph.kang.json': graph({ A: DOC_A }),
	})
	const bad = [ws('w1', '/r/bad'), ws('w2', '/r/bad2'), ws('w3', '/r/bad3')]
	const result = locate(docHash(DOC_A), [...bad, ws('w4', '/r/good')], null, fs)
	assert.equal(result.status === 'ok' && result.workspaceId, 'w4')
	assert.deepEqual(locate(docHash(DOC_A), bad, null, fs), { status: 'notIndexed' })
})

test('workspace 밖을 가리키는 경로가 하나라도 있는 인덱스는 없는 것으로 본다', () => {
	const doc = (path: string, scope: unknown[] = []) => ({ path, description: '', hash: docHash(path === 'A' ? DOC_A : DOC_B), scope })
	const bad = ['', '/etc/passwd', '../x', 'a/../../x', './A', 'a//b', 'a\\b', 'a/.']
	const indexes = [
		...bad.map((p) => ({ docs: [doc('A'), doc(p)], keywords: [], edges: [] })),
		{ docs: [doc('A', [{ local: 'x', address: 'x', doc: '../x' }])], keywords: [], edges: [] },
		{ docs: [doc('A')], keywords: [{ path: '../x', name: 'n', definition: 'd' }], edges: [] },
		{ docs: [doc('A')], keywords: [], edges: [{ from: 'A', to: '/x', symbols: [] }] },
		{ docs: [doc('A')], keywords: [], edges: [{ from: '..', to: 'A', symbols: [] }] },
	]
	for (const index of indexes) {
		const fs = fakeFs({ '/r/a/.kang/graph.kang.json': JSON.stringify({ kang: 'graph/1', ...index }), '/r/a/A.kang': DOC_A })
		assert.deepEqual(locate(docHash(DOC_A), [ws('w1', '/r/a')], null, fs), { status: 'notIndexed' }, JSON.stringify(index))
	}
})

test('같은 문서 경로가 두 번 나오는 인덱스는 없는 것으로 본다', () => {
	const doc = { path: 'A', description: '', hash: docHash(DOC_A), scope: [] }
	const fs = fakeFs({ '/r/a/.kang/graph.kang.json': JSON.stringify({ kang: 'graph/1', docs: [doc, doc], keywords: [], edges: [] }) })
	assert.deepEqual(locate(docHash(DOC_A), [ws('w1', '/r/a')], null, fs), { status: 'notIndexed' })
})

test('같은 (from, to) 쌍의 관계가 두 번 나오는 인덱스는 없는 것으로 본다', () => {
	const doc = (path: string) => ({ path, description: '', hash: docHash(path === 'A' ? DOC_A : DOC_B), scope: [] })
	const edge = { from: 'A', to: 'B', symbols: [] }
	const fs = fakeFs({ '/r/a/.kang/graph.kang.json': JSON.stringify({ kang: 'graph/1', docs: [doc('A'), doc('B')], keywords: [], edges: [edge, edge] }) })
	assert.deepEqual(locate(docHash(DOC_A), [ws('w1', '/r/a')], null, fs), { status: 'notIndexed' })
})

test('문서가 10 000 개를 넘는 인덱스는 없는 것으로 본다', () => {
	const docs = (count: number) => [
		{ path: 'A', description: '', hash: docHash(DOC_A), scope: [] },
		...Array.from({ length: count - 1 }, (_, i) => ({ path: `d${i}`, description: '', hash: '0'.repeat(64), scope: [] })),
	]
	for (const [count, status] of [[10000, 'ok'], [10001, 'notIndexed']] as const) {
		const fs = fakeFs({ '/r/a/.kang/graph.kang.json': JSON.stringify({ kang: 'graph/1', docs: docs(count), keywords: [], edges: [] }), '/r/a/A.kang': DOC_A })
		assert.equal(locate(docHash(DOC_A), [ws('w1', '/r/a')], null, fs).status, status, String(count))
	}
})

test('로컬 후보가 없고 focus 된 workspace 가 원격이면 remoteOnly 다', () => {
	const remote = { ...ws('w2', '/r/b', 'b', 'dev'), hostId: 'ssh-1' }
	assert.deepEqual(locate(docHash(DOC_A), [ws('w1', '/r/a'), remote], { displayName: 'b', branch: 'dev' }, fakeFs({})), { status: 'remoteOnly' })
})

test('여러 workspace 가 맞으면 focus 된 workspace 의 이름과 branch 로 고른다', () => {
	const index = graph({ A: DOC_A })
	const fs = fakeFs({
		'/r/a/.git/HEAD': 'x',
		'/r/a/.kang/graph.kang.json': index,
		'/r/b/.git/HEAD': 'x',
		'/r/b/.kang/graph.kang.json': index,
	})
	const result = locate(docHash(DOC_A), [ws('w1', '/r/a', 'a', 'main'), ws('w2', '/r/b', 'b', 'dev')], { displayName: 'b', branch: 'dev' }, fs)
	assert.equal(result.status === 'ok' && result.workspaceId, 'w2')
})

test('같은 git 저장소의 worktree 둘이 맞으면 첫 번째를 고르고 정의를 준다', () => {
	const index = graph({ A: DOC_A }, [{ path: 'A', name: '문서', definition: '정의' }])
	const fs = fakeFs({
		'/r/a/.git/HEAD': 'x',
		'/r/a/.kang/graph.kang.json': index,
		'/r/a/A.kang': DOC_A,
		'/r/wt/.git': 'gitdir: ../a/.git/worktrees/wt\n',
		'/r/wt/.kang/graph.kang.json': index,
		'/r/wt/A.kang': DOC_A,
	})
	const result = locate(docHash(DOC_A), [ws('w1', '/r/a'), ws('w2', '/r/wt')], { displayName: 'other', branch: 'x' }, fs)
	assert.equal(result.status, 'ok')
	if (result.status !== 'ok') return
	assert.equal(result.workspaceId, 'w1')
	assert.deepEqual(result.definitions, { 'A.문서': '정의' })
})

test('다른 저장소 둘이 맞고 focus 로 고를 수 없으면 ambiguous 다', () => {
	const index = graph({ A: DOC_A })
	const fs = fakeFs({
		'/r/a/.git/HEAD': 'x',
		'/r/a/.kang/graph.kang.json': index,
		'/r/b/.git/HEAD': 'x',
		'/r/b/.kang/graph.kang.json': index,
	})
	assert.deepEqual(locate(docHash(DOC_A), [ws('w1', '/r/a'), ws('w2', '/r/b')], null, fs), { status: 'ambiguous' })
})

test('git 저장소가 아닌 workspace 둘이 맞으면 ambiguous 다', () => {
	const index = graph({ A: DOC_A })
	const fs = fakeFs({ '/r/a/.kang/graph.kang.json': index, '/r/b/.kang/graph.kang.json': index })
	assert.deepEqual(locate(docHash(DOC_A), [ws('w1', '/r/a'), ws('w2', '/r/b')], null, fs), { status: 'ambiguous' })
})

test('원격 workspace 만 있으면 remoteOnly 다', () => {
	const remote = { ...ws('w1', '/r/a'), hostId: 'ssh-1' }
	assert.deepEqual(locate(docHash(DOC_A), [remote], null, fakeFs({})), { status: 'remoteOnly' })
})

test('hostId 가 null 이나 local 이면 로컬이고, 그 밖은 원격이다', () => {
	assert.equal(isLocal(ws('w1', '/r/a')), true)
	assert.equal(isLocal({ ...ws('w1', '/r/a'), hostId: 'local' }), true)
	assert.equal(isLocal({ ...ws('w1', '/r/a'), hostId: 'ssh-1' }), false)
})

test('hostId 가 local 인 workspace 의 문서를 찾는다', () => {
	const fs = fakeFs({ '/r/a/.kang/graph.kang.json': graph({ A: DOC_A }), '/r/a/A.kang': DOC_A })
	const found = locate(docHash(DOC_A), [{ ...ws('w1', '/r/a'), hostId: 'local' }], null, fs)
	assert.equal(found.status, 'ok')
	assert.equal(found.status === 'ok' && found.path, 'A')
})

test('로컬 workspace 가 맞지 않으면 원격 workspace 가 있어도 notIndexed 다', () => {
	const remote = { ...ws('w2', '/r/b'), hostId: 'ssh-1' }
	assert.deepEqual(locate(docHash(DOC_A), [ws('w1', '/r/a'), remote], null, fakeFs({})), { status: 'notIndexed' })
})

test('인덱스가 없거나 JSON 이 깨졌거나 표지가 다른 workspace 는 건너뛴다', () => {
	const fs = fakeFs({
		'/r/broken/.kang/graph.kang.json': '{"kang":"graph/1",\n"docs":[\n',
		'/r/other/.kang/graph.kang.json': '{"kang":"other","docs":[],"keywords":[],"edges":[]}',
		'/r/good/.kang/graph.kang.json': graph({ A: DOC_A }),
	})
	const list = [ws('w0', '/r/none'), ws('w1', '/r/broken'), ws('w2', '/r/other'), ws('w3', '/r/good')]
	const result = locate(docHash(DOC_A), list, null, fs)
	assert.equal(result.status === 'ok' && result.workspaceId, 'w3')
	assert.deepEqual(locate(docHash(DOC_A), list.slice(0, 3), null, fs), { status: 'notIndexed' })
})

test('지금 해시가 인덱스와 다르거나 파일이 없는 문서는 stale 이다', () => {
	const fs = fakeFs({
		'/r/a/.kang/graph.kang.json': graph({ A: DOC_A, B: DOC_B, C: '# C\n' }),
		'/r/a/A.kang': DOC_A,
		'/r/a/B.kang': '# B 문서 바뀜\n',
	})
	const result = locate(docHash(DOC_A), [ws('w1', '/r/a')], null, fs)
	assert.deepEqual(result.status === 'ok' && result.stale, ['B', 'C'])
})

test('stale 문서가 선언한 keyword 는 정의에서 뺀다', () => {
	const fs = fakeFs({
		'/r/a/.kang/graph.kang.json': graph({ A: DOC_A, B: DOC_B }, [
			{ path: 'A', name: '가', definition: '가 정의' },
			{ path: 'B', name: '나', definition: '나 정의' },
		]),
		'/r/a/A.kang': DOC_A,
		'/r/a/B.kang': '# B 문서 바뀜\n',
	})
	const result = locate(docHash(DOC_A), [ws('w1', '/r/a')], null, fs)
	assert.deepEqual(result.status === 'ok' && result.definitions, { 'A.가': '가 정의' })
})

/**
 * 같은 저장소의 worktree 둘이 있는 가짜 파일 시스템을 만든다. 두 worktree 모두 문서 A, B 를 가진다.
 * @param limited 문서 파일을 읽으면 ReadLimit 을 던지는 루트들
 * @returns 파일 시스템과, 문서 파일(`.kang`)을 읽은 (루트, 경로) 기록
 */
function twoWorktrees(limited: string[] = []) {
	const index = graph({ A: DOC_A, B: DOC_B })
	const base = fakeFs({
		'/r/a/.git': 'gitdir: /r/main/.git/worktrees/a\n',
		'/r/b/.git': 'gitdir: /r/main/.git/worktrees/b\n',
		'/r/a/.kang/graph.kang.json': index,
		'/r/b/.kang/graph.kang.json': index,
		'/r/a/A.kang': DOC_A,
		'/r/a/B.kang': DOC_B,
		'/r/b/A.kang': DOC_A,
		'/r/b/B.kang': DOC_B,
	})
	const docReads: string[] = []
	const fs = {
		dirs: base.dirs,
		read(root: string, path: string) {
			if (!path.endsWith('.kang') || path.startsWith('.kang/')) return base.read(root, path)
			docReads.push(`${root}:${path}`)
			// 한도가 걸린 루트의 문서를 읽으면 읽은 양이 한도를 넘은 것이다.
			if (limited.includes(root)) throw new ReadLimit()
			return base.read(root, path)
		},
	}
	return { fs, docReads }
}

test('같은 저장소의 worktree 둘이 맞아도 고른 workspace 의 문서만 읽는다', () => {
	const { fs, docReads } = twoWorktrees()
	const result = locate(docHash(DOC_A), [ws('w1', '/r/a'), ws('w2', '/r/b')], null, fs)
	assert.equal(result.status === 'ok' && result.workspaceId, 'w1')
	assert.deepEqual(docReads, ['/r/a:A.kang', '/r/a:B.kang'])
})

test('읽은 양이 한도를 넘은 후보는 버리고 나머지에서 다시 골라 답한다', () => {
	const { fs } = twoWorktrees(['/r/a'])
	const result = locate(docHash(DOC_A), [ws('w1', '/r/a'), ws('w2', '/r/b')], null, fs)
	assert.equal(result.status === 'ok' && result.workspaceId, 'w2')
})

test('버린 후보가 focus 된 workspace 이면 다른 저장소를 고르지 않고 notIndexed 다', () => {
	const { fs } = twoWorktrees(['/r/a'])
	const focused = { displayName: 'w1', branch: 'main' }
	const list = [ws('w1', '/r/a'), ws('w2', '/r/b', 'w2', 'dev')]
	assert.deepEqual(locate(docHash(DOC_A), list, focused, fs), { status: 'notIndexed' })
})

test('pickWorkspace 는 후보가 하나면 그 후보를 고른다', () => {
	const candidate = { workspace: ws('w1', '/r/a'), root: '', path: 'A', gitCommonDir: null }
	assert.equal(pickWorkspace([candidate], null), candidate)
})

test('normalize 는 공백 뒤에 글자가 오는 긴 줄도 100ms 안에 끝낸다', () => {
	const line = `${' '.repeat(40000)}x`
	const started = performance.now()
	assert.equal(normalize(`${line}\n${line}  `), `x\n${line}`)
	const elapsed = performance.now() - started
	assert.ok(elapsed < 100, `${elapsed}ms`)
})

test('.git 파일의 gitdir 줄에 긴 공백이 있어도 100ms 안에 같은 저장소로 본다', () => {
	const index = graph({ A: DOC_A })
	const fs = fakeFs({
		'/r/a/.git/HEAD': 'x',
		'/r/a/.kang/graph.kang.json': index,
		'/r/a/A.kang': DOC_A,
		'/r/wt/.git': `gitdir: ../a/.git/worktrees/wt${' '.repeat(20000)}x\n`,
		'/r/wt/.kang/graph.kang.json': index,
		'/r/wt/A.kang': DOC_A,
	})
	const started = performance.now()
	const result = locate(docHash(DOC_A), [ws('w1', '/r/a'), ws('w2', '/r/wt')], null, fs)
	const elapsed = performance.now() - started
	assert.ok(elapsed < 100, `${elapsed}ms`)
	assert.equal(result.status === 'ok' && result.workspaceId, 'w1')
})
