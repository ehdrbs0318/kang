// parse.ts 가 kang 원문을 화면용으로 나누고 그리는지, 참조를 scope 에서 찾는지 검사한다.
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { assert, test } from 'orca/testing'
import { lookupRef, parseKang, renderBody } from './parse'

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
 * frontmatter 를 붙인 원문의 본문을 그린다.
 * @param body frontmatter 뒤에 올 본문
 * @returns 본문 HTML
 */
function render(body: string): string {
	return renderBody(parseKang(`---\ndescription: d\n---\n\n${body}`).body)
}

test('ADR-0002 에서 description, import 3개, 본문 링크를 만든다', () => {
	const view = parseKang(source('docs/adr/0002-flatten-on-read-import-on-write'))
	assert.ok(view.description.startsWith('ADR-0002: 읽을 때는 평탄화'))
	assert.deepEqual(view.imports, [
		{ target: 'CONTEXT.topic', local: 'topic' },
		{ target: 'CONTEXT.owner', local: 'owner' },
		{ target: 'CONTEXT.문서', local: '문서' },
	])
	const html = renderBody(view.body)
	assert.ok(html.includes('<a class="kang-ref" data-ref="문서">문서</a>'))
	assert.ok(html.includes('<a class="kang-ref" data-ref="owner">owner</a>'))
	// frontmatter 와 import 줄은 본문에 남지 않는다.
	assert.ok(!html.includes('description:'))
	assert.ok(!html.includes('rev &#34;'))
	assert.ok(!html.includes('`'))
})

test('CONTEXT 에서 keyword 카드와 줄 끝 #topic 링크를 만든다', () => {
	const view = parseKang(source('CONTEXT'))
	const html = renderBody(view.body)
	assert.ok(
		html.includes(
			'<div class="kang-term"><strong>keyword</strong><span>그 프로젝트의 도메인에서만 쓰는 특수 용어. 반드시 한 줄 정의가 있고, 계층을 가질 수 있다</span> <a class="kang-ref" data-ref="무엇을 keyword 로 선언하나">무엇을 keyword 로 선언하나</a></div>',
		),
	)
	// 정의 안의 백틱도 참조 링크다.
	assert.ok(html.includes('<strong>계층 keyword</strong><span>결제.상태 처럼 상위 개념 아래에 정의한 <a class="kang-ref" data-ref="keyword">keyword</a>.'))
})

test('코드 펜스 안의 백틱은 링크가 되지 않고, 코드 펜스 안의 import 와 keyword 는 모으지 않는다', () => {
	const view = parseKang('---\ndescription: d\n---\n\n## t\n\n```\nimport `x`.`y` rev "1"\nkeyword `z`: 정의\n```\n')
	assert.deepEqual(view.imports, [])
	const html = renderBody(view.body)
	assert.ok(html.includes('<code>import `x`.`y` rev &quot;1&quot;\nkeyword `z`: 정의\n</code>'))
	assert.ok(!html.includes('kang-ref'))
})

test('~~~ 는 컴파일러처럼 코드 펜스가 아니므로 안의 백틱이 링크가 된다', () => {
	const html = render('## t\n\n~~~\n`x` 와 `y`\n~~~\n')
	assert.ok(html.includes('<a class="kang-ref" data-ref="x">x</a>'), html)
	assert.ok(!html.includes('<code>'), html)
})

test('4칸 들여 쓴 줄은 컴파일러처럼 코드가 아니므로 링크가 되고, < 는 한 번만 escape 한다', () => {
	const html = render('## t\n\n본문\n\n    `x` < y\n')
	assert.ok(html.includes('<a class="kang-ref" data-ref="x">x</a> &lt; y'), html)
	assert.ok(!html.includes('<pre>'), html)
	assert.ok(!html.includes('&amp;lt;'))
})

test('백틱 3개 이상으로 시작하는 줄은 info string 이 있어도 펜스를 닫는다', () => {
	const html = render('## t\n\n```\n`a`\n```js\n\n## U\n\n`b`\n')
	assert.ok(html.includes('<pre><code>`a`\n</code></pre>'), html)
	assert.ok(html.includes('<h2>U</h2>'), html)
	assert.ok(html.includes('<a class="kang-ref" data-ref="b">b</a>'), html)
})

test('여는 펜스보다 짧은 백틱 줄은 펜스를 닫지 않고, 들여 쓴 펜스도 펜스다', () => {
	const html = render('## t\n\n     ````md\n```\n## U\n````\n')
	assert.ok(html.includes('<pre><code class="language-md">```\n## U\n</code></pre>'), html)
	assert.ok(!html.includes('<h2>U</h2>'), html)
})

test('문단 아래의 --- 나 === 는 setext 제목을 만들지 않는다', () => {
	const html = render('## t\n\n글\n===\n\n말\n---\n')
	assert.ok(!html.includes('<h1>'), html)
	assert.ok(!html.includes('<h2>글'), html)
	assert.ok(!html.includes('<h2>말'), html)
})

test('이미지 alt 와 title 은 escape 하고, 상대 경로가 아닌 이미지는 글자로 그린다', () => {
	const xss = render('## t\n\n![x" onerror="alert(1)](y)\n')
	assert.ok(!xss.includes('onerror="'), xss)
	assert.ok(xss.includes('<img src="y" alt="x&#34; onerror=&#34;alert(1)">'), xss)
	const title = render('## t\n\n![a](y "t\\" onload=\\"z")\n')
	assert.ok(!title.includes('onload="'), title)
	for (const src of ['https://e.com/p.png', '//e.com/p.png', '/\\e.com/p.png', 'data:image/png;base64,AA']) {
		const html = render(`## t\n\n![a<b](${src})\n`)
		assert.ok(!html.includes('<img'), html)
		assert.ok(html.includes('a&#60;b'), html)
	}
})

