// locate.ts 의 정규화, 해시, workspace 찾기를 가짜 파일 시스템으로 검사한다.
import { assert, test } from 'orca/testing'
import { docHash, locate, normalize, pickWorkspace, type Workspace } from './locate'

/**
 * 경로와 내용의 표로 가짜 파일 시스템을 만든다.
 * @param files 절대 경로를 key 로, 파일 내용을 값으로 갖는 표
 * @returns 표에 없는 경로는 null 을 돌려주는 `read`
 */
function fakeFs(files: Record<string, string>) {
	return { read: (path: string) => files[path] ?? null }
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
	const docLines = Object.entries(docs).map(([path, text]) => JSON.stringify({ path, description: path, hash: docHash(text) }))
	const keywordLines = keywords.map((k) => JSON.stringify(k))
	return `{"kang":"graph/1",\n"docs":[\n${docLines.join(',\n')}\n],\n"keywords":[\n${keywordLines.join(',\n')}\n],\n"edges":[\n]}\n`
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
	assert.notEqual(docHash('﻿a'), docHash('a'))
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
	const result = locate(DOC_A, [ws('w1', '/r/a')], null, fs)
	assert.equal(result.status, 'ok')
	if (result.status !== 'ok') return
	assert.equal(result.kind, 'doc')
	assert.equal(result.workspaceId, 'w1')
	assert.equal(result.path, 'docs/A')
	assert.deepEqual(result.docs.map((d) => d.path), ['docs/A', 'B'])
	assert.deepEqual(result.definitions, { 'B.결제.카드': '카드 결제' })
	assert.deepEqual(result.stale, [])
})

test('원문이 그래프 인덱스 파일과 같으면 kind 는 graph 이고 path 는 null 이다', () => {
	const index = graph({ A: DOC_A })
	const fs = fakeFs({ '/r/a/.kang/graph.kang.json': index, '/r/a/A.kang': DOC_A })
	const result = locate(index, [ws('w1', '/r/a')], null, fs)
	assert.equal(result.status, 'ok')
	if (result.status !== 'ok') return
	assert.equal(result.kind, 'graph')
	assert.equal(result.path, null)
})

test('여러 workspace 가 맞으면 focus 된 workspace 의 이름과 branch 로 고른다', () => {
	const index = graph({ A: DOC_A })
	const fs = fakeFs({
		'/r/a/.git/HEAD': 'x',
		'/r/a/.kang/graph.kang.json': index,
		'/r/b/.git/HEAD': 'x',
		'/r/b/.kang/graph.kang.json': index,
	})
	const result = locate(DOC_A, [ws('w1', '/r/a', 'a', 'main'), ws('w2', '/r/b', 'b', 'dev')], { displayName: 'b', branch: 'dev' }, fs)
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
	const result = locate(DOC_A, [ws('w1', '/r/a'), ws('w2', '/r/wt')], { displayName: 'other', branch: 'x' }, fs)
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
	assert.deepEqual(locate(DOC_A, [ws('w1', '/r/a'), ws('w2', '/r/b')], null, fs), { status: 'ambiguous' })
})

test('git 저장소가 아닌 workspace 둘이 맞으면 ambiguous 다', () => {
	const index = graph({ A: DOC_A })
	const fs = fakeFs({ '/r/a/.kang/graph.kang.json': index, '/r/b/.kang/graph.kang.json': index })
	assert.deepEqual(locate(DOC_A, [ws('w1', '/r/a'), ws('w2', '/r/b')], null, fs), { status: 'ambiguous' })
})

test('원격 workspace 만 있으면 remoteOnly 다', () => {
	const remote = { ...ws('w1', '/r/a'), hostId: 'ssh-1' }
	assert.deepEqual(locate(DOC_A, [remote], null, fakeFs({})), { status: 'remoteOnly' })
})

test('로컬 workspace 가 맞지 않으면 원격 workspace 가 있어도 notIndexed 다', () => {
	const remote = { ...ws('w2', '/r/b'), hostId: 'ssh-1' }
	assert.deepEqual(locate(DOC_A, [ws('w1', '/r/a'), remote], null, fakeFs({})), { status: 'notIndexed' })
})

test('인덱스가 없거나 JSON 이 깨졌거나 표지가 다른 workspace 는 건너뛴다', () => {
	const fs = fakeFs({
		'/r/broken/.kang/graph.kang.json': '{"kang":"graph/1",\n"docs":[\n',
		'/r/other/.kang/graph.kang.json': '{"kang":"other","docs":[],"keywords":[],"edges":[]}',
		'/r/good/.kang/graph.kang.json': graph({ A: DOC_A }),
	})
	const list = [ws('w0', '/r/none'), ws('w1', '/r/broken'), ws('w2', '/r/other'), ws('w3', '/r/good')]
	const result = locate(DOC_A, list, null, fs)
	assert.equal(result.status === 'ok' && result.workspaceId, 'w3')
	assert.deepEqual(locate(DOC_A, list.slice(0, 3), null, fs), { status: 'notIndexed' })
})

test('지금 해시가 인덱스와 다르거나 파일이 없는 문서는 stale 이다', () => {
	const fs = fakeFs({
		'/r/a/.kang/graph.kang.json': graph({ A: DOC_A, B: DOC_B, C: '# C\n' }),
		'/r/a/A.kang': DOC_A,
		'/r/a/B.kang': '# B 문서 바뀜\n',
	})
	const result = locate(DOC_A, [ws('w1', '/r/a')], null, fs)
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
	const result = locate(DOC_A, [ws('w1', '/r/a')], null, fs)
	assert.deepEqual(result.status === 'ok' && result.definitions, { 'A.가': '가 정의' })
})

test('pickWorkspace 는 후보가 하나면 그 후보를 고른다', () => {
	const candidate = { workspace: ws('w1', '/r/a'), path: 'A', gitCommonDir: null }
	assert.equal(pickWorkspace([candidate], null), candidate)
})
