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

test('목록과 인용 안의 백틱 줄은 컴파일러처럼 펜스가 아니므로 안의 백틱이 링크다', () => {
	for (const mark of ['- ', '> ']) {
		const html = render(`## t\n\n${mark}\`\`\`\`\n${mark}\`x\`\n${mark}\`\`\`\`\n\n## U\n\n\`y\`\n`)
		assert.ok(!html.includes('<pre>'), html)
		assert.ok(html.includes('<a class="kang-ref" data-ref="x">x</a>'), html)
		assert.ok(html.includes('<h2>U</h2>'), html)
		assert.ok(html.includes('<a class="kang-ref" data-ref="y">y</a>'), html)
	}
})

test('문단 바로 다음 줄의 펜스는 info string 에 백틱이 있어도 컴파일러처럼 그 줄에서 열린다', () => {
	const html = render('## t\n\n글\n```js `a`\n`x`\n```\n\n## U\n\n`y`\n')
	assert.ok(html.includes('<p>글</p>'), html)
	assert.ok(html.includes('<pre><code class="language-js">`x`\n</code></pre>'), html)
	assert.ok(html.includes('<h2>U</h2>'), html)
	assert.ok(html.includes('<a class="kang-ref" data-ref="y">y</a>'), html)
})

test('문단 바로 다음 줄의 4칸 넘게 들여 쓴 펜스도 컴파일러처럼 펜스다', () => {
	const html = render('## t\n\n글\n      ```\n`x`\n      ```\n\n## U\n\n`y`\n')
	assert.ok(html.includes('<pre><code>`x`\n</code></pre>'), html)
	assert.ok(html.includes('<h2>U</h2>'), html)
	assert.ok(html.includes('<a class="kang-ref" data-ref="y">y</a>'), html)
})

test('HTML 블록처럼 보이는 줄도 컴파일러처럼 글자이고, 그 아래의 참조와 topic 이 살아 있다', () => {
	const html = render('## t\n\n<div>\n`x`\n</div>\n\n<!--\n## V\n`y`\n-->\n')
	assert.ok(!html.includes('<div>'), html)
	assert.ok(html.includes('&#60;div&#62;'), html)
	assert.ok(html.includes('<a class="kang-ref" data-ref="x">x</a>'), html)
	assert.ok(html.includes('<h2>V</h2>'), html)
	assert.ok(html.includes('<a class="kang-ref" data-ref="y">y</a>'), html)
})

test('문단 아래의 --- 나 === 는 setext 제목을 만들지 않는다', () => {
	const html = render('## t\n\n글\n===\n\n말\n---\n')
	assert.ok(!html.includes('<h1>'), html)
	assert.ok(!html.includes('<h2>글'), html)
	assert.ok(!html.includes('<h2>말'), html)
})

