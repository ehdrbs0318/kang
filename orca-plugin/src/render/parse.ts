// kang 원문을 렌더링 화면의 블록으로 바꾸고, 본문 참조를 kang 주소로 푼다.
// 마크다운 해석은 marked 가 한다. 이 모듈은 kang 문법(frontmatter, import, keyword, 백틱 참조, 선언 줄)만 다룬다.
// 화면과 Orca 에 의존하지 않는 순수 함수이므로 Node 테스트에서 그대로 부른다.

/** 본문의 참조 하나다. `text` 는 화면 문구, `local` 은 문서 안의 참조 문자열이다. */
export type Ref = { text: string; local: string }

/** 화면의 블록 하나다. */
export type Block =
	/** 마크다운 블록이다. 참조는 `<a class="kang-ref" data-ref="…">…</a>` 로 바뀐 상태다. */
	| { kind: 'markdown'; text: string }
	/** keyword 선언 줄이다. `definition` 은 인라인 마크다운이고, `refs` 는 줄 끝 #topic 링크 HTML 또는 '' 이다. */
	| { kind: 'keyword'; name: string; definition: string; refs: string }

/** 문서 한 개를 화면용으로 나눈 결과다. */
export type KangView = {
	/** frontmatter 의 `description` 값이다. 없으면 '' 이다. */
	description: string
	/** import 줄들이다. `target` 은 kang 주소, `doc` 은 그 주소의 문서 경로, `local` 은 이 문서 안의 이름이다. */
	imports: { target: string; doc: string; local: string }[]
	/** 이 문서가 선언한 keyword 이름들이다. 계층은 `.` 으로 잇는다. */
	keywords: string[]
	blocks: Block[]
}

