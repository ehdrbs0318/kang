// 문서 해시의 입력 텍스트를 만든다. worker(Node)와 frame(브라우저)이 함께 쓰므로 Node 모듈을 import 하지 않는다.
// 해시 함수는 쪽마다 다르다. worker 는 `node:crypto`, frame 은 Web Crypto 를 쓴다. 둘의 입력은 이 모듈이 정한다.
import { trimEnd, trimStart } from './ws'

/**
 * 해시 입력 텍스트를 `crates/kang/src/hash.rs` 의 `normalize` 와 같은 규칙으로 정규화한다.
 * 줄 끝 공백을 지우고, 연속 빈 줄을 하나로 합치고, 앞뒤 공백을 지운다.
 * CRLF 의 CR 은 줄 끝 공백이므로 지워진다.
 * @param text 정규화할 원문
 * @returns 정규화한 텍스트
 */
export function normalize(text: string): string {
	const lines: string[] = []
	// 줄마다 줄 끝 공백을 지우고, 앞 줄도 빈 줄인 빈 줄은 건너뛴다.
	for (const line of text.split('\n')) {
		const trimmed = trimEnd(line)
		if (trimmed === '' && lines.length > 0 && lines[lines.length - 1] === '') continue
		lines.push(trimmed)
	}
	return trimStart(trimEnd(lines.join('\n')))
}

/**
 * 문서 해시의 입력 텍스트를 만든다. 맨 앞 BOM(U+FEFF) 하나를 떼고 정규화한다.
 * 컴파일러가 BOM 을 떼고 읽으므로 같이 뗀다.
 * @param text 문서 원문
 * @returns SHA-256 에 UTF-8 로 넣을 텍스트
 */
export function hashInput(text: string): string {
	return normalize(text.replace(/^﻿/, ''))
}
