// 플러그인의 worker 다. custom editor 의 `kang.locate`, `kang.open` 요청에 답한다.
// Orca 가 이 파일을 Node 프로세스로 실행하므로 실제 파일 시스템을 읽을 수 있다.
import { readFileSync, realpathSync, statSync } from 'node:fs'
import { join, sep } from 'node:path'
import type { OrcaWorkerApi } from 'orca/worker'
import { isLocal, locate, ReadLimit, readIndex, type FileReader } from './locate'

/** worker 가 읽는 파일의 최대 크기다. */
const MAX_SIZE = 32 * 1024 * 1024

/** 요청 하나가 workspace 하나에서 읽는 파일 크기의 합의 최대값이다. 인덱스는 커밋된 파일이라 아주 많은 큰 파일을 가리킬 수 있다. */
const MAX_TOTAL = 64 * 1024 * 1024

/**
 * 요청 하나가 쓰는 실제 파일 시스템을 만든다. workspace 안의 보통 파일만 읽는다.
 * @returns 파일 읽기. 읽은 파일 크기의 합을 이 읽기 하나 안에서 workspace 루트마다 센다
 */
export function realFs(): FileReader {
	const totals = new Map<string, number>()
	return {
		/**
		 * workspace 안의 파일을 UTF-8 글자로 읽는다. symlink 는 따라간 뒤의 실제 경로를 본다.
		 * @param root workspace 루트 절대 경로
		 * @param path 루트 기준 상대 경로
		 * @returns 파일 내용. 없거나, 실제 경로가 루트의 실제 경로 밖이거나, 보통 파일이 아니거나, MAX_SIZE 보다 크거나, 읽을 수 없으면 null
		 * @throws ReadLimit 이 읽기로 이 루트에서 읽은 파일 크기의 합이 MAX_TOTAL 을 넘으면 던진다
		 */
		read(root, path) {
			try {
				const realRoot = realpathSync(root)
				const real = realpathSync(join(root, path))
				// 커밋된 symlink 가 workspace 밖을 가리키면 없는 파일로 본다. 글자 검사만으로는 symlink 를 막을 수 없다.
				if (real !== realRoot && !real.startsWith(realRoot + sep)) return null
				const stat = statSync(real)
				// 디렉토리, FIFO, 장치 파일, 큰 파일은 읽지 않는다. `/dev/zero` 나 FIFO 를 읽으면 worker 가 멈춘다.
				if (!stat.isFile() || stat.size > MAX_SIZE) return null
				const total = (totals.get(root) ?? 0) + stat.size
				totals.set(root, total)
				// 이 루트에서 읽은 양이 한도를 넘으면 더 읽지 않는다. locate 가 이 workspace 를 건너뛴다.
				if (total > MAX_TOTAL) {
					console.debug(`workspace 에서 읽은 파일이 ${MAX_TOTAL} 바이트를 넘어서 건너뜁니다`, { root })
					throw new ReadLimit()
				}
				return readFileSync(real, 'utf8')
			} catch (error) {
				// 한도를 넘은 것은 locate 가 받도록 다시 던진다.
				if (error instanceof ReadLimit) throw error
				// 없는 파일, 권한 없음은 모두 읽을 수 없는 파일로 본다.
				return null
			}
		},
	}
}

/**
 * worker 를 시작한다. Orca 가 worker 를 띄울 때 한 번 부른다.
 * @param orca Orca 가 주는 worker API
 */
export default function activate(orca: OrcaWorkerApi): void {
	/**
	 * `kang.locate` 에 답한다. 열린 파일의 원문으로 workspace 와 문서를 찾는다.
	 * @param input.text 열린 파일의 원문
	 * @returns locate 의 답. 읽은 파일 크기의 합이 MAX_TOTAL 을 넘은 workspace 는 건너뛴다.
	 *   host 호출이 실패하면 그 오류로 거부하고, Orca 가 요청한 쪽에 실패를 전한다
	 */
	orca.panels.handle('kang.locate', async ({ text }) => {
		const { workspaces } = await orca.host.call('workspace.list')
		const focused = await orca.host.call('workspace.readContext')
		return locate(text, workspaces, focused, realFs())
	})

	/**
	 * `kang.open` 에 답한다. 그 workspace 의 인덱스에 있는 문서만 편집기 탭으로 연다.
	 * @param input.workspaceId 문서를 열 workspace id. 로컬 workspace 이어야 한다
	 * @param input.path 확장자를 뺀 문서 경로
	 * @returns 열었으면 `{ opened: true }`, 열 수 없는 요청이면 `{ opened: false }`. 인덱스에 없거나, 파일이 없거나, symlink 가 workspace 밖을 가리키면 열 수 없다.
	 *   host 호출이 실패하면 그 오류로 거부한다
	 */
	orca.panels.handle('kang.open', async ({ workspaceId, path }) => {
		const { workspaces } = await orca.host.call('workspace.list')
		const workspace = workspaces.find((w) => w.id === workspaceId && isLocal(w))
		// 인덱스 하나와 문서 하나는 각각 MAX_SIZE 까지이므로 이 요청은 MAX_TOTAL 을 넘지 않는다.
		const fs = realFs()
		const index = workspace ? readIndex(workspace.path, fs) : null
		// 인덱스에 있고 실제 경로가 workspace 안인 문서만 연다. readIndex 는 경로 글자를, realFs 는 symlink 를 따라간 실제 경로를 검사한다.
		if (!index || !index.docs.some((d) => d.path === path) || fs.read(workspace!.path, `${path}.kang`) === null) return { opened: false }
		await orca.host.call('editor.open', { workspaceId, path: `${path}.kang` })
		return { opened: true }
	})
}
