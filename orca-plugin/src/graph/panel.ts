// 그래프 화면의 머리줄, 왼쪽 문서 목록, 오른쪽 상세 패널을 DOM 으로 만든다. cytoscape 는 쓰지 않는다.
// 인덱스의 글자는 모두 textContent 로만 넣는다. 그래서 문서 내용이 HTML 로 해석되지 않는다.
import type { DocNode, GraphModel, Link } from './model'

/** 폴더 색이다. 밝은 테마와 어두운 테마의 배경 모두에서 읽히는 중간 밝기의 색을 쓴다. 폴더가 더 많으면 처음부터 다시 쓴다. */
export const PALETTE = ['#4f8ff7', '#f59e0b', '#10b981', '#e8590c', '#a855f7', '#06b6d4', '#ec4899', '#84cc16']

/**
 * 폴더 순번의 색을 고른다.
 * @param index `DocNode.folderIndex`
 * @returns `PALETTE` 의 색
 */
export function folderColor(index: number): string {
	return PALETTE[index % PALETTE.length]
}

/**
 * 태그 이름, class, 글자로 요소를 만든다.
 * @param tag 태그 이름
 * @param className class 속성. 비우면 넣지 않는다
 * @param text textContent. 없으면 넣지 않는다
 * @returns 만든 요소
 */
function el<K extends keyof HTMLElementTagNameMap>(tag: K, className = '', text?: string): HTMLElementTagNameMap[K] {
	const node = document.createElement(tag)
	if (className) node.className = className
	if (text !== undefined) node.textContent = text
	return node
}

/**
 * 폴더 이름을 화면에 쓰는 글자로 바꾼다.
 * @param folder `folderOf` 의 결과
 * @returns 루트이면 `저장소 루트`, 아니면 그대로
 */
function folderName(folder: string): string {
	return folder === '/' ? '저장소 루트' : folder
}

/**
 * 폴더 색 점을 만든다.
 * @param index 폴더 순번
 * @returns 색을 칠한 작은 원
 */
function dot(index: number): HTMLElement {
	const d = el('span', 'kg-dot')
	d.style.background = folderColor(index)
	return d
}

/**
 * 역할 배지를 만든다.
 * @param node 문서
 * @returns 허브와 고립 문서의 배지. 보통 문서는 null
 */
function badge(node: DocNode): HTMLElement | null {
	// 보통 문서는 배지가 없다.
	if (node.role === 'normal') return null
	return el('span', `kg-badge kg-badge-${node.role}`, node.role === 'hub' ? '★ 허브' : '○ 고립')
}

/** 화면 DOM 을 만들 때 받는 동작이다. */
export type PanelActions = {
	/** 문서 하나를 고른다. null 이면 고르기를 푼다. `center` 가 true 이면 그래프를 그 점으로 옮긴다. */
	select(path: string | null, center: boolean): void
	/** 문서를 Orca 편집기 탭으로 연다. */
	open(path: string): void
	/** 검색어가 바뀌었다. */
	search(query: string): void
	/** 그래프 전체를 화면에 맞춘다. */
	fit(): void
}

/** `createPanels` 가 만든 요소와, 상태가 바뀔 때 부르는 함수다. */
export type Panels = {
	header: HTMLElement
	list: HTMLElement
	detail: HTMLElement
	/** 모델이 바뀌면 머리줄과 목록을 다시 그린다. */
	setModel(model: GraphModel): void
	/** 고른 문서와 검색 결과를 목록과 상세 패널에 반영한다. */
	update(selected: string | null, visible: Set<string>): void
}

/**
 * 머리줄, 목록, 상세 패널을 만든다. 처음에는 비어 있고 `setModel` 로 채운다.
 * @param actions 사용자가 누르거나 입력할 때 부를 동작
 * @returns 만든 요소와 갱신 함수
 */
