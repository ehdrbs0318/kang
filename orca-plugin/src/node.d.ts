// worker 와 테스트가 쓰는 Node 내장 모듈의 최소 타입이다.
// shortcut: @types/node 를 쓰지 않으려고 쓰는 함수만 적는다. 쓰는 함수가 늘면 @types/node 로 바꾼다.

declare module 'node:fs' {
	/**
	 * 파일을 UTF-8 문자열로 읽는다.
	 * @param path 읽을 파일 경로
	 * @param encoding 항상 'utf8'
	 * @returns 파일 내용
	 * @throws 파일이 없거나 디렉토리이면 던진다
	 */
	export function readFileSync(path: string, encoding: 'utf8'): string
}

declare module 'node:crypto' {
	/** 해시 계산 객체다. */
	interface Hash {
		/** 입력을 더한다. */
		update(data: string, encoding: 'utf8'): Hash
		/** 결과를 hex 문자열로 돌려준다. */
		digest(encoding: 'hex'): string
	}
	/**
	 * 해시 계산 객체를 만든다.
	 * @param algorithm 알고리즘 이름 (예: 'sha256')
	 * @returns 해시 계산 객체
	 */
	export function createHash(algorithm: string): Hash
}

declare module 'node:path' {
	/** 경로를 이어 붙이고 정리한다. */
	export function join(...parts: string[]): string
	/** 경로를 절대 경로로 만든다. 상대 경로는 앞의 인자를 기준으로 푼다. */
	export function resolve(...parts: string[]): string
}

declare module 'node:url' {
	/** `file:` URL 을 파일 경로로 바꾼다. */
	export function fileURLToPath(url: string | URL): string
}
