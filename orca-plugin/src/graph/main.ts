// .kang.json 그래프 인덱스의 읽기 전용 그래프 화면이다. Orca 의 custom editor frame 에서 실행한다.
// 원문을 받으면 그 해시로 worker 에 `kang.locate` 를 묻고, 답의 문서와 import 관계를 cytoscape 로 그린다.
// worker 가 파일을 찾지 못하면(원격 workspace 등) 열린 원문을 직접 읽어 그린다. 그때는 문서를 열 수 없다.
// 화면 구성은 GEPA Workbench 를 따른다. 위에 통계와 범례, 왼쪽에 문서 목록, 가운데에 그래프, 오른쪽에 고른 문서의 상세 패널이 있다.
// 점에 마우스를 올리면 이웃만 밝게 남기고, 점을 누르면 상세 패널에 고정하고, 두 번 누르면 `kang.open` 으로 문서를 연다. 파일에는 아무것도 쓰지 않는다.
import cytoscape, { type Core, type NodeSingular, type StylesheetJson } from '../../vendor/cytoscape.esm.min.mjs'
import { addStyles, DETACHED_NOTICE, frameHash, createTooltip, locateNotice, notice, positionTooltip, STALE_NOTICE } from '../frame'
import { definitionsOf, parseIndex } from '../graph-index'
import type { Locate } from '../protocol'
import { buildModel, type GraphModel, matches, toElements } from './model'
import { createPanels, folderColor } from './panel'

/** 스타일 파일을 다 읽으면 풀린다. 그래프 영역의 크기가 스타일로 정해지므로, 그래프는 이 promise 뒤에 만든다. */
const styled = addStyles(new URL('./style.css', import.meta.url).href)

/** 지금 그린 그래프다. 새 원문이 오면 지우고 다시 만든다. */
let graph: Core | null = null
/** 지금 화면의 모델과, 문서를 열 Orca workspace id 와 kang 프로젝트 루트다. 그래프가 없으면 null 이고, 원문만으로 그렸으면 `target` 이 null 이다. */
let current: { model: GraphModel; target: { workspaceId: string; root: string } | null } | null = null
/** 상세 패널에 고정한 문서 경로다. */
let selected: string | null = null
/** 마우스가 올라가 있는 문서 경로다. 고른 문서보다 먼저 강조한다. */
let hovered: string | null = null
/** 검색어다. */
let query = ''
/** 가장 최근에 받은 원문의 번호다. 늦게 온 `kang.locate` 답이 새 화면을 덮지 않게 한다. */
let latest = 0

const panels = createPanels({
	select(path, center) {
		selected = path
		refresh()
		const node = path ? graph?.getElementById(path) : null
		// 목록에서 고른 점이 그래프 영역 밖에 있을 때만 그 점이 보이게 그래프를 부드럽게 옮긴다.
		if (center && node && node.nonempty() && !inView(node.renderedPosition())) graph?.animate({ center: { eles: node } }, { duration: 300 })
	},
	open,
	search(next) {
		query = next
		refresh()
	},
	fit() {
		graph?.animate({ fit: { eles: graph.elements(), padding: 48 } }, { duration: 300 })
	},
})

const notices = document.createElement('div')
notices.className = 'kang-notices'
const canvas = document.createElement('div')
canvas.className = 'kang-graph'
const body = document.createElement('div')
body.className = 'kg-body'
body.append(panels.list, canvas, panels.detail)
const layout = document.createElement('div')
layout.className = 'kg-layout'
layout.hidden = true
layout.append(panels.header, body)
document.body.append(notices, layout)
const tooltip = createTooltip()

// 창 크기나 안내 문구 때문에 그래프 영역의 크기가 바뀌면 cytoscape 에 다시 재게 한다. cytoscape 는 창 크기 변화만 스스로 본다.
new ResizeObserver(() => graph?.resize()).observe(canvas)

// 마우스가 점 위에서 그래프 영역 밖으로 바로 나가면 cytoscape 가 mouseout 을 보내지 않는다. 그래서 영역을 나갈 때 강조와 tooltip 을 직접 지운다.
canvas.addEventListener('mouseleave', () => {
	tooltip.hidden = true
	canvas.style.cursor = ''
	// 강조 중인 점이 없으면 다시 그릴 것이 없다.
	if (hovered === null) return
	hovered = null
	refresh(false)
})