export function createPanels(actions: PanelActions): Panels {
	const header = el('header', 'kg-header')
	const title = el('div', 'kg-title')
	title.append(el('span', 'kg-logo', '◈'), el('b', '', '문서 그래프'))
	const stats = el('div', 'kg-stats')
	const legend = el('div', 'kg-legend')
	const tools = el('div', 'kg-tools')
	const searchBox = el('input', 'kg-search')
	searchBox.type = 'search'
	searchBox.placeholder = '문서 찾기'
	searchBox.setAttribute('aria-label', '문서 찾기')
	searchBox.addEventListener('input', () => actions.search(searchBox.value))
	const fitButton = el('button', 'kg-button', '전체 보기')
	fitButton.type = 'button'
	fitButton.addEventListener('click', () => actions.fit())
	tools.append(searchBox, fitButton)
	header.append(title, stats, legend, tools)

	const list = el('nav', 'kg-list')
	list.setAttribute('aria-label', '문서 목록')
	const detail = el('aside', 'kg-detail')
	detail.setAttribute('aria-label', '문서 상세')

	let model: GraphModel = { nodes: [], edges: [], folders: [] }
	let byPath = new Map<string, DocNode>()
	/** 목록의 문서 버튼. 고른 문서와 검색 결과를 표시할 때 쓴다. */
	let items = new Map<string, HTMLElement>()
	/** 목록의 폴더 묶음. 검색으로 문서가 모두 숨으면 폴더 이름도 숨긴다. */
	let groups = new Map<string, HTMLElement>()

	/**
	 * 통계 칸 하나를 만든다.
	 * @param label 이름
	 * @param value 값
	 * @returns 이름과 값을 담은 칸
	 */
	function stat(label: string, value: number): HTMLElement {
		const s = el('div', 'kg-stat')
		s.append(el('span', '', label), el('b', '', String(value)))
		return s
	}

	/**
	 * 다른 문서로 가는 연결 목록을 만든다. 문서 이름을 누르면 그 문서를 고른다.
	 * @param heading 목록 제목
	 * @param links 연결 목록
	 * @param empty 연결이 없을 때의 문구
	 * @returns 제목과 목록을 담은 구역
	 */
	function linkSection(heading: string, links: Link[], empty: string): HTMLElement {
		const section = el('section', 'kg-section')
		section.append(el('h3', '', `${heading} ${links.length}`))
		// 연결이 없으면 안내 문구만 둔다.
		if (links.length === 0) {
			section.append(el('p', 'kg-empty', empty))
			return section
		}
		const ul = el('ul', 'kg-links')
		for (const link of links) {
			const other = byPath.get(link.path)
			const li = el('li')
			const button = el('button', 'kg-link')
			button.type = 'button'
			button.title = link.path
			// 상대 문서는 buildModel 이 걸러서 항상 있지만, 없으면 경로만 보여 준다.
			if (other) button.append(dot(other.folderIndex))
			button.append(el('span', '', other?.label ?? link.path))
			button.addEventListener('click', () => actions.select(link.path, true))
			li.append(button, el('div', 'kg-symbols', link.symbols.join(', ')))
			ul.append(li)
		}
		section.append(ul)
		return section
	}

	/**
	 * 상세 패널을 고른 문서로 채운다.
	 * @param node 고른 문서. null 이면 안내 문구를 보여 준다
	 */
	function renderDetail(node: DocNode | null): void {
		detail.replaceChildren()
		// 고른 문서가 없으면 쓰는 법을 알려 준다.
		if (!node) {
			const hint = el('div', 'kg-hint')
			hint.append(
				el('b', '', '문서를 고르세요'),
				el('p', '', '점이나 왼쪽 목록을 누르면 여기에 설명과 연결이 보입니다. 점을 두 번 누르면 문서가 열립니다.'),
			)
			detail.append(hint)
			return
		}
		const head = el('div', 'kg-detail-head')
		const name = el('h2')
		name.append(dot(node.folderIndex), el('span', '', node.label))
		const role = badge(node)
		if (role) name.append(role)
		const openButton = el('button', 'kg-button kg-primary', '문서 열기')
		openButton.type = 'button'
		openButton.addEventListener('click', () => actions.open(node.path))
		head.append(name, el('code', 'kg-path', `${node.path}.kang`), openButton)
		detail.append(head, el('p', 'kg-description', node.description))
		detail.append(linkSection('가져오는 문서', node.outgoing, '이 문서는 다른 문서를 import 하지 않습니다.'))
		detail.append(linkSection('가져가는 문서', node.incoming, '이 문서를 import 하는 문서가 없습니다.'))
		const kw = el('section', 'kg-section')
		kw.append(el('h3', '', `keyword ${node.keywords.length}`))
		// 선언한 keyword 가 없으면 안내 문구만 둔다.
		if (node.keywords.length === 0) kw.append(el('p', 'kg-empty', '이 문서가 선언한 keyword 가 없습니다.'))
		else {
			const dl = el('dl', 'kg-keywords')
			// keyword 마다 이름과 정의를 한 쌍으로 넣는다.
			for (const k of node.keywords) dl.append(el('dt', '', k.name), el('dd', '', k.definition))
			kw.append(dl)
		}
		detail.append(kw)
	}

	return {
		header,
		list,
		detail,
		setModel(next) {
			model = next
			byPath = new Map(model.nodes.map((n) => [n.path, n]))
			const keywordCount = model.nodes.reduce((sum, n) => sum + n.keywords.length, 0)
			stats.replaceChildren(stat('문서', model.nodes.length), stat('연결', model.edges.length), stat('keyword', keywordCount))
			legend.replaceChildren()
			// 폴더마다 색 점과 이름을 범례에 넣는다.
			model.folders.forEach((folder, index) => {
				const item = el('span', 'kg-legend-item')
				item.title = folder
				item.append(dot(index), el('span', '', folderName(folder)))
				legend.append(item)
			})
			list.replaceChildren()
			items = new Map()
			groups = new Map()
			// 폴더별로 문서를 묶는다. 묶음 안의 순서는 이름순이다.
			model.folders.forEach((folder, index) => {
				const group = el('div', 'kg-group')
				const heading = el('div', 'kg-group-name')
				heading.append(dot(index), el('span', '', folderName(folder)))
				group.append(heading)
				const members = model.nodes.filter((n) => n.folder === folder).sort((a, b) => a.label.localeCompare(b.label))
				for (const node of members) {
					const item = el('button', 'kg-item')
					item.type = 'button'
					item.title = node.path
					item.append(el('span', 'kg-item-name', node.label))
					const mark = node.role === 'hub' ? '★' : node.role === 'orphan' ? '○' : ''
					item.append(el('span', 'kg-item-meta', `${mark} ↙${node.incoming.length} ↗${node.outgoing.length}`.trim()))
					item.addEventListener('click', () => actions.select(node.path, true))
					item.addEventListener('dblclick', () => actions.open(node.path))
					items.set(node.path, item)
					group.append(item)
				}
				groups.set(folder, group)
				list.append(group)
			})
			renderDetail(null)
		},
		update(selected, visible) {
			// 목록에서 고른 문서를 표시하고 검색에 맞지 않는 문서를 숨긴다.
			for (const [path, item] of items) {
				item.classList.toggle('is-selected', path === selected)
				item.hidden = !visible.has(path)
			}
			// 문서가 모두 숨은 폴더는 이름도 숨긴다.
			for (const [folder, group] of groups) group.hidden = !model.nodes.some((n) => n.folder === folder && visible.has(n.path))
			renderDetail(selected ? (byPath.get(selected) ?? null) : null)
		},
	}
}
