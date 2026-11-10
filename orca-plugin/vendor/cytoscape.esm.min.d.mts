// cytoscape.esm.min.mjs 에서 이 플러그인이 쓰는 export 만 적은 타입이다.

/** 점 또는 화살표 하나의 정의다. `data.id` 가 같은 그래프 안에서 겹치면 안 된다. */
export type ElementDefinition = { group: 'nodes' | 'edges'; data: Record<string, string | number> }

/** 스타일 값이다. 함수이면 요소마다 불러서 값을 정한다. */
export type StyleValue = string | number | ((element: NodeSingular) => string | number)

/** 스타일 규칙 하나다. `selector` 에 맞는 요소에 `style` 을 준다. */
export type StylesheetJson = { selector: string; style: Record<string, StyleValue> }[]

/** 요소 묶음이다. 점 하나도 묶음이다. */
export type Collection = {
	/** @returns 묶음이 비어 있지 않으면 true */
	nonempty(): boolean
	/**
	 * class 를 더한다.
	 * @param classes 공백으로 나눈 class 이름
	 * @returns 같은 묶음
	 */
	addClass(classes: string): Collection
	/**
	 * class 를 뺀다.
	 * @param classes 공백으로 나눈 class 이름
	 * @returns 같은 묶음
	 */
	removeClass(classes: string): Collection
	/**
	 * 다른 묶음에 없는 요소만 남긴다.
	 * @param other 뺄 묶음
	 * @returns 새 묶음
	 */
	not(other: Collection): Collection
	/** @returns 이 묶음과, 이 묶음에 이어진 점과 화살표 */
	closedNeighborhood(): Collection
	/** @returns 이 묶음의 점에 이어진 화살표 */
	connectedEdges(): Collection
	/**
	 * 요소마다 함수를 부른다.
	 * @param fn 요소를 받는 함수
	 */
	forEach(fn: (element: NodeSingular) => void): void
	/**
	 * 요소마다 함수를 불러 결과를 모은다.
	 * @param fn 요소를 받는 함수
	 * @returns 결과 배열
	 */
	map<T>(fn: (element: NodeSingular) => T): T[]
}

/** 점 또는 화살표 하나다. */
export type NodeSingular = Collection & {
	/** @returns 요소 id. 점이면 문서 경로다 */
	id(): string
	/** @returns 그래프 영역 왼쪽 위 기준의 화면 좌표 */
	renderedPosition(): { x: number; y: number }
	/** @returns 점 중심의 모델 좌표 */
	position(): { x: number; y: number }
	/**
	 * 점 중심을 옮긴다.
	 * @param at 새 모델 좌표
	 * @returns 같은 점
	 */
	position(at: { x: number; y: number }): NodeSingular
	/**
	 * 점의 영역을 잰다.
	 * @param options `includeLabels` 가 true 이면 라벨까지 넣는다
	 * @returns 모델 좌표의 사각형
	 */
	boundingBox(options: { includeLabels: boolean }): { x1: number; y1: number; x2: number; y2: number }
	/**
	 * 요소의 data 값 하나를 읽는다.
	 * @param key data 의 key
	 * @returns 그 값
	 */
	data(key: string): string | number
}

/** 이벤트 객체다. */
export type EventObject = {
	/** 이벤트를 받은 요소다. 빈 곳의 이벤트이면 그래프 자신이다. */
	target: NodeSingular | Core
	/** 그래프 영역 왼쪽 위 기준의 화면 좌표다. */
	renderedPosition: { x: number; y: number }
}

/** selector 를 준 이벤트의 객체다. `target` 은 항상 요소다. */
export type ElementEvent = EventObject & { target: NodeSingular }

/** viewport 애니메이션의 목표다. */
export type ViewportAnimation = { center?: { eles: Collection }; fit?: { eles: Collection; padding?: number } }