test('링크의 href, title 과 코드 펜스의 info string 은 속성을 깨지 않는다', () => {
	const link = render('## t\n\n[a](<x" onclick="1> "t\\" onclick=\\"2")\n')
	assert.ok(!link.includes('onclick="'), link)
	const code = render('## t\n\n```a"onclick="1\nb\n```\n')
	assert.ok(!code.includes('onclick="'), code)
})

test('\\` 는 백틱 문자로 남는다', () => {
	assert.ok(render('## t\n\n글자 \\` 하나 `x`\n').includes('글자 ` 하나 <a class="kang-ref" data-ref="x">x</a>'))
})

test('uncoded, iknow, exception, cover, pending 의 원본 기호가 출력에 남지 않는다', () => {
	const html = render(
		[
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
	for (const raw of ['//', 'iknow `', 'exception `', 'cover `', '` pending', '`']) assert.ok(!html.includes(raw), raw)
	assert.ok(html.includes('<h2>검토 절차 <span class="kang-badge">uncoded</span></h2>'))
	assert.ok(html.includes('<a class="kang-ref" data-ref="docs/B.금액">docs/B.금액</a>'))
	assert.ok(html.includes('<p>본문</p>'))
	assert.ok(html.includes('<div class="kang-decl"><span class="kang-badge">exception</span> 무료 청구서</div>'))
	assert.ok(html.includes('<span class="kang-badge">exception</span> 해외 결제 <span class="kang-badge">pending</span>'))
	assert.ok(html.includes('<span class="kang-badge">cover</span> <a class="kang-ref" data-ref="무료상품 예외">무료상품 예외</a>'))
})

test('본문의 HTML 은 글자로 남는다', () => {
	const html = render('## t\n\n<script>x</script> 와 <b>y</b>\n\n<div>z</div>\n')
	assert.ok(!html.includes('<script>'))
	assert.ok(!html.includes('<b>'))
	assert.ok(!html.includes('<div>'))
	assert.ok(html.includes('&#60;script&#62;'))
})

test('참조 바로 앞의 ! 는 참조가 아니라 글자로 남는다', () => {
	const html = render('## t\n\n주의!`결제` 와 `docs`/`A`!`예외`\n')
	assert.ok(html.includes('주의!<a class="kang-ref" data-ref="결제">결제</a>'), html)
	assert.ok(html.includes('<a class="kang-ref" data-ref="docs/A!예외">docs/A!예외</a>'), html)
})

test('참조 바로 앞의 # 는 참조가 아니라 글자로 남는다', () => {
	const html = render('## t\n\n주의#`결제` 와 `docs`/`A`#`상품`\n')
	assert.ok(html.includes('주의#<a class="kang-ref" data-ref="결제">결제</a>'), html)
	assert.ok(html.includes('<a class="kang-ref" data-ref="docs/A#상품">docs/A#상품</a>'), html)
})

test('description 은 컴파일러처럼 줄 앞뒤 공백을 떼고 찾고, 값의 앞뒤 공백만 떼고 따옴표는 둔다', () => {
	assert.equal(parseKang('---\n  description:  "a b"  \n---\n').description, '"a b"')
})

test('keyword 줄 끝의 # 는 앞에 공백이 있을 때만 topic 표시다', () => {
	const html = render('keyword `C`: 언어 C#`lang`\n\n## t\n')
	assert.ok(html.includes('<span>언어 C#<a class="kang-ref" data-ref="lang">lang</a></span></div>'), html)
})

test('frontmatter 는 컴파일러처럼 맨 앞 BOM 과 빈 줄을 건너뛰고 앞뒤 공백을 뗀 --- 로 찾는다', () => {
	const view = parseKang('\uFEFF\n \n  --- \ndescription: d\n ---\n\n## t\n')
	assert.equal(view.description, 'd')
	assert.equal(view.body, '\n## t\n')
})

test('lookupRef 는 참조 문자열을 scope 의 local 로 그대로 찾는다', () => {
	const scope = [
		{ local: 'A 결제', address: 'docs/A.결제', doc: 'docs/A' },
		{ local: '결제수단.카드', address: 'docs/B.결제수단.카드', doc: 'docs/B' },
		{ local: '결제의 방법', address: 'docs/B#결제의 방법', doc: 'docs/B' },
		{ local: '예외', address: 'docs/B!예외', doc: 'docs/B' },
		{ local: 'docs/A#상품', address: 'docs/A#상품', doc: 'docs/A' },
	]
	assert.equal(lookupRef(scope, 'A 결제'), scope[0])
	assert.equal(lookupRef(scope, '결제수단.카드'), scope[1])
	assert.equal(lookupRef(scope, '결제의 방법'), scope[2])
	assert.equal(lookupRef(scope, '#결제의 방법'), undefined)
	assert.equal(lookupRef(scope, '예외'), scope[3])
	assert.equal(lookupRef(scope, '!예외'), undefined)
	assert.equal(lookupRef(scope, 'docs/A#상품'), scope[4])
})

test('lookupRef 는 scope 에 없는 참조에 undefined 를 돌려준다', () => {
	const scope = [{ local: 'A 결제', address: 'docs/A.결제', doc: 'docs/A' }]
	assert.equal(lookupRef(scope, '모르는 이름'), undefined)
	assert.equal(lookupRef(scope, 'A 결제.하위'), undefined)
	assert.equal(lookupRef(scope, 'docs/A.결제'), undefined)
	assert.equal(lookupRef([], 'A 결제'), undefined)
})
