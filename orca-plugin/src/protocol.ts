// custom editor 와 worker 가 함께 쓰는 요청 계약이다. 양쪽 모두 이 파일의 타입을 읽는다.

/** 그래프 인덱스의 문서 한 줄이다. `hash` 는 정규화한 원문의 SHA-256 hex 값이다. */
export type GraphDoc = { path: string; description: string; hash: string }

/** 그래프 인덱스의 keyword 한 줄이다. `name` 은 계층을 `.` 으로 잇는다. */
export type GraphKeyword = { path: string; name: string; definition: string }

/** 그래프 인덱스의 import 관계 한 줄이다. `symbols` 는 kang 의 정식 주소다. */
export type GraphEdge = { from: string; to: string; symbols: string[] }

/** `kang.locate` 의 답이다. 받은 원문이 어느 workspace 의 어느 문서인지 알려 준다. */
export type Locate =
	| {
			status: 'ok'
			kind: 'doc' | 'graph'
			workspaceId: string
			path: string | null
			docs: GraphDoc[]
			edges: GraphEdge[]
			definitions: Record<string, string>
			stale: string[]
	  }
	| { status: 'notIndexed' | 'ambiguous' | 'remoteOnly' }

declare module 'orca' {
	interface PluginRequests {
		/** 열린 파일의 원문으로 그 파일의 workspace 와 문서를 찾는다. */
		'kang.locate': { input: { text: string }; output: Locate }
		/** 인덱스에 있는 문서 `<path>.kang` 을 Orca 의 편집기 탭으로 연다. */
		'kang.open': { input: { workspaceId: string; path: string }; output: { opened: boolean } }
	}
}
