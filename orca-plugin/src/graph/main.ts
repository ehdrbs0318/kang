// .kang.json 그래프 인덱스의 읽기 전용 그래프 화면이다. Orca 의 custom editor frame 에서 실행한다.
// 원문을 받으면 worker 에 `kang.locate` 를 묻고, 답의 문서와 import 관계를 cytoscape 로 그린다. JSON 은 worker 가 읽는다.
// 점을 누르면 `kang.open` 으로 그 문서를 열고, 화살표에 마우스를 올리면 심볼 목록을 보여 준다. 파일에는 아무것도 쓰지 않는다.
import cytoscape, { type Core, type StylesheetJson } from '../../vendor/cytoscape.esm.min.mjs'
import { addStyles, createTooltip, locateNotice, notice, positionTooltip, STALE_NOTICE } from '../frame'
import type { Locate } from '../protocol'
import { toElements } from './model'

/** 스타일 파일을 다 읽으면 풀린다. 그래프 영역의 크기가 스타일로 정해지므로, 그래프는 이 promise 뒤에 만든다. */
const styled = addStyles(new URL('./style.css', import.meta.url).href)

const notices = document.createElement('div')
notices.className = 'kang-notices'
const canvas = document.createElement('div')
canvas.className = 'kang-graph'
document.body.append(notices, canvas)
const tooltip = createTooltip()

// 창 크기나 안내 문구 때문에 그래프 영역의 크기가 바뀌면 cytoscape 에 다시 재게 한다. cytoscape 는 창 크기 변화만 스스로 본다.
new ResizeObserver(() => graph?.resize()).observe(canvas)

/** 지금 그린 그래프다. 새 원문이 오면 지우고 다시 만든다. */
let graph: Core | null = null
/** 가장 최근에 받은 원문의 번호다. 늦게 온 `kang.locate` 답이 새 화면을 덮지 않게 한다. */
let latest = 0

/**
 * CSS 색 값을 cytoscape 가 읽는 `rgb(...)` 로 바꾼다. Orca 테마는 cytoscape 가 모르는 `oklch()` 같은 형식을 쓸 수 있다.
 * @param name `--orca-*` 변수 이름
 * @returns 변수 색을 1px 에 칠해서 읽은 `rgb(r, g, b)` 문자열. 투명도는 버린다. 변수가 비었으면 회색이다
 */
function color(name: string): string {
	const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim()
	const ctx = document.createElement('canvas').getContext('2d')
	// 변수가 비었거나 2d context 를 만들 수 없으면 두 테마 모두에서 읽을 수 있는 회색을 쓴다.
	if (!value || !ctx) return 'rgb(128, 128, 128)'
	ctx.fillStyle = value
	ctx.fillRect(0, 0, 1, 1)
	const [r, g, b] = ctx.getImageData(0, 0, 1, 1).data
	return `rgb(${r}, ${g}, ${b})`
}

/**
 * 지금 테마의 색으로 그래프 스타일을 만든다. 점 크기는 들어오는 연결 수(`weight`)로 정한다.
 * @returns cytoscape 스타일
 */
function graphStyle(): StylesheetJson {
	const line = color('--orca-muted-foreground')
	return [
		{
			selector: 'node',
			style: {
				label: 'data(label)',
				width: 'mapData(weight, 0, 10, 16, 56)',
				height: 'mapData(weight, 0, 10, 16, 56)',
				'background-color': color('--orca-primary'),
				color: color('--orca-foreground'),
				'font-size': 12,
				'text-valign': 'bottom',
				'text-margin-y': 4,
				'text-background-color': color('--orca-background'),
				'text-background-opacity': 0.8,
			},
		},
		{
			selector: 'edge',
			style: {
				width: 1.5,
				'line-color': line,
				'target-arrow-color': line,
				'target-arrow-shape': 'triangle',
				'curve-style': 'bezier',
			},
		},
	]
}

/**
 * 원문 하나를 그린다. `kang.locate` 답을 기다린 뒤 한 번에 그린다.
 * @param text 그래프 인덱스 원문
 * @returns 그리기가 끝나면 풀리는 promise. worker 요청이 실패해도 거부하지 않고 안내 문구를 보여 준다
 */
