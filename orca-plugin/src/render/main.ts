// .kang 파일의 읽기 전용 렌더링 화면이다. Orca 의 custom editor frame 에서 실행한다.
// 원문을 받으면 worker 에 `kang.locate` 를 묻고, parseKang 결과를 marked 로 그린다.
// 참조 링크는 마우스를 올리면 정의를 보여 주고, 누르면 `kang.open` 으로 그 문서를 연다. 문서에는 아무것도 쓰지 않는다.
import { marked } from '../../vendor/marked.esm.js'
import type { Locate } from '../protocol'
import { parseKang, resolveRef, type KangView } from './parse'

/** 상태가 `ok` 가 아닐 때 보여 주는 안내 문구다. */
const NOTICES: Record<Exclude<Locate['status'], 'ok'>, string> = {
	notIndexed: '`kang build` 를 실행하세요.',
	ambiguous: '이 문서와 같은 문서가 여러 workspace 에 있어서 하나를 고를 수 없습니다.',
	remoteOnly: '원격 workspace 는 지원하지 않습니다.',
}

// 브라우저의 ES module 은 CSS 를 import 할 수 없으므로 link 로 넣는다. frame 의 CSP 가 플러그인 자신의 파일을 허용한다.
const style = document.createElement('link')
style.rel = 'stylesheet'
style.href = new URL('./style.css', import.meta.url).href
document.head.append(style)

const root = document.createElement('main')
root.className = 'kang'
document.body.append(root)

/** 가장 최근에 받은 원문의 번호다. 늦게 온 `kang.locate` 답이 새 화면을 덮지 않게 한다. */
let latest = 0

/**
 * 원문 하나를 그린다. `kang.locate` 답을 기다린 뒤 한 번에 그린다.
 * @param text 문서 원문
 * @returns 그리기가 끝나면 풀리는 promise. worker 요청이 실패해도 거부하지 않고 안내 문구를 보여 준다
 */
async function show(text: string): Promise<void> {
	const turn = ++latest
	let located: Locate | null
	try {
		located = await orca.plugin.request('kang.locate', { text })
	} catch {
		// worker 가 없거나 응답하지 않으면 링크 없이 그린다.
		located = null
	}
	// 그 사이에 새 원문이 왔으면 이 결과는 버린다.
	if (turn !== latest) return
	draw(parseKang(text), located)
}

/**
 * 화면 전체를 다시 그린다.
 * @param view parseKang 결과
 * @param located `kang.locate` 답. 요청이 실패했으면 null
 */
function draw(view: KangView, located: Locate | null): void {
	root.replaceChildren()
	// 상태가 ok 가 아니면 안내 문구를 맨 위에 둔다.
	if (located === null || located.status !== 'ok') {
		const notice = document.createElement('p')
		notice.className = 'kang-notice'
		notice.textContent = located === null ? 'kang worker 에 연결하지 못했습니다.' : NOTICES[located.status]
		root.append(notice)
	}
	// description 은 인용 블록처럼 맨 위에 둔다.
	if (view.description) {
		const quote = document.createElement('blockquote')
		quote.className = 'kang-description'
		quote.textContent = view.description
		root.append(quote)
	}
	// import 는 심볼 이름과 문서 경로를 담은 칩으로 보여 준다.
	if (view.imports.length > 0) {
		const chips = document.createElement('div')
		chips.className = 'kang-imports'
		for (const imp of view.imports) {
			const chip = document.createElement('a')
			chip.className = 'kang-ref kang-chip'
			chip.dataset.ref = imp.local
			const name = document.createElement('b')
			name.textContent = imp.local
			const doc = document.createElement('span')
			doc.textContent = imp.doc
			chip.append(name, doc)
			chips.append(chip)
		}
		root.append(chips)
	}
	// keyword 는 이름과 정의를 담은 카드로, 나머지는 marked 로 그린 마크다운으로 넣는다.
	for (const block of view.blocks) {
		const el = document.createElement('div')
		if (block.kind === 'keyword') {
			el.className = 'kang-term'
			const name = document.createElement('strong')
			name.textContent = block.name
			const definition = document.createElement('span')
			definition.innerHTML = `${marked.parseInline(block.definition, { async: false })} ${block.refs}`
			el.append(name, definition)
		} else {
			el.innerHTML = marked.parse(block.text, { async: false })
		}
		root.append(el)
	}
	if (located?.status === 'ok' && located.path !== null) link(view, located, located.path)
}

/**
 * 풀 수 있는 참조에 링크를 단다. 링크는 정의를 tooltip 으로 보여 주고, 누르면 문서를 연다.
 * 인덱스에 없는 문서를 가리키는 참조는 글자로 둔다.
 * @param view parseKang 결과
 * @param located 상태가 ok 인 `kang.locate` 답
 * @param selfPath 이 문서의 경로
 */
function link(view: KangView, located: Extract<Locate, { status: 'ok' }>, selfPath: string): void {
	// 화면의 참조마다 주소를 풀고, 인덱스에 있는 문서이면 링크로 만든다.
	for (const el of Array.from(root.querySelectorAll<HTMLAnchorElement>('a.kang-ref'))) {
		const resolved = resolveRef(view, selfPath, el.dataset.ref ?? '')
		const doc = resolved && located.docs.find((d) => d.path === resolved.doc)
		if (!resolved || !doc) continue
		el.href = '#'
		el.title = located.definitions[resolved.address] ?? `${doc.path}: ${doc.description}`
		el.addEventListener('click', (event) => {
			event.preventDefault()
			open(located.workspaceId, selfPath, resolved)
		})
	}
}

/**
 * 참조 대상을 연다. 이 문서의 topic 이면 그 제목으로 스크롤하고, 아니면 `kang.open` 으로 대상 문서를 연다.
 * @param workspaceId 문서가 있는 workspace id
 * @param selfPath 이 문서의 경로
 * @param target 푼 참조
 */
function open(workspaceId: string, selfPath: string, target: { address: string; doc: string }): void {
	const topic = target.address.slice(selfPath.length + 1)
	// 이 문서의 topic 이면 다른 탭을 열지 않고 그 제목으로 간다.
	if (target.doc === selfPath && target.address[selfPath.length] === '#') {
		const heading = Array.from(root.querySelectorAll('h2, h3, h4, h5, h6')).find(
			(h) => Array.from(h.childNodes).filter((n) => !(n instanceof Element && n.classList.contains('kang-badge'))).map((n) => n.textContent).join('').trim() === topic,
		)
		heading?.scrollIntoView({ behavior: 'smooth' })
		return
	}
	// 요청이 실패해도 화면은 그대로 둔다. 열 수 없는 문서는 worker 가 `opened: false` 로 답한다.
	orca.plugin.request('kang.open', { workspaceId, path: target.doc }).catch(() => undefined)
}

// 마크다운의 일반 링크는 frame 안에서 이동하지 않게 막고, 웹 주소만 기본 브라우저로 연다.
root.addEventListener('click', (event) => {
	const anchor = (event.target as Element).closest('a[href]')
	if (!anchor || anchor.classList.contains('kang-ref')) return
	event.preventDefault()
	const href = anchor.getAttribute('href') ?? ''
	if (/^(https?:|mailto:)/.test(href)) orca.shell?.openExternal(href).catch(() => undefined)
})

// 처음 문서와, 밖에서 파일이 바뀔 때마다 오는 문서를 그린다. bytes 문서는 오지 않는다 (manifest 가 text 다).
orca.document?.onDidChange((doc) => {
	if (doc.content.kind === 'text') void show(doc.content.text)
})
