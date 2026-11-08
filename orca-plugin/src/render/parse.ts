// kang 원문을 렌더링 화면용으로 나누고 HTML 로 그린다. 본문 참조는 `kang build` 가 쓴 scope 에서 찾는다.
// kang 문법(import, keyword, 백틱 참조, 선언 줄, topic 수식어)은 marked 의 확장으로 다룬다.
// 코드 영역과 kang 줄은 marked 가 아니라 컴파일러와 같은 줄 규칙으로 먼저 정한다. 백틱 3개 이상으로 시작하는 줄만 펜스이고, 그 안에서는 kang 문법을 해석하지 않는다.
// 컴파일러는 목록과 인용을 모른다. 그래서 `- ## 이름` 이나 `> keyword …` 는 kang 줄이 아니라 본문 글자다.
// 화면과 Orca 에 의존하지 않는 순수 함수이므로 Node 테스트에서 그대로 부른다.
import type { ScopeEntry } from '../protocol'
import { Marked, type MarkedExtension, type RendererThis, type Token, type TokenizerExtension, type TokenizerThis } from '../../vendor/marked.esm.js'
import { isSpace, trim, trimStart, WS } from '../ws'

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
// 점으로만 이은 참조다. 컴파일러처럼 scope 의 이름으로 나눌 수 있다.
const DOTTED = /^`[^`]*`(?:\.`[^`]*`)*$/
// 컴파일러처럼 앞의 공백 문자를 뗀 줄이 `##` 로 시작하고 그 다음이 `#` 가 아니면 topic 제목 줄이다. `###` 이하는 marked 가 그린다.
// `import`, `keyword`, `exception`, `cover` 낱말 뒤에 공백 한 칸이 오면 선언 줄이다. 탭이면 본문이다.
const KANG_LINE = /^(?:##(?!#)|(import|keyword|exception|cover) )/
// 아래 정규식은 lex 가 고친 kang 줄 한 줄에 쓴다. 앞의 공백 문자와 낱말 뒤의 공백 문자는 lex 가 이미 뗐다. 줄 안의 공백도 컴파일러처럼 Rust 공백 문자다.
// 컴파일러의 `parse_symbol_ref` 처럼 import 대상은 `/` 로 이은 문서 경로 뒤에 `.` keyword 이름, `#` topic, `!` exception 중 하나가 온다. 계층은 keyword 만 갖는다. 이름은 비지 않고 `/` 가 없다.
const NAME = '`[^`/\\n]+`'
const TARGET = `${NAME}(?:/${NAME})*(?:\\.${NAME}(?:\\.${NAME})*|[#!]${NAME})`
// 정의와 선언부는 `.` 대신 `[^\n]` 로 읽는다. `.` 는 U+2028, U+2029 에서 멈추는데 컴파일러는 이를 글자로 읽는다.
// 컴파일러는 ` as ` 와 ` rev ` 를 공백 한 칸까지 그대로 찾고, 그 앞뒤를 Rust 공백 문자로 뗀다.
const IMPORT = new RegExp(`^import (${TARGET})(?:[${WS}]* as [${WS}]*(${NAME}))?(?:[${WS}]* rev [${WS}]*"[^"\\n]*")?[${WS}]*$`)
const KEYWORD = new RegExp(`^keyword (\`[^\`\\n]*\`(?:\\.\`[^\`\\n]*\`)*)[${WS}]*:([^\\n]*)$`)
const DECL = /^(exception|cover) ([^\n]*)$/
// 컴파일러처럼 수식어를 뗀 exception, cover 선언부는 백틱 이름 하나이고, 뒤에 ` pending` 이 올 수 있다.
const DECL_NAME = /^`([^`]*)`( pending)?$/
// `##` 뒤의 공백은 없어도 된다.
const HEADING = /^##([^\n]*)$/
// 컴파일러의 `is_modifier_word` 처럼 `//` 뒤에 `uncoded` 나 `iknow` 낱말이 오고, 낱말 뒤는 끝이거나 공백 문자다. lastIndex 자리에서만 찾는다.
const MODIFIER_WORD = new RegExp(`[${WS}]*(?:uncoded|iknow)(?:[${WS}]|$)`, 'y')
// 컴파일러처럼 topic 표시의 `#` 앞에는 공백이 있어야 한다. `C#\`lang\`` 의 `#` 는 글자다.
const TOPIC = new RegExp(` (#${NAMES})[${WS}]*$`)

