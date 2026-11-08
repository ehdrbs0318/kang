// kang 원문을 렌더링 화면용으로 나누고 HTML 로 그린다. 본문 참조는 `kang build` 가 쓴 scope 에서 찾는다.
// kang 문법(import, keyword, 백틱 참조, 선언 줄, topic 수식어)은 marked 의 확장으로 다룬다.
// 코드 영역은 컴파일러와 같은 규칙으로 정한다. 백틱 3개 이상으로 시작하는 줄만 펜스이고, 그 안에서는 kang 문법을 해석하지 않는다.
// 화면과 Orca 에 의존하지 않는 순수 함수이므로 Node 테스트에서 그대로 부른다.
import type { ScopeEntry } from '../protocol'
import { Marked, type RendererThis, type Token, type TokenizerExtension, type TokenizerThis } from '../../vendor/marked.esm.js'

/** 문서 한 개를 화면용으로 나눈 결과다. */
export type KangView = {
	/** frontmatter 의 `description` 값이다. 없으면 '' 이다. */
	description: string
	/** import 줄들이다. `target` 은 kang 주소, `local` 은 칩에 보일 이름이다 (alias, 없으면 마지막 이름). */
	imports: { target: string; local: string }[]
	/** frontmatter 를 뺀 본문이다. `renderBody` 로 그린다. */
	body: string
}

/** 백틱으로 감싼 이름들을 `/ . # !` 로 이은 kang 참조 원문이다. */
const NAMES = '`[^`\\n]*`(?:[/.#!]`[^`\\n]*`)*'
const REF = new RegExp(`^${NAMES}`)
const IMPORT = new RegExp(`^import[ \\t]+(${NAMES})(?:[ \\t]+as[ \\t]+\`([^\`\\n]*)\`)?(?:[ \\t]+rev[ \\t]+"[^"\\n]*")?[ \\t]*(?:\\n|$)`)
const KEYWORD = /^keyword[ \t]+(`[^`\n]*`(?:\.`[^`\n]*`)*)[ \t]*:[ \t]*(.*)(?:\n|$)/
const DECL = /^(exception|cover)[ \t]+`([^`\n]*)`([ \t]+pending)?(?:[ \t]*\/\/[ \t]*iknow[ \t]+(.*?))?[ \t]*(?:\n|$)/
const HEADING = /^(#{1,6})[ \t]+(.*?)[ \t]*\/\/[ \t]*(?:(uncoded)|iknow[ \t]+(.*?))[ \t]*(?:\n|$)/
const IKNOW = /[ \t]*\/\/[ \t]*iknow[ \t]+(.*)$/
// 컴파일러처럼 topic 표시의 `#` 앞에는 공백이 있어야 한다. `C#\`lang\`` 의 `#` 는 글자다.
const TOPIC = new RegExp(` (#${NAMES})[ \\t]*$`)
/** 요청을 보내도 되는 이미지 주소다. scheme, `/`, `\\`, 공백이 없는 상대 경로만 받는다. */
const RELATIVE_SRC = /^(?![a-z][a-z0-9+.-]*:|[\\/])[^\\\s\x00-\x1f]+$/i

/**
 * HTML 에서 특별한 뜻이 있는 문자를 문자 참조로 바꾼다.
 * @param text 바꿀 글자
 * @returns HTML 안에서 원래 글자로 보이는 문자열
 */