// Esc 를 누르면 고른 문서를 푼다.
document.addEventListener('keydown', (event) => {
	if (event.key === 'Escape') {
		selected = null
		refresh()
	}
})

/**
 * 그래프 영역 안의 점이 가장자리에서 40px 안쪽에 있는지 본다.
 * @param at 그래프 영역 기준 좌표
 * @returns 안쪽이면 true
 */
function inView(at: { x: number; y: number }): boolean {
	const margin = 40
	return at.x >= margin && at.y >= margin && at.x <= canvas.clientWidth - margin && at.y <= canvas.clientHeight - margin
}

/**
 * 문서를 Orca 편집기 탭으로 연다. 요청이 실패해도 화면은 그대로 둔다.
 * @param path 인덱스의 문서 경로
 */
function open(path: string): void {
	const target = current?.target
	// 그래프가 없거나 원문만으로 그렸으면 열 workspace 를 모른다.
	if (!target) return
	orca.plugin.request('kang.open', { workspaceId: target.workspaceId, root: target.root, path }).catch((error) => console.debug('kang.open 요청이 실패했습니다', error))
}

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
 * 지금 테마의 색으로 그래프 스타일을 만든다.
 * 점 색은 폴더, 점 크기는 들어오는 연결 수(`weight`), 화살표 굵기는 심볼 수(`count`)로 정한다.
 * @returns cytoscape 스타일
 */
function graphStyle(): StylesheetJson {
	const line = color('--orca-muted-foreground')
	const fg = color('--orca-foreground')
	const bg = color('--orca-background')
	const accent = color('--orca-primary')
	return [
		{
			selector: 'node',
			style: {
				label: 'data(label)',
				width: 'mapData(weight, 0, 8, 22, 64)',
				height: 'mapData(weight, 0, 8, 22, 64)',
				// 점 색은 폴더 순번으로 PALETTE 에서 고른다.
				'background-color': (n: NodeSingular) => folderColor(Number(n.data('folderIndex'))),
				'background-opacity': 0.9,
				'border-width': 3,
				'border-color': (n: NodeSingular) => folderColor(Number(n.data('folderIndex'))),
				color: fg,
				'font-size': 12,
				'font-weight': 600,
				'text-valign': 'bottom',
				'text-margin-y': 6,
				// 긴 이름은 공백에서 줄을 바꿔 점 아래에 좁게 쌓는다.
				'text-wrap': 'wrap',
				'text-max-width': '130px',
				'text-halign': 'center',
				'text-background-color': bg,
				'text-background-opacity': 0.85,
				'text-background-padding': '2px',
				'text-background-shape': 'roundrectangle',
				'transition-property': 'opacity, border-width, underlay-opacity',
				'transition-duration': 180,
			},
		},
		{
			// 허브는 강조색의 빛 테두리를 두른다.
			selector: 'node[role = "hub"]',
			style: { 'underlay-color': accent, 'underlay-opacity': 0.25, 'underlay-padding': 8, 'underlay-shape': 'ellipse' },
		},
		{
			// 고립 문서는 속을 비운다.
			selector: 'node[role = "orphan"]',
			style: { 'background-opacity': 0.15, 'border-style': 'dashed' },
		},
		{
			selector: 'edge',
			style: {
				width: 'mapData(count, 1, 8, 1.5, 6)',
				'line-color': line,
				'line-opacity': 0.6,
				'target-arrow-color': line,
				'target-arrow-shape': 'triangle',
				'arrow-scale': 1.1,
				'curve-style': 'bezier',
				'transition-property': 'opacity, line-color, target-arrow-color',
				'transition-duration': 180,
			},
		},
		{ selector: '.faded', style: { opacity: 0.12 } },
		{ selector: 'edge.lit', style: { 'line-color': accent, 'target-arrow-color': accent, 'line-opacity': 1 } },
		{ selector: 'node.focus', style: { 'border-color': fg, 'border-width': 4, 'underlay-color': fg, 'underlay-opacity': 0.18, 'underlay-padding': 10 } },
		{ selector: 'node.match', style: { 'border-color': accent, 'border-width': 4 } },
	]
}

