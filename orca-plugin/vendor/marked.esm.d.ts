// marked.esm.js 에서 이 플러그인이 쓰는 export 만 적은 타입이다.

/** marked 의 동기 파서다. */
export const marked: {
	/**
	 * 마크다운 블록을 HTML 로 바꾼다.
	 * @param src 마크다운 원문
	 * @param options `async: false` 이면 문자열을 돌려준다
	 * @returns HTML 문자열
	 */
	parse(src: string, options: { async: false }): string
	/**
	 * 마크다운 한 줄을 문단 태그 없이 HTML 로 바꾼다.
	 * @param src 마크다운 원문
	 * @param options `async: false` 이면 문자열을 돌려준다
	 * @returns HTML 문자열
	 */
	parseInline(src: string, options: { async: false }): string
}
