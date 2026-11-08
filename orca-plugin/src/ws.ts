// 컴파일러(Rust `char::is_whitespace`)와 같은 공백 문자 묶음이다. worker 의 해시 계산과 렌더링 화면의 줄 규칙이 함께 쓴다.
// JavaScript 의 `trim` 은 U+FEFF 를 떼고 U+0085 를 떼지 않으므로 컴파일러와 다르다. 그래서 이 묶음을 따로 둔다.

/** Rust `char::is_whitespace` 와 같은 공백 문자 묶음이다. 정규식의 `[…]` 안에 넣는다. U+FEFF 는 넣지 않는다. */
export const WS = '\\t\\n\\v\\f\\r \\u0085\\u00A0\\u1680\\u2000-\\u200A\\u2028\\u2029\\u202F\\u205F\\u3000'
const SPACE = new RegExp(`[${WS}]`)
const LEADING = new RegExp(`^[${WS}]+`)

/**
 * 글자 하나가 공백 문자인지 본다.
 * @param char 볼 글자. 문자열 밖이라 undefined 이면 공백이 아니다
 * @returns 공백 문자이면 true
 */
export function isSpace(char: string | undefined): boolean {
	return char !== undefined && SPACE.test(char)
}

/**
 * 앞의 공백 문자를 지운다. Rust 의 `trim_start` 와 같다.
 * @param text 지울 글자
 * @returns 앞의 공백 문자를 뗀 글자
 */
export function trimStart(text: string): string {
	return text.replace(LEADING, '')
}

/**
 * 끝의 공백 문자를 지운다. Rust 의 `trim_end` 와 같다. `[공백]+$` 정규식은 공백 뒤에 글자가 오는 긴 줄에서 자리마다 다시 훑어 느리다. 그래서 끝에서부터 한 번만 훑는다.
 * @param text 지울 글자
 * @returns 끝의 공백 문자를 뗀 글자
 */
export function trimEnd(text: string): string {
	let end = text.length
	// 끝에서부터 공백 문자가 아닌 글자를 만날 때까지 줄인다. WS 의 문자는 모두 UTF-16 한 단위다.
	while (end > 0 && SPACE.test(text[end - 1])) end--
	return text.slice(0, end)
}

/**
 * 앞뒤의 공백 문자를 지운다. Rust 의 `trim` 과 같다.
 * @param text 지울 글자
 * @returns 앞뒤의 공백 문자를 뗀 글자
 */
export function trim(text: string): string {
	return trimStart(trimEnd(text))
}