/**
 * 선언 줄을 선언부와 수식어로 가른다. 컴파일러의 `split_modifier` 와 같은 규칙이다.
 * 백틱 밖의 `//` 중에서 바로 앞이 공백 문자이고 뒤에 수식어 낱말이 오는 첫 자리에서 가른다. `` \` `` 는 백틱으로 세지 않는다.
 * @param text 선언 줄에서 낱말과 이름 또는 `##` 를 뗀 나머지
 * @returns [선언부, 앞뒤 공백을 뗀 수식어]. 수식어가 없으면 [text, undefined]
 */
function splitModifier(text: string): [string, string | undefined] {
	let tick = false
	// 한 글자씩 보며 백틱 안팎을 따라간다.
	for (let i = 0; i < text.length; i++) {
		// `` \` `` 는 글자이므로 두 글자를 건너뛴다.
		if (text[i] === '\\' && text[i + 1] === '`') i++
		// 백틱은 안팎을 바꾼다.
		else if (text[i] === '`') tick = !tick
		// 백틱 밖의 `//` 이다. 앞이 공백 문자이고 수식어 낱말이 따라오면 여기서 가른다. 아니면 두 번째 `/` 를 건너뛴다.
		else if (!tick && text.startsWith('//', i)) {
			MODIFIER_WORD.lastIndex = i + 2
			if (isSpace(text[i - 1]) && MODIFIER_WORD.test(text)) return [text.slice(0, i), trim(text.slice(i + 2))]
			i++
		}
	}
	return [text, undefined]
}

/**
 * 수식어에서 iknow 대상 원문을 꺼낸다. 컴파일러의 `parse_modifier` 처럼 `iknow` 낱말 뒤를 앞뒤 공백을 떼고 읽는다.
 * @param modifier splitModifier 가 뗀 수식어. 없으면 undefined
 * @returns iknow 대상 원문. iknow 수식어가 아니면 ''
 */
function iknowOf(modifier: string | undefined): string {
	return modifier?.startsWith('iknow') ? trim(modifier.slice('iknow'.length)) : ''
}

/**
 * renderBody 가 그리는 동안 쓰는 scope 의 local 이름들과, 그 이름의 가장 많은 조각 수다.
 * marked 의 그리기는 한 번에 끝나므로 renderBody 가 그리기 바로 앞에 바꾼다.
 */
let inScope = { locals: new Set<string>(), depth: 1 }

/** 점 참조 하나에서 나누어 볼 조각 수의 최대값이다. 나누기는 조각 수와 scope 깊이의 곱에 비례해 느려진다. */
const MAX_PARTS = 64

/**
 * 점으로 이은 참조의 조각들을 scope 의 이름으로 나눈다. 컴파일러의 `split_names`(`crates/kang/src/check.rs`)와 같은 순서로 찾는다.
 * 모든 조각이 scope 에 있는 나누기 중에서 왼쪽을 가장 길게 잡는 것을 고른다. 한 이름의 조각 수는 scope 의 가장 많은 조각 수까지만 본다.
 * @param parts 백틱 이름 조각들. 두 개 이상이다
 * @returns 나눈 이름들. 모든 조각이 scope 에 있는 나누기가 없거나 조각이 MAX_PARTS 개를 넘으면 조각을 모두 이은 이름 하나다
 */
function splitNames(parts: string[]): string[] {
	// shortcut: 조각이 MAX_PARTS 개를 넘으면 나누지 않는다. 컴파일러는 나누지만 화면이 멈추지 않게 한다. 실제 문서에 그런 참조가 생기면 컴파일러처럼 이은 문자열의 조각으로 찾게 바꾼다.
	if (parts.length > MAX_PARTS) return [parts.join('.')]
	const join = (at: number, count: number) => parts.slice(at, at + count).join('.')
	// fit[at] 은 at 번째 조각부터 끝까지를 모두 scope 의 이름으로 나눌 때 at 자리에서 잡을 조각 수다. 뒤를 먼저 알아야 하므로 끝에서부터 채운다.
	const fit: (number | undefined)[] = []
	fit[parts.length] = 0
	for (let at = parts.length - 1; at >= 0; at--) {
		// 긴 이름부터 보고, 나머지도 나눌 수 있는 첫 길이를 잡는다.
		for (let count = Math.min(parts.length - at, inScope.depth); count > 0 && fit[at] === undefined; count--) {
			if (fit[at + count] !== undefined && inScope.locals.has(join(at, count))) fit[at] = count
		}
	}
	// 모두 나눌 수 없으면 컴파일러가 이 참조를 받지 않는다. 나누지 않고 글자로 둔다.
	if (fit[0] === undefined) return [parts.join('.')]
	const pieces: string[] = []
	for (let at = 0; at < parts.length; at += fit[at]!) pieces.push(join(at, fit[at]!))
	return pieces
}

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
		// 점으로만 이은 참조는 그릴 때 나눌 수 있게 조각을 남긴다. 이름에는 백틱이 없으므로 `` `.` `` 로 자른다.
		const parts = DOTTED.test(match[0]) ? match[0].slice(1, -1).split('`.`') : []
		return { type: 'kangRef', raw: match[0], local: toLocal(match[0]), parts }
	},
	/**
	 * token 을 HTML 로 그린다. 점으로 이은 참조는 컴파일러처럼 scope 의 이름으로 나누어 이름마다 링크를 만든다.
	 * @param token 이 확장의 tokenizer 가 만든 token
	 * @returns `<a class="kang-ref">` 링크 HTML. 나누었으면 링크 사이에 `.` 를 둔다
	 */
	renderer(token) {
		const parts = token.parts as string[]
		// 조각이 둘 이상인 참조만 나눈다.
		const pieces = parts.length > 1 ? splitNames(parts) : [token.local as string]
		return pieces.map((local) => `<a class="kang-ref" data-ref="${escape(local)}">${escape(local)}</a>`).join('.')
	},
}

