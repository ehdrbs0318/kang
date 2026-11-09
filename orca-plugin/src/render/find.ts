// 렌더링 화면 안의 찾기 막대다. isolated frame 에는 브라우저의 찾기가 닿지 않으므로 화면이 직접 찾는다.
// 키는 브라우저와 같다. Cmd/Ctrl+F 로 열고, Enter·Cmd+G 로 다음, Shift 를 함께 누르면 이전, Escape 로 닫는다.
// 결과는 CSS Custom Highlight API 로 칠한다. DOM 을 바꾸지 않으므로 참조 링크와 tooltip 이 그대로 동작한다.

/** 모든 결과를 칠하는 highlight 이름이다. style.css 의 `::highlight()` 와 같아야 한다. */
const ALL = 'kang-find'

/** 지금 결과를 칠하는 highlight 이름이다. style.css 의 `::highlight()` 와 같아야 한다. */
const CURRENT = 'kang-find-current'

/**
 * 글자에서 찾는 말이 나오는 구간을 모두 찾는다. 대소문자는 가리지 않고, 겹치는 결과는 세지 않는다.
 * 순수 함수이므로 DOM 을 읽지 않는다.
 * @param text 찾을 대상 글자
 * @param query 찾는 말. 정규식 문자도 글자 그대로 찾는다
 * @returns 결과마다 `[시작, 끝)` 위치. 찾는 말이 비었으면 빈 목록
 */
export function findOffsets(text: string, query: string): Array<[number, number]> {
	// 빈 말은 모든 위치에 맞으므로 찾지 않는다.
	if (query === '') return []
	// 소문자로 바꾼 사본은 길이가 달라질 수 있으므로, 원문 위에서 `i` 플래그로 찾는다.
	const pattern = new RegExp(query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'giu')
	return Array.from(text.matchAll(pattern), (m) => [m.index, m.index + m[0].length])
}

/** 찾기 막대가 키 하나로 하는 일이다. */
export type FindAction = 'open' | 'next' | 'previous' | 'close'

/**
 * 키 입력을 찾기 막대의 일로 바꾼다. 순수 함수이므로 DOM 을 읽지 않는다.
 * 글자 키는 `key` 가 아니라 `code` 로 본다. 한글 입력 상태에서는 F 키의 `key` 가 `ㄹ` 이 되기 때문이다. Orca 의 단축키도 같은 방식으로 본다.
 * @param event 키 입력. `code`, `key`, 수정 키만 읽는다
 * @param open 찾기 막대가 열려 있으면 true
 * @returns Cmd/Ctrl+F 는 `open`, 막대가 열려 있을 때 Cmd/Ctrl+G 는 `next`, Shift 를 함께 누르면 `previous`, Escape 는 `close`. 그 밖의 키는 null
 */
export function shortcut(
	event: Pick<KeyboardEvent, 'code' | 'key' | 'metaKey' | 'ctrlKey' | 'shiftKey' | 'altKey'>,
	open: boolean,
): FindAction | null {
	const mod = (event.metaKey || event.ctrlKey) && !event.altKey
	// Cmd/Ctrl+F 는 막대를 연다. 이미 열려 있으면 찾는 말을 다시 선택한다.
	if (mod && !event.shiftKey && event.code === 'KeyF') return 'open'
	// 막대가 닫혀 있으면 나머지 키는 화면의 다른 처리에 넘긴다.
	if (!open) return null
	if (mod && event.code === 'KeyG') return event.shiftKey ? 'previous' : 'next'
	return event.key === 'Escape' ? 'close' : null
}

/**
 * 찾기 막대를 만들어 body 에 붙이고 키를 연결한다.
 * @param root 찾을 대상 요소. 이 요소 안의 글자만 찾는다
 * @returns `refresh`. 화면을 다시 그린 뒤 부르면 열려 있는 찾기를 새 내용으로 다시 한다
 */
