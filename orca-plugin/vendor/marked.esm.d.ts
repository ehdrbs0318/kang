// marked.esm.js 에서 이 플러그인이 쓰는 export 만 적은 타입이다.

/** lexer 가 만든 token 하나다. 확장 token 은 자기 필드를 더 가진다. */
export type Token = { type: string; raw: string; text?: string; tokens?: Token[]; [key: string]: unknown }

/** 확장 tokenizer 안의 `this` 다. */
export type TokenizerThis = {
	lexer: {
		/**
		 * 인라인 원문을 나중에 token 으로 바꾸도록 줄을 세운다.
		 * @param src 인라인 원문
		 * @param tokens 결과 token 을 넣을 배열
		 * @returns 같은 배열
		 */
		inline(src: string, tokens: Token[]): Token[]
	}
}

/** 확장 renderer 안의 `this` 다. */
export type RendererThis = {
	parser: {
		/**
		 * 인라인 token 들을 HTML 로 바꾼다.
		 * @param tokens 인라인 token 배열
		 * @returns HTML 문자열
		 */
		parseInline(tokens: Token[]): string
	}
}

/** 문법 확장 하나다. */
export type TokenizerExtension = {
	name: string
	level: 'block' | 'inline'
	/** 원문에서 이 문법이 처음 나올 수 있는 위치다. 없으면 -1 또는 undefined 다. */
	start?(src: string): number | undefined
	tokenizer(this: TokenizerThis, src: string, tokens: Token[]): Token | undefined
	renderer(this: RendererThis, token: Token): string
}

/** `use` 에 넘기는 설정이다. */
export type MarkedExtension = {
	extensions?: TokenizerExtension[]
	/** 기본 tokenizer 의 메소드를 바꾼다. undefined 를 돌려주면 아무것도 읽지 않고, false 를 돌려주면 기본 tokenizer 가 읽는다. */
	tokenizer?: {
		fences?(src: string): Token | undefined
		code?(src: string): Token | undefined
		html?(src: string): Token | undefined
		lheading?(src: string): Token | undefined
		def?(src: string): Token | undefined
		heading?(src: string): Token | undefined | false
	}
	/** 기본 renderer 의 메소드를 바꾼다. 여기서는 `html` 과 `image` 만 쓴다. */
	renderer?: {
		html?(token: { text: string }): string
		/** `token.text` 는 이미지의 alt 원문이다. */
		image?(token: { text: string }): string
	}
}

/** 설정을 따로 갖는 marked 인스턴스다. */
export class Marked {
	/**
	 * 인스턴스를 만든다.
	 * @param extensions 처음부터 쓸 설정들
	 */
	constructor(...extensions: MarkedExtension[])
	/**
	 * 원문을 token 으로 나눈다.
	 * @param src 마크다운 원문
	 * @returns 최상위 token 배열
	 */
	lexer(src: string): Token[]
	/**
	 * 최상위 token 들을 HTML 로 바꾼다.
	 * @param tokens `lexer` 가 만든 token 이나 같은 꼴의 token 배열
	 * @returns HTML 문자열
	 */
	parser(tokens: Token[]): string
}
