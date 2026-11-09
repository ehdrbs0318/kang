// .kang 파일의 읽기 전용 렌더링 화면이다. Orca 의 custom editor frame 에서 실행한다.
// 원문을 받으면 worker 에 `kang.locate` 를 묻고, parse.ts 의 marked 확장으로 본문을 그린다.
// 참조 링크는 마우스를 올리면 정의를 보여 주고, 누르면 `kang.open` 으로 그 문서를 연다. 문서에는 아무것도 쓰지 않는다.
import type { Locate } from '../protocol'
import { addStyles, createTooltip, locateNotice, positionTooltip } from '../frame'
import { setupFind } from './find'
import { lookupRef, parseKang, renderBody, type KangView } from './parse'

void addStyles(new URL('./style.css', import.meta.url).href)

const root = document.createElement('main')
root.className = 'kang'
document.body.append(root)
const tooltip = createTooltip()
const find = setupFind(root)

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
	} catch (error) {
		// worker 가 없거나 응답하지 않으면 링크 없이 그린다.
		console.debug('kang.locate 요청이 실패해서 링크 없이 그립니다', error)
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
	hideTooltip()
	root.replaceChildren()
	// 상태가 ok 가 아니면 안내 문구를 맨 위에 둔다.
	const notice = locateNotice(located)
	if (notice) root.append(notice)
	// description 은 인용 블록처럼 맨 위에 둔다.
	if (view.description) {
		const quote = document.createElement('blockquote')
		quote.className = 'kang-description'
		quote.textContent = view.description
		root.append(quote)
	}
	const scope = located?.status === 'ok' ? located.scope : []
	// import 는 심볼 이름과 문서 경로를 담은 칩으로 보여 준다. 문서 경로는 scope 에서 읽고, 없으면 kang 주소를 그대로 보여 준다.
	if (view.imports.length > 0) {
		const chips = document.createElement('div')
		chips.className = 'kang-imports'
		for (const imp of view.imports) {
			const chip = document.createElement('a')
			chip.className = 'kang-ref kang-chip'
			chip.dataset.address = imp.target
			const name = document.createElement('b')
			name.textContent = imp.local
			const doc = document.createElement('span')
			doc.textContent = scope.find((e) => e.address === imp.target)?.doc ?? imp.target
			chip.append(name, doc)
			chips.append(chip)
		}
		root.append(chips)
	}
	// 본문은 keyword 카드, 배지, 참조 링크까지 renderBody 가 그린다.
	const body = document.createElement('div')
	body.innerHTML = renderBody(view.body, scope)
	root.append(body)
	// 문서로 찾았을 때만 링크를 단다. 그래프 인덱스 파일이나 찾지 못한 문서는 링크가 없다.
	if (located?.status === 'ok' && located.kind === 'doc') link(located)
	// 찾기 막대가 열려 있으면 새 내용으로 다시 찾는다.
	find.refresh()
}

/**
 * scope 에 있는 참조에 링크를 단다. 링크는 정의가 있으면 페이지 안 tooltip 으로 보여 주고, 누르면 그 심볼의 문서를 연다.
 * scope 에 없거나 인덱스에 없는 문서를 가리키는 참조는 글자로 둔다.
 * @param located 상태가 ok 이고 kind 가 doc 인 `kang.locate` 답
 */
function link(located: Extract<Locate, { status: 'ok' }>): void {
	// 화면의 참조마다 scope 에서 항목을 찾는다. 칩은 kang 주소로, 본문 참조는 참조 문자열로 찾는다.
	for (const el of Array.from(root.querySelectorAll<HTMLAnchorElement>('a.kang-ref'))) {
		const address = el.dataset.address
		const entry = address !== undefined ? located.scope.find((e) => e.address === address) : lookupRef(located.scope, el.dataset.ref ?? '')
		// 항목이 없거나 그 문서가 인덱스에 없으면 글자로 둔다.
		if (!entry || !located.docs.some((d) => d.path === entry.doc)) continue
		el.href = '#'
		// 정의가 없으면 tooltip 을 달지 않는다. stale 문서의 정의는 worker 가 이미 뺐다. `constructor` 같은 물려받은 key 는 정의가 아니다.
		if (Object.hasOwn(located.definitions, entry.address)) {
			const definition = located.definitions[entry.address]
			el.addEventListener('mouseover', () => showTooltip(el, definition))
			el.addEventListener('focus', () => showTooltip(el, definition))
			el.addEventListener('mouseout', hideTooltip)
			el.addEventListener('blur', hideTooltip)
		}
		el.addEventListener('click', (event) => openEntry(event, located.workspaceId, located.root, entry.doc))
	}
}