/** import 줄이다. 본문에는 그리지 않는다. parseKang 이 목록으로 꺼낸다. */
const importExtension: TokenizerExtension = {
	name: 'kangImport',
	level: 'block',
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
		const local = toLocal(match[2] ?? names[names.length - 1])
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
	 * 원문 맨 앞을 keyword 줄 token 으로 바꾼다.
	 * @param src 남은 원문
	 * @returns token. 문법이 맞지 않으면 undefined 이고 marked 의 다른 tokenizer 가 받는다
	 */
	tokenizer(src) {
		const match = KEYWORD.exec(src)
		// keyword 문법이 아니면 일반 문단으로 둔다.
		if (!match) return undefined
		const [rest, modifier] = splitModifier(match[2])
		const topic = TOPIC.exec(rest)
		const definition = trim(topic ? rest.slice(0, topic.index) : rest)
		return {
			type: 'kangKeyword',
			raw: match[0],
			name: toLocal(match[1]),
			definition: inline(this, definition),
			// 줄 끝 topic 의 `#` 는 문법 기호이므로 떼고 참조만 남긴다.
			topic: inline(this, topic ? topic[1].slice(1) : ''),
			iknow: inline(this, iknowOf(modifier)),
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
	 * 원문 맨 앞을 exception, cover 줄 token 으로 바꾼다.
	 * @param src 남은 원문
	 * @returns token. 문법이 맞지 않으면 undefined 이고 marked 의 다른 tokenizer 가 받는다
	 */
	tokenizer(src) {
		const match = DECL.exec(src)
		const [declaration, modifier] = splitModifier(match?.[2] ?? '')
		const parts = DECL_NAME.exec(trim(declaration))
		// 선언 문법이 아니면 일반 문단으로 둔다.
		if (!match || !parts) return undefined
		const [raw, kind] = match
		const [, name, pending] = parts
		return {
			type: 'kangDecl',
			raw,
			kind,
			pending: Boolean(pending),
			// cover 의 대상은 import 한 exception 이므로 참조로 해석한다. exception 의 이름은 선언이므로 글자다.
			target: kind === 'cover' ? inline(this, `\`${name}\``) : [],
			name,
			iknow: inline(this, iknowOf(modifier)),
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

/**
 * topic 제목 줄(`##`)이다. 컴파일러처럼 `##` 뒤 공백의 유무를 따지지 않는다.
 * `// uncoded` 나 `// iknow …` 수식어는 이 자리에서만 배지로 그린다. `###` 이하는 marked 가 글자 그대로 그린다.
 */
const headingExtension: TokenizerExtension = {
	name: 'kangHeading',
	level: 'block',
	/**
	 * 원문 맨 앞을 topic 제목 token 으로 바꾼다.
	 * @param src 남은 원문
	 * @returns token. topic 제목 줄이 아니면 undefined 이고 marked 의 다른 tokenizer 가 받는다
	 */
	tokenizer(src) {
		const match = HEADING.exec(src)
		// topic 제목 줄이 아니면 marked 의 다른 tokenizer 에 넘긴다.
		if (!match) return undefined
		// 수식어가 있으면 제목 글자에서 뗀다.
		const [title, modifier] = splitModifier(match[1])
		const text = trim(title)
		return {
			type: 'kangHeading',
			raw: match[0],
			text,
			tokens: inline(this, text),
			uncoded: modifier === 'uncoded',
			iknow: inline(this, iknowOf(modifier)),
		}
	},
	/**
	 * token 을 HTML 로 그린다.
	 * @param token 이 확장의 tokenizer 가 만든 token
	 * @returns 배지를 단 제목 HTML
	 */
	renderer(token) {
		const uncoded = token.uncoded ? ` ${badge('uncoded')}` : ''
		return `<h2>${this.parser.parseInline(token.tokens!)}${uncoded}${iknowHtml(this, token.iknow as Token[])}</h2>\n`
	},
}

/** 두 marked 인스턴스가 함께 쓰는 설정이다. 문서에 쓴 HTML 은 태그가 아니라 글자로 그린다. */
const common: MarkedExtension = {
	// 코드 영역과 kang 줄은 lex 가 컴파일러 규칙으로 먼저 떼어 낸다. 컴파일러는 들여 쓴 코드, HTML 블록, setext 제목, 링크 정의(`[이름]: 주소`)를 모른다. 그래서 다섯 tokenizer 를 끈다.
	// undefined 를 돌려주면 그 tokenizer 가 꺼진다. false 를 돌려주면 marked 의 기본 tokenizer 가 대신 읽는다.
	tokenizer: {
		fences: () => undefined,
		code: () => undefined,
		html: () => undefined,
		lheading: () => undefined,
		def: () => undefined,
		/**
		 * 목록과 인용 안의 `## 이름` 을 제목으로 읽지 않는다. 컴파일러에서 그 줄은 topic 제목이 아니라 본문 글자다.
		 * 맨 바깥의 `##` 줄은 lex 가 먼저 떼어 내므로 여기에 오지 않는다.
		 * @param src 남은 원문
		 * @returns `##` 제목이면 undefined 로 끄고, 그 밖의 제목은 false 로 기본 tokenizer 에 넘긴다
		 */
		heading: (src) => (/^ {0,3}##(?!#)/.test(src) ? undefined : false),
	},
	renderer: {
		/** 문서에 쓴 HTML 을 태그가 아니라 글자로 그린다. `token.text` 는 HTML 원문이고, 돌려주는 값은 escape 한 글자다. */
		html: (token) => escape(token.text),
		/** 이미지는 escape 한 alt 글자로 그린다. 상대 주소는 문서가 아니라 플러그인 화면을 기준으로 풀리므로 그릴 이미지가 없다. 밖으로 요청도 보내지 않는다. */
		image: (token) => escape(token.text),
	},
}

/** kang 줄 사이의 글을 읽는 marked 인스턴스다. 선언 줄 확장이 없으므로 목록과 인용 안의 선언 줄도 글자다. */
const prose = new Marked(common, { extensions: [refExtension] })

/** kang 줄 한 줄을 읽고, 모든 token 을 HTML 로 그리는 marked 인스턴스다. */
const kang = new Marked(common, { extensions: [refExtension, importExtension, keywordExtension, declExtension, headingExtension] })

/**
 * 본문을 최상위 token 으로 나눈다. 코드 영역과 kang 줄은 컴파일러(`crates/kang/src/parse.rs`)와 같은 줄 규칙으로 먼저 정한다.
 * 줄 앞의 공백 문자는 컴파일러처럼 Rust `char::is_whitespace` 의 문자를 뗀다.
 * 앞 공백을 뗀 줄이 백틱 3개 이상으로 시작하면 펜스가 열린다. 문단 바로 다음 줄이어도 같다. `- ` 나 `> ` 로 시작하는 줄은 펜스가 아니다.
 * 그 뒤에서 여는 줄보다 적지 않은 백틱으로 시작하는 줄이 펜스를 닫는다. 닫는 줄에 글자가 더 있어도 닫는다.
 * `~~~` 는 펜스가 아니다. 닫히지 않으면 끝까지 펜스다.
 * 펜스 밖에서 앞 공백을 뗀 줄이 KANG_LINE 이고 선언 문법에 맞으면 그 한 줄만 kang 확장으로 읽는다. 나머지 글은 prose 가 읽는다.
 * 그래서 `import 를 해야 한다` 처럼 선언 낱말로 시작하는 글은 문단이나 목록에서 떨어지지 않는다.
 * @param body parseKang 의 `body`
 * @returns 최상위 token 배열. 펜스 한 개는 code token 한 개다
 */
function lex(body: string): Token[] {
	const tokens: Token[] = []
	let text: string[] = []
	let fence: { run: number; lang: string; lines: string[] } | undefined
	for (const line of body.split('\n')) {
		const trimmed = trimStart(line)
		const run = /^`*/.exec(trimmed)![0].length
		// 열린 펜스 안이다. 여는 줄보다 적지 않은 백틱 줄이 닫고, 나머지 줄은 코드다.
		if (fence) {
			if (run < fence.run) fence.lines.push(line)
			else {
				tokens.push({ type: 'code', raw: '', lang: fence.lang, text: fence.lines.join('\n') })
				fence = undefined
			}
			continue
		}
		const kangLine = run < 3 ? KANG_LINE.exec(trimmed) : null
		const word = kangLine?.[1]
		// 선언 낱말 뒤의 공백 문자는 컴파일러처럼 몇 개여도 되므로 한 칸만 남긴다. `##` 줄은 수식어의 앞 공백이 필요하므로 그대로 둔다.
		const decl = kangLine ? kang.lexer(word ? `${word} ${trimStart(trimmed.slice(word.length + 1))}` : trimmed) : []
		// 펜스도 아니고 선언 문법에 맞는 kang 줄도 아닌 줄은 모아 두었다가 prose 에 넘긴다. 컴파일러도 문법에 맞지 않는 import 줄은 본문으로 읽는다.
		if (run < 3 && !decl[0]?.type.startsWith('kang')) {
			text.push(line)
			continue
		}
		// 펜스나 kang 줄이다. 앞에 모은 글을 먼저 token 으로 바꾼다.
		tokens.push(...prose.lexer(text.join('\n')))
		text = []
		// 펜스가 열린다.
		if (run >= 3) fence = { run, lang: trim(trimmed.slice(run)), lines: [] }
		// kang 줄이다.
		else tokens.push(...decl)
	}
	// 닫히지 않은 펜스는 끝까지 코드다.
	if (fence) tokens.push({ type: 'code', raw: '', lang: fence.lang, text: fence.lines.join('\n') })
	tokens.push(...prose.lexer(text.join('\n')))
	return tokens
}

/**
 * kang 원문을 화면용으로 나눈다. frontmatter 에서 description 을 읽고, 최상위 import 를 모은다.
 * @param text 문서 원문
 * @returns 화면용 결과. 문법이 틀린 줄도 버리지 않고 본문에 남긴다
 */
export function parseKang(text: string): KangView {
	const view: KangView = { description: '', imports: [], body: '' }
	const lines = text.replace(/^\uFEFF/, '').replace(/\r\n/g, '\n').split('\n')
	// 컴파일러처럼 맨 앞 빈 줄을 건너뛴다. 그 다음 줄이 앞뒤 공백을 뗀 `---` 이면 frontmatter 가 열린다.
	const open = lines.findIndex((l) => trim(l) !== '')
	let start = 0
	// 열린 frontmatter 는 다음 `---` 까지다. 그 안에서 description 만 읽는다.
	if (open >= 0 && trim(lines[open]) === '---') {
		const close = lines.findIndex((l, i) => i > open && trim(l) === '---')
		const end = close < 0 ? lines.length : close
		const front = lines.slice(open + 1, end)
		// 컴파일러처럼 줄 앞뒤 공백을 떼고 찾고, 값은 앞뒤 공백만 뗀다. 따옴표는 값의 일부다.
		const line = front.map(trim).find((l) => l.startsWith('description:'))
		if (line !== undefined) view.description = trim(line.slice('description:'.length))
		start = close < 0 ? lines.length : close + 1
	}
	view.body = lines.slice(start).join('\n')
	// 최상위 token 에서 import 를 모은다. 코드 영역 안의 같은 글자는 code token 이므로 걸리지 않는다.
	for (const token of lex(view.body)) {
		if (token.type === 'kangImport') view.imports.push({ target: token.target as string, local: token.local as string })
	}
	return view
}

/**
 * 본문을 HTML 로 그린다. 백틱 참조는 `<a class="kang-ref" data-ref="…">` 링크가 되고, 선언 줄과 수식어는 배지가 된다.
 * 점으로 이은 참조의 이은 이름이 scope 에 없으면, 컴파일러처럼 scope 의 이름으로 나누어 이름마다 링크를 만든다.
 * @param body parseKang 의 `body`
 * @param scope `kang.locate` 답의 `scope`. 답이 없으면 빈 목록이다
 * @returns HTML 문자열. 링크에는 아직 동작이 없다
 */
export function renderBody(body: string, scope: ScopeEntry[]): string {
	// iknow 대상은 local 이 주소 그대로인 항목이다. 컴파일러의 scope 에는 없으므로 나누기에 쓰지 않는다.
	const locals = new Set(scope.filter((e) => e.local !== e.address).map((e) => e.local))
	inScope = { locals, depth: [...locals].reduce((depth, local) => Math.max(depth, local.split('.').length), 1) }
	return kang.parser(lex(body))
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
