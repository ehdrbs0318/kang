// 렌더링 화면과 그래프 화면이 함께 쓰는 frame 도우미다. 스타일 파일을 넣고 안내 문구를 만든다.
import type { Locate } from './protocol'

/** 상태가 `ok` 가 아닐 때 보여 주는 안내 문구 HTML 이다. 고정 문자열이므로 그대로 innerHTML 에 넣는다. */
const NOTICES: Record<Exclude<Locate['status'], 'ok'>, string> = {
	notIndexed: '<code>kang build</code> 를 실행하세요.',
	ambiguous: '같은 내용의 문서가 여럿 있어서 하나를 고를 수 없습니다.',
	remoteOnly: '원격 workspace 는 지원하지 않습니다.',
}

/** 인덱스가 오래된 문서가 있을 때 그래프 화면이 보여 주는 안내 문구 HTML 이다. */
export const STALE_NOTICE = '인덱스가 오래되었습니다. <code>kang build</code> 를 실행하세요.'

/**
 * 스타일 파일을 문서에 넣는다. 공통 스타일 `frame.css` 를 먼저, 화면의 스타일을 뒤에 넣는다.
 * 브라우저의 ES module 은 CSS 를 import 할 수 없으므로 link 로 넣는다. frame 의 CSP 가 플러그인 자신의 파일을 허용한다.
 * @param href 화면 스타일 파일의 절대 URL. 호출하는 module 의 `import.meta.url` 로 만든다
 * @returns 두 파일을 다 읽으면 풀리는 promise. 파일을 읽지 못해도 거부하지 않는다
 */
export function addStyles(href: string): Promise<void> {
	// 공통 스타일과 화면 스타일마다 link 를 하나씩 만들고, 읽기가 끝나기를 기다린다.
	const loads = [new URL('./frame.css', import.meta.url).href, href].map((url) => {
		const link = document.createElement('link')
		link.rel = 'stylesheet'
		link.href = url
		// 읽기에 실패해도 스타일 없이 그릴 수 있으므로 풀어 준다.
		const loaded = new Promise<void>((resolve) => {
			link.onload = link.onerror = () => resolve()
		})
		document.head.append(link)
		return loaded
	})
	return Promise.all(loads).then(() => undefined)
}

/**
 * `kang.locate` 답에 맞는 안내 문구 요소를 만든다.
 * @param located `kang.locate` 답. 요청이 실패했으면 null
 * @returns 상태가 ok 가 아니거나 요청이 실패했으면 안내 문구 요소, 상태가 ok 이면 null
 */
export function locateNotice(located: Locate | null): HTMLElement | null {
	// 상태가 ok 이면 안내할 것이 없다.
	if (located?.status === 'ok') return null
	return notice(located === null ? 'kang worker 에 연결하지 못했습니다.' : NOTICES[located.status])
}

/**
 * 안내 문구 요소를 만든다.
 * @param html 고정 문구 HTML. 사용자 입력을 넣지 않는다
 * @returns `kang-notice` 문단 요소
 */
export function notice(html: string): HTMLElement {
	const el = document.createElement('p')
	el.className = 'kang-notice'
	el.innerHTML = html
	return el
}

/**
 * 페이지 안에 보여 주는 tooltip 요소를 만들어 body 에 붙인다. 처음에는 숨겨져 있다.
 * 화면 좌표(`position: fixed`)로 놓으므로, 놓을 때 `left`, `top` 에 viewport 기준 값을 준다.
 * @returns 숨겨진 `kang-tooltip` 요소. 글자는 textContent 로만 넣는다
 */
export function createTooltip(): HTMLElement {
	const el = document.createElement('pre')
	el.className = 'kang-tooltip'
	el.id = 'kang-tooltip'
	el.setAttribute('role', 'tooltip')
	el.hidden = true
	document.body.append(el)
	return el
}

/** tooltip 과 viewport 가장자리 사이에 남기는 최소 간격(px)이다. */
const EDGE = 8

/** tooltip 과 기준 사각형 사이의 간격(px)이다. */
const GAP = 4

/**
 * tooltip 을 놓을 viewport 좌표를 계산한다. 기본은 기준 사각형 아래이고, 아래에 자리가 모자라면 위에 놓는다.
 * 네 방향 모두 viewport 안으로 제한한다. 순수 함수이므로 DOM 을 읽지 않는다.
 * @param anchor 기준 사각형(viewport 좌표). 점이면 left 와 right, top 과 bottom 을 같게 준다
 * @param size tooltip 의 너비와 높이
 * @param viewport viewport 의 너비와 높이
 * @returns `position: fixed` 에 넣을 left 와 top. tooltip 이 viewport 보다 크면 위쪽과 왼쪽 가장자리를 우선한다
 */
export function placeTooltip(
	anchor: { left: number; top: number; right: number; bottom: number },
	size: { width: number; height: number },
	viewport: { width: number; height: number },
): { left: number; top: number } {
	const maxLeft = viewport.width - size.width - EDGE
	const maxTop = viewport.height - size.height - EDGE
	const below = anchor.bottom + GAP
	const above = anchor.top - GAP - size.height
	// 아래에 들어가면 아래, 아니고 위에 들어가면 위, 둘 다 아니면 아래 값을 viewport 안으로 당긴다.
	const top = below <= maxTop ? below : above >= EDGE ? above : Math.min(below, maxTop)
	return { left: Math.max(EDGE, Math.min(anchor.left, maxLeft)), top: Math.max(EDGE, top) }
}

/**
 * tooltip 을 기준 사각형 가까이에 놓는다. 먼저 보이게 해서 크기를 잰 뒤 `placeTooltip` 으로 위치를 정한다.
 * @param tooltip `createTooltip` 이 만든 요소
 * @param anchor 기준 사각형(viewport 좌표)
 */
export function positionTooltip(tooltip: HTMLElement, anchor: { left: number; top: number; right: number; bottom: number }): void {
	tooltip.hidden = false
	const pos = placeTooltip(anchor, { width: tooltip.offsetWidth, height: tooltip.offsetHeight }, { width: window.innerWidth, height: window.innerHeight })
	tooltip.style.left = `${pos.left}px`
	tooltip.style.top = `${pos.top}px`
}