/**
 * 강조, 검색, 고르기 상태를 그래프와 패널에 반영한다.
 * 마우스를 올린 문서가 있으면 그 문서와 이웃을, 없으면 고른 문서와 이웃을 밝게 남긴다. 검색어가 있으면 맞는 문서만 밝게 남긴다.
 * @param panelsToo false 이면 그래프 강조만 바꾸고 목록과 상세 패널은 그대로 둔다
 */
function refresh(panelsToo = true): void {
	// 그래프가 없으면 반영할 곳이 없다.
	if (!current) return
	const visible = new Set(current.model.nodes.filter((n) => matches(n, query)).map((n) => n.path))
	// 마우스 올림은 패널을 바꾸지 않는다. 패널을 다시 그리면 상세 패널의 스크롤 위치가 처음으로 돌아간다.
	if (panelsToo) panels.update(selected, visible)
	const cy = graph
	if (!cy) return
	cy.batch(() => {
		cy.elements().removeClass('faded lit focus match')
		const focusPath = hovered ?? selected
		const focus = focusPath ? cy.getElementById(focusPath) : null
		// 강조할 문서가 있으면 그 문서와 이웃만 남기고 나머지를 흐리게 한다.
		if (focus && focus.nonempty()) {
			const near = focus.closedNeighborhood()
			cy.elements().not(near).addClass('faded')
			focus.addClass('focus')
			focus.connectedEdges().addClass('lit')
		} else if (query.trim()) {
			// 검색어만 있으면 맞는 문서를 표시하고 나머지를 흐리게 한다.
			cy.nodes().forEach((n: NodeSingular) => {
				n.addClass(visible.has(n.id()) ? 'match' : 'faded')
			})
			cy.edges().addClass('faded')
		}
		// 고른 문서는 다른 문서를 강조하는 동안에도 테두리로 표시한다.
		if (selected) cy.getElementById(selected).addClass('focus')
	})
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
		located = await orca.plugin.request('kang.locate', { hash: await frameHash(text) })
	} catch (error) {
		// worker 가 없거나 응답하지 않으면 안내 문구만 보여 준다.
		console.debug('kang.locate 요청이 실패해서 안내 문구만 보여 줍니다', error)
	}
	// 그래프 영역의 크기가 정해진 뒤에 그린다.
	await styled
	// 그 사이에 새 원문이 왔으면 이 결과는 버린다.
	if (turn !== latest) return
	draw(located, text)
}

/**
 * 안내 문구, 패널, 그래프를 다시 그린다.
 * @param located `kang.locate` 답. 요청이 실패했으면 null
 * @param text 열린 그래프 인덱스 원문. 답이 ok 가 아니면 이 원문으로 그린다
 */