/** 백틱 참조 한 개, 또는 이스케이프한 백틱이다. 참조는 `/ . # !` 로 이은 백틱 이름들이고, 앞에 `#` 이나 `!` 이 올 수 있다. */
const REF = /\\`|[#!]?`[^`\n]*`(?:[/.#!]`[^`\n]*`)*/g
/** 줄 끝의 `// iknow 대상, 대상` 수식어다. */
const IKNOW = /\s*\/\/\s*iknow\s+(.*)$/
/** 줄 끝의 `// uncoded` 수식어다. */
const UNCODED = /\s*\/\/\s*uncoded\s*$/

/**
 * HTML 과 마크다운에서 특별한 뜻이 있는 문자를 문자 참조로 바꾼다.
 * 결과는 HTML 안에서도, 마크다운 안에서도 원래 글자로 보인다.
 * @param text 바꿀 글자
 * @returns 문자 참조로 바꾼 글자
 */
function escape(text: string): string {
	return text.replace(/[&<>"*_[\]~\\]/g, (c) => `&#${c.charCodeAt(0)};`)
}

/**
 * 백틱 참조 원문을 참조 문자열로 바꾼다. 백틱을 지우고 구분 기호는 그대로 둔다.
 * @param raw 예: `` `docs`/`A`.`결제` ``, `` #`topic` ``
 * @returns 예: `docs/A.결제`, `#topic`
 */
function toLocal(raw: string): string {
	return raw.replace(/`/g, '')
}

/**
 * 참조 하나를 링크 HTML 로 만든다.
 * @param ref 참조
 * @returns `<a class="kang-ref" data-ref="…">…</a>`
 */
function link(ref: Ref): string {
	return `<a class="kang-ref" data-ref="${escape(ref.local)}">${escape(ref.text)}</a>`
}

/**
 * 줄 안의 백틱 참조를 모두 링크로 바꾸고, 나머지 글자의 `&` 와 `<` 를 문자 참조로 바꾼다.
 * 그래서 문서에 쓴 HTML 은 태그가 아니라 글자로 보인다. 이스케이프한 백틱 `` \` `` 은 그대로 두어 marked 가 백틱 문자로 보여 준다.
 * @param line 한 줄 원문
 * @returns 마크다운 한 줄
 */
function inline(line: string): string {
	let out = ''
	let last = 0
	// 참조와 이스케이프한 백틱을 차례로 찾고, 그 사이 글자는 HTML 만 막는다.
	for (const match of line.matchAll(REF)) {
		out += line.slice(last, match.index).replace(/&/g, '&amp;').replace(/</g, '&lt;')
		const raw = match[0]
		const local = toLocal(raw)
		out += raw === '\\`' ? raw : link({ local, text: local.replace(/^[#!]/, '') })
		last = match.index + raw.length
	}
	return out + line.slice(last).replace(/&/g, '&amp;').replace(/</g, '&lt;')
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
 * 줄 끝의 `// iknow …` 를 떼어 낸다.
 * @param line 한 줄 원문
 * @returns 떼어 낸 줄과, iknow 배지와 대상 링크 HTML. iknow 가 없으면 html 은 ''
 */
function takeIknow(line: string): { rest: string; html: string } {
	const match = IKNOW.exec(line)
	// iknow 수식어가 없으면 줄을 그대로 둔다.
	if (!match) return { rest: line, html: '' }
	return { rest: line.slice(0, match.index), html: ` ${badge('iknow')} ${inline(match[1])}` }
}

/**
 * kang 원문을 화면용으로 나눈다.
 * frontmatter 에서 description 을, import 줄에서 import 목록을 꺼낸다. keyword 선언 줄은 keyword 블록이 된다.
 * 나머지 줄은 마크다운 블록이 되고, 백틱 참조는 링크로, 선언 줄과 수식어는 배지로 바뀐다. 코드 펜스 안은 바꾸지 않는다.
 * @param text 문서 원문
 * @returns 화면용 결과. 문법이 틀린 줄도 버리지 않고 마크다운으로 보여 준다
 */
export function parseKang(text: string): KangView {
	const view: KangView = { description: '', imports: [], keywords: [], blocks: [] }
	const lines = text.replace(/\r\n/g, '\n').split('\n')
	let i = 0
	// 첫 줄이 `---` 이면 다음 `---` 까지가 frontmatter 다. 그 안에서 description 만 읽는다.
	if (lines[0] === '---') {
		const end = lines.indexOf('---', 1)
		const body = end < 0 ? lines.slice(1) : lines.slice(1, end)
		const match = body.map((l) => /^description:\s*(.*)$/.exec(l)).find((m) => m)
		if (match) view.description = match[1].replace(/^(["'])(.*)\1$/, '$2')
		i = end < 0 ? lines.length : end + 1
	}

	let markdown: string[] = []
	let fence = false
	/** 모아 둔 마크다운 줄을 블록 하나로 내보낸다. 빈 줄만 있으면 내보내지 않는다. */
	const flush = () => {
		if (markdown.some((l) => l.trim() !== '')) view.blocks.push({ kind: 'markdown', text: markdown.join('\n') })
		markdown = []
	}

	// 한 줄씩 kang 문법을 보고 블록을 만든다.
	for (; i < lines.length; i++) {
		const line = lines[i]
		// 코드 펜스는 열고 닫는 줄까지 원문 그대로 둔다. marked 가 코드를 이스케이프한다.
		if (/^\s*```/.test(line)) fence = !fence
		if (fence || /^\s*```/.test(line)) {
			markdown.push(line)
			continue
		}

		const imp = /^import\s+(`[^`]*`(?:[/.#!]`[^`]*`)*)(?:\s+as\s+`([^`]*)`)?(?:\s+rev\s+"[^"]*")?\s*$/.exec(line)
		// import 줄은 본문에 그리지 않고 목록에만 넣는다. local 은 alias, 없으면 마지막 이름이다.
		if (imp) {
			const target = toLocal(imp[1])
			const names = imp[1].match(/`[^`]*`/g)!
			view.imports.push({ target, doc: target.split(/[.#!]/)[0], local: imp[2] ?? toLocal(names[names.length - 1]) })
			continue
		}

		const kw = /^keyword\s+(`[^`]*`(?:\.`[^`]*`)*)\s*:\s*(.*)$/.exec(line)
		// keyword 선언 줄은 keyword 블록이 된다. 줄 끝 #topic 과 iknow 는 refs 로 옮긴다.
		if (kw) {
			flush()
			const name = toLocal(kw[1])
			const { rest, html } = takeIknow(kw[2])
			const topic = /\s*(#`[^`]*`)\s*$/.exec(rest)
			const definition = topic ? rest.slice(0, topic.index) : rest
			const refs = (topic ? link({ local: toLocal(topic[1]), text: toLocal(topic[1]).slice(1) }) : '') + html
			view.keywords.push(name)
			view.blocks.push({ kind: 'keyword', name, definition: inline(definition.trim()), refs: refs.trim() })
			continue
		}

		const decl = /^(exception|cover)\s+`([^`]*)`(\s+pending)?(.*)$/.exec(line)
		// exception, cover 줄은 배지가 붙은 HTML 블록이 된다. 앞뒤 빈 줄로 위아래 문단과 떨어뜨린다.
		if (decl) {
			const [, kind, name, pending] = decl
			const { html } = takeIknow(decl[4])
			const target = kind === 'cover' ? link({ local: name, text: name }) : escape(name)
			const extra = (pending ? ` ${badge('pending')}` : '') + html
			markdown.push('', `<div class="kang-decl">${badge(kind)} ${target}${extra}</div>`, '')
			continue
		}

		const heading = /^(#{2,6}\s.*)$/.exec(line)
		// topic 제목의 uncoded, iknow 수식어는 배지로 바꾼다.
		if (heading) {
			const { rest, html } = takeIknow(heading[1])
			const uncoded = UNCODED.test(rest)
			markdown.push(inline(rest.replace(UNCODED, '')) + (uncoded ? ` ${badge('uncoded')}` : '') + html)
			continue
		}

		markdown.push(inline(line))
	}
	flush()
	return view
}

/**
 * 본문의 참조 문자열을 kang 주소와 그 주소의 문서 경로로 푼다.
 * 찾는 순서는 import 의 local 이름, import 한 keyword 의 하위 이름, 자기 keyword, 자기 topic, 본문에 쓴 전체 주소다.
 * @param view parseKang 결과
 * @param selfPath 이 문서의 경로 (예: `docs/adr/0002-flatten-on-read-import-on-write`)
 * @param local 참조 문자열 (예: `A 결제`, `결제수단.카드`, `#topic`, `docs/A.결제`)
 * @returns 주소와 문서 경로. 풀 수 없으면 null
 */
export function resolveRef(view: KangView, selfPath: string, local: string): { address: string; doc: string } | null {
	const bare = local.replace(/^[#!]/, '')
	const imported = view.imports.find((i) => i.local === bare)
	// alias 나 import 한 심볼의 마지막 이름이면 그 import 의 대상이다.
	if (imported) return { address: imported.target, doc: imported.doc }
	const dot = local.indexOf('.')
	const parent = dot > 0 ? view.imports.find((i) => i.local === local.slice(0, dot)) : undefined
	// import 한 keyword 아래의 하위 keyword 이면 대상 주소 뒤에 하위 이름을 잇는다.
	if (parent) return { address: parent.target + local.slice(dot), doc: parent.doc }
	// 자기 keyword 와 자기 topic 은 이 문서의 주소다.
	if (view.keywords.includes(local)) return { address: `${selfPath}.${local}`, doc: selfPath }
	if (local.startsWith('#')) return { address: `${selfPath}${local}`, doc: selfPath }
	// 구분 기호가 있으면 본문에 쓴 전체 주소다. 첫 `.`, `#`, `!` 앞이 문서 경로다.
	if (/[/.#!]/.test(local)) return { address: local, doc: local.split(/[.#!]/)[0] }
	return null
}