/** 지금 tooltip 을 보여 주고 있는 링크와 그 정의다. 숨겨져 있으면 null 이다. */
let shown: { el: HTMLElement; definition: string } | null = null

/**
 * 링크 가까이에 정의 tooltip 을 보여 준다. 아래에 자리가 모자라면 위에 놓고, 화면 가장자리 안으로 제한한다.
 * 링크에 `aria-describedby` 를 달아 보조 기술이 정의를 읽게 한다.
 * @param el 정의를 가진 참조 링크
 * @param definition 보여 줄 정의 글자. textContent 로만 넣는다
 */
function showTooltip(el: HTMLElement, definition: string): void {
	// 다른 링크에 달려 있던 연결은 먼저 푼다.
	if (shown && shown.el !== el) shown.el.removeAttribute('aria-describedby')
	shown = { el, definition }
	tooltip.textContent = definition
	el.setAttribute('aria-describedby', tooltip.id)
	positionTooltip(tooltip, el.getBoundingClientRect())
}

/** 정의 tooltip 을 숨기고 링크의 `aria-describedby` 를 지운다. */
function hideTooltip(): void {
	shown?.el.removeAttribute('aria-describedby')
	shown = null
	tooltip.hidden = true
}

// 스크롤하면 닫지 않고 링크를 따라 위치를 다시 잡는다. 포커스가 일으킨 스크롤에도 tooltip 이 남는다.
window.addEventListener('scroll', () => {
	// 보여 주는 링크가 없으면 할 일이 없다.
	if (shown) positionTooltip(tooltip, shown.el.getBoundingClientRect())
}, true)

// Escape 키로 tooltip 을 닫는다.
window.addEventListener('keydown', (event) => {
	if (event.key === 'Escape') hideTooltip()
})

/**
 * 참조 링크를 누르면 그 심볼을 선언한 문서를 연다. topic 참조도 그 topic 이 있는 문서를 연다.
 * @param event 클릭 이벤트. frame 안에서 이동하지 않게 기본 동작을 막는다
 * @param workspaceId 문서를 열 workspace id
 * @param root 문서가 속한 kang 프로젝트 루트. `kang.locate` 답의 값이다
 * @param path 열 문서의 경로
 */
function openEntry(event: MouseEvent, workspaceId: string, root: string, path: string): void {
	event.preventDefault()
	// 요청이 실패해도 화면은 그대로 둔다.
	orca.plugin.request('kang.open', { workspaceId, root, path }).catch((error) => console.debug('kang.open 요청이 실패했습니다', error))
}

/**
 * 마크다운의 일반 링크를 누르면 frame 안에서 이동하지 않게 막고, 웹 주소만 기본 브라우저로 연다.
 * @param event 화면에서 일어난 클릭 이벤트
 */
function openPlainLink(event: MouseEvent): void {
	const anchor = (event.target as Element).closest('a[href]')
	// 링크가 아니거나 kang 참조 링크이면 여기서 할 일이 없다.
	if (!anchor || anchor.classList.contains('kang-ref')) return
	event.preventDefault()
	const href = anchor.getAttribute('href') ?? ''
	// 웹 주소와 메일 주소만 밖에서 연다. 요청이 실패해도 화면은 그대로 둔다.
	if (/^(https?:|mailto:)/.test(href)) orca.shell?.openExternal(href).catch((error) => console.debug('외부 링크를 열지 못했습니다', error))
}

root.addEventListener('click', openPlainLink)

/**
 * Orca 가 문서 내용을 줄 때마다 화면을 다시 그린다. 처음 문서와, 밖에서 파일이 바뀔 때 온다.
 * @param doc Orca 가 준 문서. bytes 문서는 오지 않는다 (manifest 가 text 다)
 */
orca.document?.onDidChange((doc) => {
	// text 문서만 그린다.
	if (doc.content.kind === 'text') void show(doc.content.text)
})