test('이미지는 주소와 상관없이 escape 한 alt 글자로 그린다', () => {
	const xss = render('## t\n\n![x" onerror="alert(1)](y)\n')
	assert.ok(!xss.includes('<img'), xss)
	assert.ok(xss.includes('x&#34; onerror=&#34;alert(1)'), xss)
	for (const src of ['y', 'https://e.com/p.png', '//e.com/p.png', 'data:image/png;base64,AA']) {
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

test('[참고]: 로 시작하는 줄은 컴파일러처럼 본문 글자이고, 그 줄의 참조도 링크다', () => {
	const html = render('## t\n\n[참고]: `결제`\n\n[근거]: `docs`/`A` "제목"\n')
	assert.ok(html.includes('<p>[참고]: <a class="kang-ref" data-ref="결제">결제</a></p>'), html)
	assert.ok(html.includes('<p>[근거]: <a class="kang-ref" data-ref="docs/A">docs/A</a> &quot;제목&quot;</p>'), html)
})

test('### 제목의 // uncoded 는 컴파일러처럼 수식어가 아니라 글자다', () => {
	const html = render('## t\n\n### 소제목 // uncoded\n')
	assert.ok(html.includes('<h3>소제목 // uncoded</h3>'), html)
	assert.ok(!html.includes('kang-badge'), html)
})

test('앞에 공백이나 탭이 있는 keyword, exception, cover 줄도 컴파일러처럼 선언이다', () => {
	const html = render('## t\n\n글\n  keyword `카드`: 카드 결제\n\texception `무료` pending\n cover `예외`\n')
	assert.ok(html.includes('<p>글</p>'), html)
	assert.ok(html.includes('<div class="kang-term"><strong>카드</strong><span>카드 결제</span></div>'), html)
	assert.ok(html.includes('<span class="kang-badge">exception</span> 무료 <span class="kang-badge">pending</span>'), html)
	assert.ok(html.includes('<span class="kang-badge">cover</span> <a class="kang-ref" data-ref="예외">예외</a>'), html)
})

test('문장 중간의 keyword 낱말은 선언이 아니다', () => {
	const html = render('## t\n\nA keyword `x`: y\n')
	assert.ok(html.includes('<p>A keyword <a class="kang-ref" data-ref="x">x</a>: y</p>'), html)
})

test('## 뒤에 공백이 없거나 4칸 넘게 들여 쓴 ## 줄도 컴파일러처럼 topic 제목이다', () => {
	const html = render('## t\n\n글\n##붙은 제목\n\n      ## 들여 쓴 제목 // uncoded\n\n###붙은 소제목\n')
	assert.ok(html.includes('<h2>붙은 제목</h2>'), html)
	assert.ok(html.includes('<h2>들여 쓴 제목 <span class="kang-badge">uncoded</span></h2>'), html)
	assert.ok(!html.includes('<h2>붙은 소제목'), html)
	assert.ok(html.includes('###붙은 소제목'), html)
})

test('닫히지 않은 펜스는 컴파일러처럼 끝까지 코드이고, 그 안의 제목, 참조, 선언을 읽지 않는다', () => {
	const view = parseKang('---\ndescription: d\n---\n\n## t\n\n앞\n\n```\n`결제`\n## 아님\nimport `a`.`b`\nkeyword `x`: y\n')
	assert.deepEqual(view.imports, [])
	const html = renderBody(view.body)
	assert.ok(html.includes('<p>앞</p>'), html)
	assert.ok(html.includes('<pre><code>`결제`\n## 아님\nimport `a`.`b`\nkeyword `x`: y\n</code></pre>'), html)
	assert.ok(!html.includes('kang-ref'), html)
	assert.ok(!html.includes('<h2>아님'), html)
})

test('긴 공백 줄도 100ms 안에 그린다', () => {
	const spaces = ' '.repeat(40000)
	const started = performance.now()
	render(`## t${spaces}x\n\nkeyword \`k\`: a${spaces}b\n\n## u // iknow \`v\`${spaces}w\n\nexception \`e\` // iknow \`v\`${spaces}w\n`)
	const elapsed = performance.now() - started
	assert.ok(elapsed < 100, `${elapsed}ms`)
})

test('목록과 인용 안의 제목, 선언, import 줄은 컴파일러처럼 본문 글자다', () => {
	const view = parseKang('---\ndescription: d\n---\n\n## t\n\n- ## 목록 제목\n\n> ## 인용 제목\n\n- keyword `카드`: 목록\n\n> exception `카드`\n\n- import `A`.`b`\n\n> cover `c`\n')
	assert.deepEqual(view.imports, [])
	const html = renderBody(view.body)
	for (const bad of ['<h2>목록', '<h2>인용', 'kang-term', 'kang-decl']) assert.ok(!html.includes(bad), html)
	assert.ok(html.includes('<li>## 목록 제목</li>'), html)
	assert.ok(html.includes('<p>## 인용 제목</p>'), html)
	assert.ok(html.includes('<li>keyword <a class="kang-ref" data-ref="카드">카드</a>: 목록</li>'), html)
	assert.ok(html.includes('<p>exception <a class="kang-ref" data-ref="카드">카드</a></p>'), html)
	assert.ok(html.includes('<li>import <a class="kang-ref" data-ref="A.b">A.b</a></li>'), html)
})

test('목록 바로 다음 줄의 keyword 줄은 컴파일러처럼 선언이다', () => {
	const html = render('## t\n\n- 항목\nkeyword `카드`: 정의\n')
	assert.ok(html.includes('<li>항목</li>'), html)
	assert.ok(html.includes('<div class="kang-term"><strong>카드</strong><span>정의</span></div>'), html)
})

test('컴파일러처럼 Rust 의 모든 공백 문자를 떼고 제목과 선언을 찾고, BOM 은 공백이 아니다', () => {
	const html = render('## t\n\n　## 전각 제목\n\n\u0085## 넬 제목\n\n﻿## 비오엠 제목\n\n keyword `NB`: nbsp 정의\n\n　exception `예` pending\n')
	assert.ok(html.includes('<h2>전각 제목</h2>'), html)
	assert.ok(html.includes('<h2>넬 제목</h2>'), html)
	assert.ok(!html.includes('<h2>비오엠'), html)
	assert.ok(html.includes('<div class="kang-term"><strong>NB</strong><span>nbsp 정의</span></div>'), html)
	assert.ok(html.includes('<span class="kang-badge">exception</span> 예 <span class="kang-badge">pending</span>'), html)
})

test('앞에 공백이 있는 import 줄도 컴파일러처럼 import 이다', () => {
	const view = parseKang('---\ndescription: d\n---\n\n　 import `A`.`결제수단`\n  import `docs`/`B`.`카드` as `C`\n\n## t\n')
	assert.deepEqual(view.imports, [
		{ target: 'A.결제수단', local: '결제수단' },
		{ target: 'docs/B.카드', local: 'C' },
	])
	assert.ok(!renderBody(view.body).includes('import'))
})

test('keyword, exception, cover, import 낱말 뒤에는 컴파일러처럼 공백 한 칸이 와야 하고 탭이면 본문이다', () => {
	const view = parseKang('---\ndescription: d\n---\n\nimport\t`A`.`b`\n\n## t\n\nkeyword\t`카드`: 탭\n\nexception\t`없음`\n\ncover\t`덮음`\n\nkeyword  `두칸`: 두 칸\n\n  exception  `예` pending\n')
	assert.deepEqual(view.imports, [])
	const html = renderBody(view.body)
	assert.ok(html.includes('<p>import\t<a class="kang-ref" data-ref="A.b">A.b</a></p>'), html)
	assert.ok(html.includes('<p>keyword\t<a class="kang-ref" data-ref="카드">카드</a>: 탭</p>'), html)
	assert.ok(html.includes('<p>exception\t<a class="kang-ref" data-ref="없음">없음</a></p>'), html)
	assert.ok(html.includes('<p>cover\t<a class="kang-ref" data-ref="덮음">덮음</a></p>'), html)
	assert.ok(html.includes('<div class="kang-term"><strong>두칸</strong><span>두 칸</span></div>'), html)
	assert.ok(html.includes('<span class="kang-badge">exception</span> 예 <span class="kang-badge">pending</span>'), html)
})

test('keyword 정의 안의 // 는 컴파일러처럼 앞에 공백이 있고 백틱 밖일 때만 수식어다', () => {
	const html = render('keyword `카드`: 플라스틱 a//b 와 x// iknow 끝 // iknow `B`.`카드`\nkeyword `값`: 이름 `a // iknow b` 끝\n\n## t\n')
	assert.ok(
		html.includes('<strong>카드</strong><span>플라스틱 a//b 와 x// iknow 끝</span> <span class="kang-badge">iknow</span> <a class="kang-ref" data-ref="B.카드">B.카드</a></div>'),
		html,
	)
	assert.ok(html.includes('<strong>값</strong><span>이름 <a class="kang-ref" data-ref="a // iknow b">a // iknow b</a> 끝</span></div>'), html)
})

test('topic 제목과 exception 의 수식어도 컴파일러처럼 앞에 Rust 공백 문자가 있어야 한다', () => {
	const html = render('## 제목　// uncoded\n\n## 붙은// uncoded\n\nexception `e`// iknow `B`.`카드`\nexception `f`　// iknow `B`.`카드`\n')
	assert.ok(html.includes('<h2>제목 <span class="kang-badge">uncoded</span></h2>'), html)
	assert.ok(html.includes('<h2>붙은// uncoded</h2>'), html)
	assert.ok(!html.includes('<span class="kang-badge">exception</span> e'), html)
	assert.ok(html.includes('<span class="kang-badge">exception</span> f <span class="kang-badge">iknow</span> <a class="kang-ref" data-ref="B.카드">B.카드</a>'), html)
})
