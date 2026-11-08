// 렌더링 화면과 그래프 화면이 함께 쓰는 frame 도우미다. 스타일 파일을 넣고 안내 문구를 만든다.
import type { Locate } from './protocol'

/** 상태가 `ok` 가 아닐 때 보여 주는 안내 문구 HTML 이다. 고정 문자열이므로 그대로 innerHTML 에 넣는다. */
const NOTICES: Record<Exclude<Locate['status'], 'ok'>, string> = {
	notIndexed: '<code>kang build</code> 를 실행하세요.',
	ambiguous: '이 문서와 같은 문서가 여러 workspace 에 있어서 하나를 고를 수 없습니다.',
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
