// 플러그인의 worker 다. custom editor 의 `kang.locate`, `kang.open` 요청에 답한다.
// Orca 가 이 파일을 Node 프로세스로 실행하므로 실제 파일 시스템을 읽을 수 있다.
import { readFileSync } from 'node:fs'
import type { OrcaWorkerApi } from 'orca/worker'
import { isLocal, locate, readIndex, type FileReader } from './locate'

/** 실제 파일 시스템이다. 파일이 없거나 디렉토리이면 null 을 돌려준다. */
const realFs: FileReader = {
	read(path) {
		try {
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
	orca.panels.handle('kang.locate', async ({ text }) => {
		const { workspaces } = await orca.host.call('workspace.list')
		const focused = await orca.host.call('workspace.readContext')
		return locate(text, workspaces, focused, realFs)
	})

	orca.panels.handle('kang.open', async ({ workspaceId, path }) => {
		const { workspaces } = await orca.host.call('workspace.list')
		const workspace = workspaces.find((w) => w.id === workspaceId && isLocal(w))
		const index = workspace ? readIndex(workspace.path, realFs) : null
		// 인덱스에 있는 문서만 연다. 그래서 요청이 workspace 밖의 파일을 열 수 없다.
		if (!index || !index.docs.some((d) => d.path === path)) return { opened: false }
		await orca.host.call('editor.open', { workspaceId, path: `${path}.kang` })
		return { opened: true }
	})
}