async function show(text: string): Promise<void> {
	const turn = ++latest
	let located: Locate | null = null
	try {
		located = await orca.plugin.request('kang.locate', { text })
	} catch (error) {
		// worker 가 없거나 응답하지 않으면 안내 문구만 보여 준다.
		console.debug('kang.locate 요청이 실패해서 안내 문구만 보여 줍니다', error)
	}
	// 그래프 영역의 크기가 정해진 뒤에 그린다.
	await styled
	// 그 사이에 새 원문이 왔으면 이 결과는 버린다.
	if (turn !== latest) return
	draw(located)
}

/**
 * 안내 문구와 그래프를 다시 그린다.
 * @param located `kang.locate` 답. 요청이 실패했으면 null
 */
function draw(located: Locate | null): void {
	graph?.destroy()
	graph = null
	tooltip.hidden = true
	notices.replaceChildren()
	const failed = locateNotice(located)
	// 상태가 ok 가 아니면 그릴 그래프가 없으므로 안내 문구만 둔다.
	if (failed || located?.status !== 'ok') {
		if (failed) notices.append(failed)
		return
	}
	// 해시가 바뀐 문서가 있으면 그래프가 지금 문서와 다를 수 있다고 알린다.
	if (located.stale.length > 0) notices.append(notice(STALE_NOTICE))
	const cy = cytoscape({
		container: canvas,
		elements: toElements(located.docs, located.edges),
		style: graphStyle(),
		boxSelectionEnabled: false,
	})
	// 애니메이션 없이 배치해서 첫 화면에 바로 그린다. 기본값인 애니메이션 배치로는 Orca 화면 캡처에 빈 화면만 남았다.
	cy.layout({ name: 'cose', animate: false, nodeDimensionsIncludeLabels: true, nodeRepulsion: 1_000_000, componentSpacing: 120 }).run()
	// 문서가 적으면 fit 이 크게 확대해서 글자가 너무 커진다. 그때는 실제 크기로 두고 가운데에 놓는다.
	if (cy.zoom() > 1) {
		cy.zoom(1)
		cy.center()
	}
	/**
	 * 점을 누르면 그 문서를 연다.
	 * @param event cytoscape 의 tap 이벤트. `target` 은 누른 점이고 `data('id')` 가 문서 경로다
	 */
	cy.on('tap', 'node', (event) => {
		const path = String(event.target.data('id'))
		// 요청이 실패해도 화면은 그대로 둔다.
		orca.plugin.request('kang.open', { workspaceId: located.workspaceId, path }).catch((error) => console.debug('kang.open 요청이 실패했습니다', error))
	})
	/**
	 * 화살표에 마우스를 올리면 그 import 의 심볼 목록을 마우스 옆에 보여 준다.
	 * @param event cytoscape 의 mouseover 이벤트. `target` 은 화살표이고 `renderedPosition` 은 화면 안의 마우스 위치다
	 */
	cy.on('mouseover', 'edge', (event) => {
		tooltip.textContent = String(event.target.data('symbols'))
		// 마우스 위치를 viewport 좌표의 점으로 바꿔 가장자리 안쪽에 놓는다.
		const box = canvas.getBoundingClientRect()
		const x = box.left + event.renderedPosition.x
		const y = box.top + event.renderedPosition.y
		positionTooltip(tooltip, { left: x, top: y, right: x, bottom: y })
	})
	/** 화살표에서 마우스가 나가면 심볼 목록을 숨긴다. */
	cy.on('mouseout', 'edge', () => {
		tooltip.hidden = true
	})
	graph = cy
}

/** 테마가 바뀌면 --orca-* 변수가 이미 바뀌었으므로 색을 다시 읽어 스타일을 바꾼다. 그래프가 없으면 할 일이 없다. */
orca.on('theme', () => {
	graph?.style(graphStyle())
})

/**
 * Orca 가 문서 내용을 줄 때마다 그래프를 다시 그린다. 처음 문서와, 밖에서 파일이 바뀔 때 온다.
 * @param doc Orca 가 준 문서. bytes 문서는 오지 않는다 (manifest 가 text 다)
 */
orca.document?.onDidChange((doc) => {
	// text 문서만 그린다.
	if (doc.content.kind === 'text') void show(doc.content.text)
})
