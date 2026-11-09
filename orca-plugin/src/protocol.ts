// custom editor 와 worker 가 함께 쓰는 요청 계약이다. 양쪽 모두 이 파일의 타입을 읽는다.

/** 문서 본문의 참조 문자열 하나가 가리키는 심볼이다. `local` 은 참조 문자열, `address` 는 kang 의 정식 주소, `doc` 은 그 심볼을 선언한 문서 경로다. */
export type ScopeEntry = { local: string; address: string; doc: string }

/** 그래프 인덱스의 문서 한 줄이다. `hash` 는 맨 앞 BOM 을 떼고 정규화한 원문의 SHA-256 hex 값이다. `scope` 는 `kang build` 가 푼 본문 참조 목록이다. */
export type GraphDoc = { path: string; description: string; hash: string; scope: ScopeEntry[] }

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
			/** kang 프로젝트 루트의 workspace 기준 상대 경로다. workspace 루트이면 빈 문자열이다. `path` 와 문서 경로는 이 루트 기준이다. */
			root: string
			path: string | null
			docs: GraphDoc[]
			edges: GraphEdge[]
			definitions: Record<string, string>
			scope: ScopeEntry[]
			stale: string[]
	  }
	| { status: 'notIndexed' | 'ambiguous' | 'remoteOnly' }

declare module 'orca' {
	interface PluginRequests {
		/** 열린 파일의 원문으로 그 파일의 workspace 와 문서를 찾는다. */
		'kang.locate': { input: { text: string }; output: Locate }
		/** kang 프로젝트 `root` 의 인덱스에 있는 문서 `<root>/<path>.kang` 을 Orca 의 편집기 탭으로 연다. `root` 는 `kang.locate` 답의 값이다. */
		'kang.open': { input: { workspaceId: string; root: string; path: string }; output: { opened: boolean } }
	}
}