function draw(located: Locate | null, text: string): void {
	graph?.destroy()
	graph = null
	current = null
	hovered = null
	tooltip.hidden = true
	notices.replaceChildren()
	let model: GraphModel
	// 답이 ok 이면 worker 가 읽은 인덱스로 그리고, 문서를 열 수 있다.
	if (located?.status === 'ok') {
		// 해시가 바뀐 문서가 있으면 그래프가 지금 문서와 다를 수 있다고 알린다.
		if (located.stale.length > 0) notices.append(notice(STALE_NOTICE))
		model = buildModel(located.docs, located.edges, located.definitions)
		current = { model, target: { workspaceId: located.workspaceId, root: located.root } }
	} else {
		const lists = parseIndex(text)
		// 답이 ok 가 아닌 까닭을 먼저 알린다. 상태가 ok 가 아니면 항상 문구가 있다.
		notices.append(locateNotice(located)!)
		// 원문도 인덱스가 아니면 그릴 그래프가 없으므로 안내 문구만 둔다.
		if (!lists) {
			layout.hidden = true
			return
		}
		// worker 가 파일을 찾지 못했어도 원문에 그래프가 다 있으므로 그린다. 문서는 열 수 없다.
		notices.append(notice(DETACHED_NOTICE))
		model = buildModel(lists.docs, lists.edges, definitionsOf(lists.keywords))
		current = { model, target: null }
	}
	// 새 원문에 고른 문서가 없으면 고르기를 푼다.
	if (selected && !model.nodes.some((n) => n.path === selected)) selected = null
	panels.setModel(model)
	layout.hidden = false
	const cy = cytoscape({
		container: canvas,
		elements: toElements(model),
		style: graphStyle(),
		boxSelectionEnabled: false,
		minZoom: 0.2,
		maxZoom: 3,
	})
	// 애니메이션 없이 배치해서 첫 화면에 바로 그린다. 기본값인 애니메이션 배치로는 Orca 화면 캡처에 빈 화면만 남았다.
	cy.layout({ name: 'cose', animate: false, nodeDimensionsIncludeLabels: true, nodeRepulsion: 1_000_000, componentSpacing: 120, padding: 48 }).run()
	// 문서가 적으면 fit 이 크게 확대해서 글자가 너무 커진다. 그때는 1.3 배로 두고 가운데에 놓는다.
	if (cy.zoom() > 1.3) {
		cy.zoom(1.3)
		cy.center()
	}
	/**
	 * 점을 누르면 그 문서를 상세 패널에 고정한다. 다시 누르면 푼다.
	 * @param event cytoscape 의 tap 이벤트. `target` 은 누른 점이고 `id()` 가 문서 경로다
	 */
	cy.on('tap', 'node', (event) => {
		const path = String(event.target.id())
		selected = selected === path ? null : path
		refresh()
	})
	/**
	 * 빈 곳을 누르면 고르기를 푼다.
	 * @param event cytoscape 의 tap 이벤트. 빈 곳이면 `target` 이 그래프 자신이다
	 */
	cy.on('tap', (event) => {
		if (event.target !== cy) return
		selected = null
		refresh()
	})
	/**
	 * 점을 두 번 누르면 그 문서를 연다.
	 * @param event cytoscape 의 dbltap 이벤트
	 */
	cy.on('dbltap', 'node', (event) => open(String(event.target.id())))
	/**
	 * 점에 마우스를 올리면 이웃을 강조하고 경로와 description 을 보여 준다.
	 * @param event cytoscape 의 mouseover 이벤트. `renderedPosition` 은 그래프 영역 안의 마우스 위치다
	 */
	cy.on('mouseover', 'node', (event) => {
		const node = model.nodes.find((n) => n.path === event.target.id())
		// 모델에 없는 점은 없지만, 있으면 강조하지 않는다.
		if (!node) return
		hovered = node.path
		refresh(false)
		canvas.style.cursor = 'pointer'
		showTooltip(`${node.path}\n\n${node.description}`, event.renderedPosition)
	})
	/** 점에서 마우스가 나가면 강조를 되돌린다. */
	cy.on('mouseout', 'node', () => {
		hovered = null
		tooltip.hidden = true
		canvas.style.cursor = ''
		refresh(false)
	})
	/**
	 * 화살표에 마우스를 올리면 그 import 의 심볼 목록을 보여 준다.
	 * @param event cytoscape 의 mouseover 이벤트. `target` 은 화살표다
	 */
	cy.on('mouseover', 'edge', (event) => {
		const symbols = String(event.target.data('symbols'))
		showTooltip(`${event.target.data('source')} → ${event.target.data('target')}\n\n${symbols}`, event.renderedPosition)
	})
	/** 화살표에서 마우스가 나가면 심볼 목록을 숨긴다. */
	cy.on('mouseout', 'edge', () => {
		tooltip.hidden = true
	})
	graph = cy
	refresh()
}

/**
 * 그래프 영역 안의 마우스 위치 옆에 tooltip 을 놓는다.
 * @param text tooltip 글자
 * @param at 그래프 영역 기준의 마우스 위치
 */
function showTooltip(text: string, at: { x: number; y: number }): void {
	tooltip.textContent = text
	// 그래프 영역 기준 좌표를 viewport 좌표로 바꿔 가장자리 안쪽에 놓는다.
	const box = canvas.getBoundingClientRect()
	const x = box.left + at.x + 12
	const y = box.top + at.y + 12
	positionTooltip(tooltip, { left: x, top: y, right: x, bottom: y })
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
