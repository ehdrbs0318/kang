// parse.ts 가 kang 원문을 화면용 블록으로 바꾸는지, 참조를 주소로 푸는지 검사한다.
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { assert, test } from 'orca/testing'
import { marked } from '../../vendor/marked.esm.js'
import { parseKang, resolveRef, type KangView } from './parse'

/** 이 파일에서 세 단계 위가 저장소 루트다. */
const ROOT = fileURLToPath(new URL('../../../', import.meta.url))

/**
 * 저장소의 kang 문서 원문을 읽는다.
 * @param path 확장자를 뺀 문서 경로
 * @returns 원문
 */
function source(path: string): string {
	return readFileSync(join(ROOT, `${path}.kang`), 'utf8')
}

/**
 * 모든 markdown 블록의 텍스트를 이어 붙인다.
 * @param view parseKang 결과
 * @returns markdown 블록 텍스트를 줄바꿈으로 이은 문자열
 */
function markdown(view: KangView): string {
	return view.blocks.map((b) => (b.kind === 'markdown' ? b.text : '')).join('\n')
}

test('ADR-0002 에서 description, import 3개, 본문 링크를 만든다', () => {
	const view = parseKang(source('docs/adr/0002-flatten-on-read-import-on-write'))
	assert.ok(view.description.startsWith('ADR-0002: 읽을 때는 평탄화'))
	assert.deepEqual(view.imports, [
		{ target: 'CONTEXT.topic', doc: 'CONTEXT', local: 'topic' },
		{ target: 'CONTEXT.owner', doc: 'CONTEXT', local: 'owner' },
		{ target: 'CONTEXT.문서', doc: 'CONTEXT', local: '문서' },
	])
	const text = markdown(view)
	assert.ok(text.includes('<a class="kang-ref" data-ref="문서">문서</a>'))
	assert.ok(text.includes('<a class="kang-ref" data-ref="owner">owner</a>'))
	// frontmatter 와 import 줄은 본문에 남지 않는다.
	assert.ok(!text.includes('description:'))
	assert.ok(!text.includes('rev "'))
	assert.ok(!text.includes('`'))
})

test('CONTEXT 에서 keyword 블록과 줄 끝 #topic 링크를 만든다', () => {
	const view = parseKang(source('CONTEXT'))
	assert.ok(view.keywords.includes('문서'))
	assert.ok(view.keywords.includes('rev 핀'))
	const keyword = view.blocks.find((b) => b.kind === 'keyword' && b.name === 'keyword')
	assert.ok(keyword && keyword.kind === 'keyword')
	assert.equal(keyword.definition, '그 프로젝트의 도메인에서만 쓰는 특수 용어. 반드시 한 줄 정의가 있고, 계층을 가질 수 있다')
	assert.equal(keyword.refs, '<a class="kang-ref" data-ref="#무엇을 keyword 로 선언하나">무엇을 keyword 로 선언하나</a>')
	const layered = view.blocks.find((b) => b.kind === 'keyword' && b.name === '계층 keyword')
	assert.ok(layered && layered.kind === 'keyword')
	// 정의 안의 백틱도 참조 링크다. 줄 끝 #topic 이 없으면 refs 는 비어 있다.
	assert.ok(layered.definition.includes('<a class="kang-ref" data-ref="keyword">keyword</a>'))
	assert.equal(layered.refs, '')
})

test('계층 keyword 이름은 . 으로 잇는다', () => {
	const view = parseKang('---\ndescription: d\n---\n\nkeyword `결제수단`.`카드`: 카드 결제\n')
	assert.deepEqual(view.keywords, ['결제수단.카드'])
})

test('코드 펜스 안의 백틱은 링크가 되지 않고, \\` 는 백틱 문자로 남는다', () => {
	const view = parseKang('---\ndescription: d\n---\n\n## t\n\n```\nimport `x`\n```\n\n글자 \\` 하나\n')
	const text = markdown(view)
	assert.ok(text.includes('```\nimport `x`\n```'))
	assert.ok(!text.includes('kang-ref'))
	assert.ok(marked.parse(text, { async: false }).includes('글자 ` 하나'))
})

test('uncoded, iknow, exception, cover, pending 의 원본 기호가 출력에 남지 않는다', () => {
	const view = parseKang(
		[
			'---',
			'description: d',
			'---',
			'',
			'keyword `금액`: 원화 액수 // iknow `docs`/`B`.`금액`',
			'',
			'## 검토 절차 // uncoded',
			'',
			'## 청구 // iknow `docs`/`B`#`청구`',
			'',
			'본문',
			'exception `무료 청구서`',
			'exception `해외 결제` pending',
			'cover `무료상품 예외`',
		].join('\n'),
	)
	const html = view.blocks.map((b) => (b.kind === 'markdown' ? b.text : b.definition + b.refs)).join('\n')
	for (const raw of ['//', 'iknow `', 'exception `', 'cover `', '` pending', '`']) assert.ok(!html.includes(raw), raw)
	assert.ok(html.includes('kang-badge'))
	assert.ok(html.includes('<a class="kang-ref" data-ref="docs/B.금액">docs/B.금액</a>'))
	assert.ok(html.includes('<a class="kang-ref" data-ref="무료상품 예외">무료상품 예외</a>'))
	assert.ok(html.includes('pending'))
})

test('본문의 HTML 은 글자로 남는다', () => {
	const text = markdown(parseKang('---\ndescription: d\n---\n\n## t\n\n<script>x</script> & y\n'))
	assert.ok(text.includes('&lt;script>x&lt;/script> &amp; y'))
})

test('resolveRef 는 alias, import 의 마지막 이름, 자기 keyword, 자기 topic 을 주소로 바꾼다', () => {
	const view = parseKang(
		[
			'---',
			'description: d',
			'---',
			'',
			'import `docs`/`A`.`결제` as `A 결제` rev "a3f9c1"',
			'import `CONTEXT`.`문서` rev "f044b7"',
			'',
			'keyword `결제수단`.`카드`: 카드 결제',
		].join('\n'),
	)
	assert.deepEqual(resolveRef(view, 'docs/B', 'A 결제'), { address: 'docs/A.결제', doc: 'docs/A' })
	assert.deepEqual(resolveRef(view, 'docs/B', '문서'), { address: 'CONTEXT.문서', doc: 'CONTEXT' })
	assert.deepEqual(resolveRef(view, 'docs/B', '결제수단.카드'), { address: 'docs/B.결제수단.카드', doc: 'docs/B' })
	assert.deepEqual(resolveRef(view, 'docs/B', '#결제의 방법'), { address: 'docs/B#결제의 방법', doc: 'docs/B' })
	assert.deepEqual(resolveRef(view, 'docs/B', 'docs/A!예외'), { address: 'docs/A!예외', doc: 'docs/A' })
	assert.deepEqual(resolveRef(view, 'docs/B', 'A 결제.하위'), { address: 'docs/A.결제.하위', doc: 'docs/A' })
	assert.equal(resolveRef(view, 'docs/B', '모르는 이름'), null)
})
