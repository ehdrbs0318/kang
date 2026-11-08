// worker 와 테스트가 쓰는 Node 내장 모듈의 최소 타입이다.
// 플러그인에는 빌드와 설치 단계가 없으므로 @types/node 를 받을 곳이 없다. 그래서 쓰는 함수만 직접 적는다.

declare module 'node:fs' {
	/**
	 * 파일을 UTF-8 문자열로 읽는다.
	 * @param path 읽을 파일 경로
	 * @param encoding 항상 'utf8'
	 * @returns 파일 내용
	 * @throws 파일이 없거나 디렉토리이면 던진다
	 */
	export function readFileSync(path: string, encoding: 'utf8'): string
	/** `statSync` 가 돌려주는 파일 정보 중 쓰는 부분이다. */
	interface Stats {
		/** 보통 파일이면 true 다. */
		isFile(): boolean
		/** 바이트 단위 크기다. */
		size: number
	}
	/**
	 * 파일 정보를 읽는다. symlink 는 따라간다.
	 * @param path 볼 경로
	 * @returns 파일 정보
	 * @throws 경로가 없거나 볼 권한이 없으면 던진다
	 */
	export function statSync(path: string): Stats
	/**
	 * 파일에 글자를 쓴다. 파일이 없으면 만든다.
	 * @param path 쓸 파일 경로
	 * @param data 쓸 내용
	 * @throws 쓸 수 없으면 던진다
	 */
	export function writeFileSync(path: string, data: string): void
	/**
	 * 파일 크기를 바꾼다. 늘린 부분은 0 바이트다.
	 * @param path 바꿀 파일 경로
	 * @param length 새 크기 (바이트)
	 * @throws 파일이 없거나 쓸 수 없으면 던진다
	 */
	export function truncateSync(path: string, length: number): void
	/**
	 * 파일이나 디렉토리를 지운다.
	 * @param path 지울 경로
	 * @param options `recursive` 가 true 이면 디렉토리 안까지 지운다
	 * @throws 경로가 없거나 지울 수 없으면 던진다
	 */
	export function rmSync(path: string, options?: { recursive: boolean }): void
	/**
	 * symlink 를 모두 따라간 실제 절대 경로를 돌려준다.
	 * @param path 볼 경로
	 * @returns 실제 경로
	 * @throws 경로가 없거나 볼 권한이 없으면 던진다
	 */
	export function realpathSync(path: string): string
	/**
	 * 디렉토리를 만든다.
	 * @param path 만들 경로
	 * @throws 이미 있거나 만들 수 없으면 던진다
	 */
	export function mkdirSync(path: string): void
	/**
	 * 이름이 겹치지 않는 임시 디렉토리를 만든다.
	 * @param prefix 경로 앞부분. 뒤에 임의의 글자 6 개가 붙는다
	 * @returns 만든 디렉토리 경로
	 * @throws 만들 수 없으면 던진다
	 */
	export function mkdtempSync(prefix: string): string
	/**
	 * symlink 를 만든다.
	 * @param target symlink 가 가리킬 경로
	 * @param path 만들 symlink 경로
	 * @throws 이미 있거나 만들 수 없으면 던진다
	 */
	export function symlinkSync(target: string, path: string): void
}

declare module 'node:os' {
	/** 운영 체제의 임시 디렉토리 경로를 돌려준다. */
	export function tmpdir(): string
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
	/** 경로가 절대 경로이면 true 를 돌려준다. */
	export function isAbsolute(path: string): boolean
	/** 경로를 절대 경로로 만든다. 상대 경로는 앞의 인자를 기준으로 푼다. */
	export function resolve(...parts: string[]): string
	/** 운영 체제의 경로 구분 문자다 (예: '/'). */
	export const sep: string
}

declare module 'node:url' {
	/** `file:` URL 을 파일 경로로 바꾼다. */
	export function fileURLToPath(url: string | URL): string
}