function escape(text: string): string {
	return text.replace(/[&<>"]/g, (c) => `&#${c.charCodeAt(0)};`)
}

/**
 * kang 참조 원문에서 백틱을 지워 참조 문자열을 만든다. 구분 기호는 그대로 둔다.
 * @param raw 예: `` `docs`/`A`.`결제` ``
 * @returns 예: `docs/A.결제`
 */
function toLocal(raw: string): string {
	return raw.replace(/`/g, '')
}

/**
 * 선언 표시 배지 HTML 을 만든다.
 * @param label 배지 글자 (예: `uncoded`)
 * @returns `<span class="kang-badge">…</span>`
 */
function badge(label: string): string {
	return `<span class="kang-badge">${label}</span>`
}

/**
 * 인라인 원문을 token 으로 바꾸도록 줄을 세운다.
 * @param self 확장 tokenizer 의 `this`
 * @param src 인라인 원문. 빈 문자열이면 빈 배열이다
 * @returns 나중에 채워질 token 배열
 */
function inline(self: TokenizerThis, src: string): Token[] {
	return src ? self.lexer.inline(src, []) : []
}

/**
 * iknow 대상 목록을 배지와 링크 HTML 로 그린다.
 * @param self 확장 renderer 의 `this`
 * @param tokens iknow 대상의 인라인 token. 비었으면 iknow 가 없다
 * @returns ` <배지> <링크들>` 또는 ''
 */
function iknowHtml(self: RendererThis, tokens: Token[]): string {
	return tokens.length > 0 ? ` ${badge('iknow')} ${self.parser.parseInline(tokens)}` : ''
}

/**
 * 본문의 백틱 참조다. `<a class="kang-ref" data-ref="참조 문자열">` 로 그린다. `` \` `` 는 marked 의 escape 가 먼저 가져간다.
 * 맨 앞의 `#` 와 `!` 는 참조에 넣지 않고 글자로 둔다. 컴파일러는 본문에서 백틱 쌍만 참조로 본다.
 * 참조 앞의 `#` 가 문법인 자리는 keyword 줄 끝의 topic 하나뿐이고, keywordExtension 이 그 `#` 를 뗀다.
 */
const refExtension: TokenizerExtension = {
	name: 'kangRef',
	level: 'inline',
	/**
	 * marked 가 이 확장을 시도할 위치를 찾는다.
	 * @param src 남은 원문
	 * @returns 참조의 첫 시작 위치. 없으면 -1
	 */
	start: (src) => src.indexOf('`'),
	/**
	 * 원문 맨 앞을 백틱 참조 token 으로 바꾼다.
	 * @param src 남은 원문
	 * @returns token. 문법이 맞지 않으면 undefined 이고 marked 의 다른 tokenizer 가 받는다
	 */
	tokenizer(src) {
		const match = REF.exec(src)
		// 참조가 아니면 marked 의 다른 tokenizer 에 넘긴다.
		if (!match) return undefined
		return { type: 'kangRef', raw: match[0], local: toLocal(match[0]) }
	},
	/**
	 * token 을 HTML 로 그린다.
	 * @param token 이 확장의 tokenizer 가 만든 token
	 * @returns `<a class="kang-ref">` 링크 HTML
	 */
	renderer(token) {
		const local = token.local as string
		return `<a class="kang-ref" data-ref="${escape(local)}">${escape(local)}</a>`
	},
}

/** import 줄이다. 본문에는 그리지 않는다. parseKang 이 목록으로 꺼낸다. */
const importExtension: TokenizerExtension = {
	name: 'kangImport',
	level: 'block',
	/**
	 * marked 가 이 확장을 시도할 위치를 찾는다.
	 * @param src 남은 원문
	 * @returns import 줄의 첫 시작 위치. 없으면 -1
	 */
	start: (src) => src.search(/^import[ \t]+`/m),
	/**
	 * 원문 맨 앞을 import 줄 token 으로 바꾼다.
	 * @param src 남은 원문
	 * @returns token. 문법이 맞지 않으면 undefined 이고 marked 의 다른 tokenizer 가 받는다
	 */
	tokenizer(src) {
		const match = IMPORT.exec(src)
		// import 문법이 아니면 일반 문단으로 둔다.
		if (!match) return undefined
		const target = toLocal(match[1])
		const names = match[1].match(/`[^`]*`/g)!
		// local 은 alias, 없으면 마지막 이름이다.
		const local = match[2] ?? toLocal(names[names.length - 1])
		return { type: 'kangImport', raw: match[0], target, local }
	},
	/** import 줄은 본문에 그리지 않는다. 늘 빈 문자열이다. */
	renderer: () => '',
}

/** keyword 선언 줄이다. 이름과 정의를 담은 카드로 그린다. 줄 끝 #topic 과 iknow 는 카드 끝에 둔다. */
const keywordExtension: TokenizerExtension = {
	name: 'kangKeyword',
	level: 'block',
	/**
	 * marked 가 이 확장을 시도할 위치를 찾는다.
	 * @param src 남은 원문
	 * @returns keyword 줄의 첫 시작 위치. 없으면 -1
	 */
	start: (src) => src.search(/^keyword[ \t]+`/m),
	/**
	 * 원문 맨 앞을 keyword 줄 token 으로 바꾼다.
	 * @param src 남은 원문
	 * @returns token. 문법이 맞지 않으면 undefined 이고 marked 의 다른 tokenizer 가 받는다
	 */
	tokenizer(src) {
		const match = KEYWORD.exec(src)
		// keyword 문법이 아니면 일반 문단으로 둔다.
		if (!match) return undefined
		const iknow = IKNOW.exec(match[2])
		const rest = iknow ? match[2].slice(0, iknow.index) : match[2]
		const topic = TOPIC.exec(rest)
		const definition = (topic ? rest.slice(0, topic.index) : rest).trim()
		return {
			type: 'kangKeyword',
			raw: match[0],
			name: toLocal(match[1]),
			definition: inline(this, definition),
			// 줄 끝 topic 의 `#` 는 문법 기호이므로 떼고 참조만 남긴다.
			topic: inline(this, topic ? topic[1].slice(1) : ''),
			iknow: inline(this, iknow ? iknow[1] : ''),
		}
	},
	/**
	 * token 을 HTML 로 그린다.
	 * @param token 이 확장의 tokenizer 가 만든 token
	 * @returns keyword 카드 HTML
	 */
	renderer(token) {
		const topic = this.parser.parseInline(token.topic as Token[])
		return (
			`<div class="kang-term"><strong>${escape(token.name as string)}</strong>` +
			`<span>${this.parser.parseInline(token.definition as Token[])}</span>` +
			(topic ? ` ${topic}` : '') +
			`${iknowHtml(this, token.iknow as Token[])}</div>\n`
		)
	},
}

/** exception, cover 줄이다. 원문 기호 대신 배지로 그린다. cover 의 대상은 참조 링크다. */
const declExtension: TokenizerExtension = {
	name: 'kangDecl',
	level: 'block',
	/**
	 * marked 가 이 확장을 시도할 위치를 찾는다.
	 * @param src 남은 원문
	 * @returns exception, cover 줄의 첫 시작 위치. 없으면 -1
	 */
	start: (src) => src.search(/^(?:exception|cover)[ \t]+`/m),
	/**
	 * 원문 맨 앞을 exception, cover 줄 token 으로 바꾼다.
	 * @param src 남은 원문
	 * @returns token. 문법이 맞지 않으면 undefined 이고 marked 의 다른 tokenizer 가 받는다
	 */
	tokenizer(src) {
		const match = DECL.exec(src)
		// 선언 문법이 아니면 일반 문단으로 둔다.
		if (!match) return undefined
		const [raw, kind, name, pending, iknow] = match
		return {
			type: 'kangDecl',
			raw,
			kind,
			pending: Boolean(pending),
			// cover 의 대상은 import 한 exception 이므로 참조로 해석한다. exception 의 이름은 선언이므로 글자다.
			target: kind === 'cover' ? inline(this, `\`${name}\``) : [],
			name,
			iknow: inline(this, iknow ?? ''),
		}
	},
	/**
	 * token 을 HTML 로 그린다.
	 * @param token 이 확장의 tokenizer 가 만든 token
	 * @returns 배지를 단 선언 HTML
	 */
	renderer(token) {
		const target = token.kind === 'cover' ? this.parser.parseInline(token.target as Token[]) : escape(token.name as string)
		const pending = token.pending ? ` ${badge('pending')}` : ''
		return `<div class="kang-decl">${badge(token.kind as string)} ${target}${pending}${iknowHtml(this, token.iknow as Token[])}</div>\n`
	},
}

/** `// uncoded` 나 `// iknow …` 가 붙은 topic 제목이다. 수식어를 배지로 그린다. 수식어가 없는 제목은 marked 가 그린다. */
const headingExtension: TokenizerExtension = {
	name: 'kangHeading',
	level: 'block',
	/**
	 * 원문 맨 앞을 수식어가 붙은 제목 token 으로 바꾼다.
	 * @param src 남은 원문
	 * @returns token. 문법이 맞지 않으면 undefined 이고 marked 의 다른 tokenizer 가 받는다
	 */
	tokenizer(src) {
		const match = HEADING.exec(src)
		// 수식어가 없으면 marked 의 기본 제목이다.
		if (!match) return undefined
		const [raw, hashes, text, uncoded, iknow] = match
		return { type: 'kangHeading', raw, depth: hashes.length, text, tokens: inline(this, text), uncoded: Boolean(uncoded), iknow: inline(this, iknow ?? '') }
	},
	/**
	 * token 을 HTML 로 그린다.
	 * @param token 이 확장의 tokenizer 가 만든 token
	 * @returns 배지를 단 제목 HTML
	 */
	renderer(token) {
		const tag = `h${token.depth as number}`
		const uncoded = token.uncoded ? ` ${badge('uncoded')}` : ''
		return `<${tag}>${this.parser.parseInline(token.tokens!)}${uncoded}${iknowHtml(this, token.iknow as Token[])}</${tag}>\n`
	},
}

/**
 * 원문 맨 앞의 코드 펜스를 컴파일러(`crates/kang/src/parse.rs`)와 같은 규칙으로 읽는다.
 * 앞 공백을 뗀 줄이 백틱 3개 이상으로 시작하면 펜스가 열린다. 그 뒤에서 여는 줄보다 적지 않은 백틱으로 시작하는 줄이 펜스를 닫는다.
 * 닫는 줄에 글자가 더 있어도 닫는다. `~~~` 는 펜스가 아니다. 닫히지 않으면 끝까지 펜스다.
 * @param src 남은 원문
 * @returns code token. 맨 앞 줄이 펜스가 아니면 undefined 이고 marked 의 다른 tokenizer 가 받는다. false 는 marked 의 기본 펜스로 넘기므로 쓰지 않는다
 */
function fences(src: string): Token | undefined {
	const run = (line: string) => /^`*/.exec(line.trimStart())![0].length
	const first = /^[^\n]*/.exec(src)![0]
	const open = run(first)
	// 백틱 3개 이상으로 시작하지 않으면 펜스가 아니다.
	if (open < 3) return undefined
	const lines = src.split('\n')
	const close = lines.findIndex((line, i) => i > 0 && run(line) >= open)
	// 닫는 줄이 없으면 남은 원문 전부가 펜스다.
	const end = close < 0 ? lines.length : close + 1
	const raw = lines.slice(0, end).join('\n') + (end < lines.length ? '\n' : '')
	const text = lines.slice(1, close < 0 ? lines.length : close).join('\n')
	return { type: 'code', raw, lang: first.trimStart().slice(open).trim(), text }
}

/**
 * 이미지를 그린다. 같은 플러그인 안의 상대 경로만 `<img>` 로 그리고, 나머지는 alt 글자로 그려 밖으로 요청을 보내지 않는다.
 * marked 의 기본 renderer 는 alt 를 escape 하지 않으므로 여기서 alt 와 title 을 모두 escape 한다.
 * @param token image token. `href` 는 주소, `title` 은 제목, `text` 는 alt 원문이다
 * @returns `<img>` HTML 또는 escape 한 alt 글자
 */
function image({ href, title, text }: { href: string; title?: string | null; text: string }): string {
	// 상대 경로가 아니면 요청을 보내지 않고 글자만 남긴다.
	if (!RELATIVE_SRC.test(href)) return escape(text)
	return `<img src="${escape(href)}" alt="${escape(text)}"${title ? ` title="${escape(title)}"` : ''}>`
}

/** kang 확장을 단 marked 인스턴스다. 문서에 쓴 HTML 은 태그가 아니라 글자로 그린다. */
const kang = new Marked({
	extensions: [refExtension, importExtension, keywordExtension, declExtension, headingExtension],
	// 컴파일러는 들여 쓴 코드와 setext 제목을 모른다. 그래서 두 tokenizer 를 끄고, 펜스는 컴파일러 규칙으로 바꾼다.
	// undefined 를 돌려주면 그 tokenizer 가 꺼진다. false 를 돌려주면 marked 의 기본 tokenizer 가 대신 읽는다.
	tokenizer: { fences, code: () => undefined, lheading: () => undefined },
	/** 문서에 쓴 HTML 을 태그가 아니라 글자로 그린다. `token.text` 는 HTML 원문이고, 돌려주는 값은 escape 한 글자다. */
	renderer: { html: (token) => escape(token.text), image },
})

/**
 * kang 원문을 화면용으로 나눈다. frontmatter 에서 description 을 읽고, 최상위 import 를 모은다.
 * @param text 문서 원문
 * @returns 화면용 결과. 문법이 틀린 줄도 버리지 않고 본문에 남긴다
 */
export function parseKang(text: string): KangView {
	const view: KangView = { description: '', imports: [], body: '' }
	const lines = text.replace(/^\uFEFF/, '').replace(/\r\n/g, '\n').split('\n')
	// 컴파일러처럼 맨 앞 빈 줄을 건너뛴다. 그 다음 줄이 앞뒤 공백을 뗀 `---` 이면 frontmatter 가 열린다.
	const open = lines.findIndex((l) => l.trim() !== '')
	let start = 0
	// 열린 frontmatter 는 다음 `---` 까지다. 그 안에서 description 만 읽는다.
	if (open >= 0 && lines[open].trim() === '---') {
		const close = lines.findIndex((l, i) => i > open && l.trim() === '---')
		const end = close < 0 ? lines.length : close
		const front = lines.slice(open + 1, end)
		// 컴파일러처럼 줄 앞뒤 공백을 떼고 찾고, 값은 앞뒤 공백만 뗀다. 따옴표는 값의 일부다.
		const line = front.map((l) => l.trim()).find((l) => l.startsWith('description:'))
		if (line !== undefined) view.description = line.slice('description:'.length).trim()
		start = close < 0 ? lines.length : close + 1
	}
	view.body = lines.slice(start).join('\n')
	// 최상위 token 에서 import 를 모은다. 코드 영역 안의 같은 글자는 code token 이므로 걸리지 않는다.
	for (const token of kang.lexer(view.body)) {
		if (token.type === 'kangImport') view.imports.push({ target: token.target as string, local: token.local as string })
	}
	return view
}

/**
 * 본문을 HTML 로 그린다. 백틱 참조는 `<a class="kang-ref" data-ref="…">` 링크가 되고, 선언 줄과 수식어는 배지가 된다.
 * @param body parseKang 의 `body`
 * @returns HTML 문자열. 링크에는 아직 동작이 없다
 */
export function renderBody(body: string): string {
	return kang.parse(body, { async: false })
}

/**
 * 본문의 참조 문자열을 `kang build` 가 쓴 scope 에서 찾는다. 이름 풀이는 컴파일러가 이미 했으므로 여기서는 찾기만 한다.
 * @param scope `kang.locate` 답의 `scope`
 * @param ref 참조 문자열 (예: `A 결제`, `결제수단.카드`, `docs/A#상품`)
 * @returns 맞는 항목. 없으면 undefined
 */
export function lookupRef(scope: ScopeEntry[], ref: string): ScopeEntry | undefined {
	return scope.find((e) => e.local === ref)
}