export function setupFind(root: HTMLElement): { refresh: () => void } {
	const bar = document.createElement('div')
	bar.className = 'kang-find'
	bar.hidden = true
	bar.setAttribute('role', 'search')
	const input = document.createElement('input')
	// search 형식은 Escape 로 글자를 지우고 지우기 버튼을 따로 그리므로 text 형식을 쓴다.
	input.type = 'text'
	input.placeholder = '찾기'
	input.setAttribute('aria-label', '문서에서 찾기')
	const count = document.createElement('span')
	count.className = 'kang-find-count'
	count.setAttribute('aria-live', 'polite')
	const prev = button('↑', '이전 결과', () => step(-1))
	const next = button('↓', '다음 결과', () => step(1))
	const close = button('✕', '찾기 닫기', hide)
	bar.append(input, count, prev, next, close)
	document.body.append(bar)

	let ranges: Range[] = []
	let current = -1

	/** 찾는 말로 결과를 다시 모으고 처음 결과로 간다. */
	function search(): void {
		ranges = collect(root, input.value)
		current = ranges.length > 0 ? 0 : -1
		paint()
	}

	/**
	 * 다음이나 이전 결과로 간다. 끝에서는 반대쪽 끝으로 돌아간다.
	 * @param delta 다음이면 1, 이전이면 -1
	 */
	function step(delta: number): void {
		// 결과가 없으면 움직일 곳이 없다.
		if (ranges.length === 0) return
		current = (current + delta + ranges.length) % ranges.length
		paint()
	}

	/** 결과를 칠하고 개수를 쓰고 지금 결과가 보이게 스크롤한다. */
	function paint(): void {
		CSS.highlights.set(ALL, new Highlight(...ranges))
		CSS.highlights.set(CURRENT, current >= 0 ? new Highlight(ranges[current]) : new Highlight())
		// 찾는 말이 없으면 개수를 비우고, 결과가 없으면 0 을 쓴다.
		count.textContent = input.value === '' ? '' : `${current + 1}/${ranges.length}`
		if (current >= 0) ranges[current].startContainer.parentElement?.scrollIntoView({ block: 'center' })
	}

	/** 막대를 열고 찾는 말을 선택한다. 이미 열려 있으면 선택만 한다. */
	function show(): void {
		bar.hidden = false
		input.focus()
		input.select()
		search()
	}

	/** 막대를 닫고 칠한 것을 지운다. */
	function hide(): void {
		bar.hidden = true
		ranges = []
		current = -1
		CSS.highlights.delete(ALL)
		CSS.highlights.delete(CURRENT)
	}

	input.addEventListener('input', search)
	input.addEventListener('keydown', (event) => {
		// Enter 는 다음, Shift+Enter 는 이전 결과다. 한글 조합 중의 Enter 는 조합을 끝내는 키라서 넘긴다.
		if (event.key === 'Enter' && !event.isComposing) {
			event.preventDefault()
			step(event.shiftKey ? -1 : 1)
		}
	})

	// capture 단계에서 받아서 화면의 다른 키 처리보다 먼저 막는다.
	window.addEventListener('keydown', (event) => {
		const action = shortcut(event, !bar.hidden)
		// 찾기 막대의 키가 아니면 화면의 다른 처리에 넘긴다.
		if (action === null) return
		event.preventDefault()
		// 연다, 다음, 이전, 닫는다 가운데 하나를 한다.
		if (action === 'open') show()
		else if (action === 'next') step(1)
		else if (action === 'previous') step(-1)
		else hide()
	}, true)

	return {
		refresh: () => {
			// 막대가 닫혀 있으면 다시 찾을 것이 없다.
			if (!bar.hidden) search()
		},
	}
}

/**
 * 요소 안의 글자 노드를 하나로 이어 찾고, 결과 위치를 노드 범위로 바꾼다. 링크로 나뉜 글자도 한 결과로 찾는다.
 * shortcut: 블록 사이에 구분자를 넣지 않으므로 한 문단 끝과 다음 문단 처음에 걸친 결과도 나온다. 오탐이 문제가 되면 블록 경계에 줄바꿈을 넣는다.
 * @param root 찾을 대상 요소
 * @param query 찾는 말
 * @returns 결과마다 하나의 Range. 문서 순서다
 */
function collect(root: HTMLElement, query: string): Range[] {
	const nodes: Text[] = []
	const starts: number[] = []
	let text = ''
	const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT)
	// 글자 노드마다 이어 붙인 글자에서의 시작 위치를 기록한다.
	for (let node = walker.nextNode(); node; node = walker.nextNode()) {
		nodes.push(node as Text)
		starts.push(text.length)
		text += node.nodeValue ?? ''
	}
	return findOffsets(text, query).map(([start, end]) => {
		const range = document.createRange()
		const [startNode, startOffset] = locate(nodes, starts, start, false)
		const [endNode, endOffset] = locate(nodes, starts, end, true)
		range.setStart(startNode, startOffset)
		range.setEnd(endNode, endOffset)
		return range
	})
}

/**
 * 이어 붙인 글자의 위치를 글자 노드와 그 노드 안의 위치로 바꾼다.
 * @param nodes 글자 노드 목록
 * @param starts 노드마다 이어 붙인 글자에서의 시작 위치
 * @param offset 바꿀 위치
 * @param isEnd 끝 위치이면 true. 노드 경계의 끝 위치는 앞 노드의 끝으로 둔다
 * @returns 노드와 그 노드 안의 위치
 */
function locate(nodes: Text[], starts: number[], offset: number, isEnd: boolean): [Text, number] {
	let i = 0
	// 위치를 담는 마지막 노드를 찾는다. 끝 위치는 노드 시작과 같으면 앞 노드에 둔다.
	while (i + 1 < nodes.length && (isEnd ? starts[i + 1] < offset : starts[i + 1] <= offset)) i++
	return [nodes[i], offset - starts[i]]
}

/**
 * 찾기 막대의 버튼을 만든다.
 * @param label 버튼 글자
 * @param title 보조 기술과 tooltip 이 읽는 이름
 * @param onClick 누르면 할 일
 * @returns 버튼 요소
 */
function button(label: string, title: string, onClick: () => void): HTMLButtonElement {
	const el = document.createElement('button')
	el.type = 'button'
	el.textContent = label
	el.title = title
	el.setAttribute('aria-label', title)
	el.addEventListener('click', onClick)
	return el
}
