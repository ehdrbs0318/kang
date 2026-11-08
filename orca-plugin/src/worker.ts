// 플러그인의 worker 다. custom editor 의 `kang.locate`, `kang.open` 요청에 답한다.
// Orca 가 이 파일을 Node 프로세스로 실행하므로 실제 파일 시스템을 읽을 수 있다.
import { readFileSync, statSync } from 'node:fs'
import type { OrcaWorkerApi } from 'orca/worker'
import { isLocal, locate, readIndex, type FileReader } from './locate'

/** worker 가 읽는 파일의 최대 크기다. 32 MiB 이다. */
const MAX_SIZE = 32 * 1024 * 1024

/** 실제 파일 시스템이다. 보통 파일만 읽는다. */
export const realFs: FileReader = {
	/**
	 * 파일을 UTF-8 글자로 읽는다. symlink 는 따라간 뒤의 파일을 본다.
	 * @param path 절대 경로
	 * @returns 파일 내용. 없거나, 보통 파일이 아니거나, 32 MiB 보다 크거나, 읽을 수 없으면 null
	 */
	read(path) {
		try {
			const stat = statSync(path)
			// 디렉토리, FIFO, 장치 파일, 큰 파일은 읽지 않는다. `/dev/zero` 나 FIFO 를 읽으면 worker 가 멈춘다.
			if (!stat.isFile() || stat.size > MAX_SIZE) return null
			return readFileSync(path, 'utf8')
		} catch {
			// 없는 파일, 디렉토리, 권한 없음은 모두 읽을 수 없는 파일로 본다.
			return null
		}
	},
}

/**
 * worker 를 시작한다. Orca 가 worker 를 띄울 때 한 번 부른다.
 * @param orca Orca 가 주는 worker API
 */
export default function activate(orca: OrcaWorkerApi): void {
	/**
	 * `kang.locate` 에 답한다. 열린 파일의 원문으로 workspace 와 문서를 찾는다.
	 * @param input.text 열린 파일의 원문
	 * @returns locate 의 답. host 호출이 실패하면 그 오류로 거부하고, Orca 가 요청한 쪽에 실패를 전한다
	 */
	orca.panels.handle('kang.locate', async ({ text }) => {
		const { workspaces } = await orca.host.call('workspace.list')
		const focused = await orca.host.call('workspace.readContext')
		return locate(text, workspaces, focused, realFs)
	})

	/**
	 * `kang.open` 에 답한다. 그 workspace 의 인덱스에 있는 문서만 편집기 탭으로 연다.
	 * @param input.workspaceId 문서를 열 workspace id. 로컬 workspace 이어야 한다
	 * @param input.path 확장자를 뺀 문서 경로
	 * @returns 열었으면 `{ opened: true }`, 열 수 없는 요청이면 `{ opened: false }`. host 호출이 실패하면 그 오류로 거부한다
	 */
	orca.panels.handle('kang.open', async ({ workspaceId, path }) => {
		const { workspaces } = await orca.host.call('workspace.list')
		const workspace = workspaces.find((w) => w.id === workspaceId && isLocal(w))
		const index = workspace ? readIndex(workspace.path, realFs) : null
		// 인덱스에 있는 문서만 연다. readIndex 가 workspace 밖을 가리키는 경로가 있는 인덱스를 버리므로, 요청이 workspace 밖의 파일을 열 수 없다.
		if (!index || !index.docs.some((d) => d.path === path)) return { opened: false }
		await orca.host.call('editor.open', { workspaceId, path: `${path}.kang` })
		return { opened: true }
	})
}