/** 그래프 하나다. */
export type Core = {
	/**
	 * selector 에 맞는 요소의 이벤트를 받는다.
	 * @param events 공백으로 나눈 이벤트 이름
	 * @param selector 요소 selector
	 * @param handler 이벤트를 받는 함수
	 * @returns 같은 그래프
	 */
	on(events: string, selector: string, handler: (event: ElementEvent) => void): Core
	/**
	 * 그래프 전체의 이벤트를 받는다. 빈 곳을 누른 이벤트도 온다.
	 * @param events 공백으로 나눈 이벤트 이름
	 * @param handler 이벤트를 받는 함수
	 * @returns 같은 그래프
	 */
	on(events: string, handler: (event: EventObject) => void): Core
	/**
	 * 스타일을 통째로 바꾼다.
	 * @param style 새 스타일
	 * @returns 같은 그래프
	 */
	style(style: StylesheetJson): Core
	/**
	 * 배치 하나를 만든다.
	 * @param options 배치 이름과 설정. `animate: false` 이면 `run` 이 끝날 때 배치도 끝난다
	 * @returns `run` 으로 실행하는 배치
	 */
	layout(options: LayoutOptions): { run(): void }
	/** @returns 지금 확대 배율 */
	zoom(): number
	/**
	 * 확대 배율을 바꾼다.
	 * @param level 새 배율
	 */
	zoom(level: number): void
	/** 그래프를 화면 가운데에 놓는다. */
	center(): void
	/**
	 * 요소가 화면에 다 들어오게 확대 배율과 위치를 바꾼다.
	 * @param eles 맞출 요소. undefined 이면 모든 요소다
	 * @param padding 가장자리에 남기는 간격
	 */
	fit(eles: Collection | undefined, padding: number): void
	/**
	 * viewport 를 부드럽게 옮긴다.
	 * @param target 가운데에 둘 요소나 화면에 맞출 요소
	 * @param options 애니메이션 시간(ms)
	 */
	animate(target: ViewportAnimation, options: { duration: number }): void
	/** @returns 모든 점과 화살표 */
	elements(): Collection
	/** @returns 모든 점 */
	nodes(): Collection
	/** @returns 모든 화살표 */
	edges(): Collection
	/**
	 * id 로 요소를 찾는다.
	 * @param id 요소 id
	 * @returns 찾은 요소. 없으면 빈 묶음이다
	 */
	getElementById(id: string): NodeSingular
	/**
	 * 함수 안의 스타일 변경을 모아서 한 번에 그린다.
	 * @param fn 변경을 담은 함수
	 */
	batch(fn: () => void): void
	/** 그래프 영역의 크기를 다시 잰다. 영역 크기가 바뀐 뒤에 부른다. */
	resize(): void
	/** 그래프를 지우고 이벤트와 canvas 를 정리한다. */
	destroy(): void
}

/** 배치 설정이다. 이 플러그인은 `cose` 만 쓴다. */
export type LayoutOptions = {
	name: string
	animate?: boolean
	nodeDimensionsIncludeLabels?: boolean
	/** 점끼리 밀어내는 힘이다. 기본값은 2048 이다. */
	nodeRepulsion?: number
	/** 이어지지 않은 묶음 사이의 간격이다. 기본값은 40 이다. */
	componentSpacing?: number
	/** 배치 뒤 화면에 맞출 때 가장자리에 남기는 간격이다. */
	padding?: number
}

/** 그래프를 만들 때 주는 값이다. 배치는 만든 뒤에 `layout` 으로 실행한다. */
export type CytoscapeOptions = {
	container: HTMLElement
	elements: ElementDefinition[]
	style: StylesheetJson
	boxSelectionEnabled?: boolean
	minZoom?: number
	maxZoom?: number
}

/**
 * 그래프를 만든다.
 * @param options 그래프 설정
 * @returns 만든 그래프
 */
export default function cytoscape(options: CytoscapeOptions): Core
