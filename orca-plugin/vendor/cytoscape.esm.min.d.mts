// cytoscape.esm.min.mjs 에서 이 플러그인이 쓰는 export 만 적은 타입이다.

/** 점 또는 화살표 하나의 정의다. `data.id` 가 같은 그래프 안에서 겹치면 안 된다. */
export type ElementDefinition = { group: 'nodes' | 'edges'; data: Record<string, string | number> }

/** 스타일 규칙 하나다. `selector` 에 맞는 요소에 `style` 을 준다. */
export type StylesheetJson = { selector: string; style: Record<string, string | number> }[]

/** 이벤트를 받은 점 또는 화살표다. */
export type SingularElement = {
	/**
	 * 요소의 data 값 하나를 읽는다.
	 * @param key data 의 key
	 * @returns 그 값
	 */
	data(key: string): string | number
}

/** 이벤트 객체다. */
export type EventObject = {
	/** 이벤트를 받은 요소다. */
	target: SingularElement
	/** 그래프 영역 왼쪽 위 기준의 화면 좌표다. */
	renderedPosition: { x: number; y: number }
}

/** 그래프 하나다. */
export type Core = {
	/**
	 * selector 에 맞는 요소의 이벤트를 받는다.
	 * @param events 공백으로 나눈 이벤트 이름
	 * @param selector 요소 selector
	 * @param handler 이벤트를 받는 함수
	 * @returns 같은 그래프
	 */
	on(events: string, selector: string, handler: (event: EventObject) => void): Core
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
}

/** 그래프를 만들 때 주는 값이다. 배치는 만든 뒤에 `layout` 으로 실행한다. */
export type CytoscapeOptions = {
	container: HTMLElement
	elements: ElementDefinition[]
	style: StylesheetJson
	boxSelectionEnabled?: boolean
}

/**
 * 그래프를 만든다.
 * @param options 그래프 설정
 * @returns 만든 그래프
 */
export default function cytoscape(options: CytoscapeOptions): Core
