# V0006 Orca 용 kang 플러그인 구현 플랜

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `kang build` 가 그래프 인덱스를 쓰고, Orca 플러그인이 `.kang` 문서를 렌더링하고 문서 그래프를 보여 준다.

**Architecture:** Rust 쪽은 `crates/kang/src/graph.rs` 가 그래프 인덱스 JSON 을 쓰고, `kang build` 가 성공하면 그 파일을 저장소 루트의 `.kang/graph.kang.json` 에 쓴다. 플러그인은 `orca-plugin/` 에 있고, custom editor 두 개와 worker 하나로 나뉜다. 편집기는 받은 원문을 worker 에 보내고, worker 가 해시 계산, workspace 고르기, 파일 읽기, 문서 열기를 맡는다.

**Tech Stack:** Rust (기존 `kang` 크레이트, 외부 의존성은 `sha2` 하나), Orca plugin API 1 (Orca 1.4.253), TypeScript (빌드 단계 없음), `marked` 15.0.12, `cytoscape` 3.31.0.

**Spec:** `plans/TODOS/V0005-orca-kang-plugin.md`

## Global Constraints

- `kang` 크레이트에 외부 의존성을 더하지 않는다. JSON 은 직접 쓴다 (V0001 §10.1).
- 그래프 인덱스는 같은 문서에서 항상 같은 바이트다. 만든 시각을 넣지 않는다.
- `kang build` 가 컴파일 error 로 끝나면 그래프 인덱스를 쓰지 않는다.
- kang 이 만들지 않은 파일은 덮어쓰지 않는다. 판단 기준은 첫 줄 `{"kang":"graph/1",` 이다.
- 저장소의 `.kang` 이 저장소 밖을 가리키면 `kang build` 는 그래프 인덱스를 쓰지 않고 종료 코드 2 로 끝난다.
- 플러그인 id 는 `kang`, 폴더는 `orca-plugin/`, `engines.orca` 는 `>=1.4.248` 이다.
- 플러그인은 네트워크를 쓰지 않는다. `marked` 와 `cytoscape` 의 ESM 파일을 `orca-plugin/vendor/` 에 넣는다.
- 렌더링 화면은 읽기 전용이다. 플러그인은 `.kang` 파일을 바꾸지 않는다.
- 문서 해시는 맨 앞 BOM(U+FEFF) 하나를 뗀 원문을 `crates/kang/src/hash.rs` 의 `normalize` 와 같은 규칙으로 정규화한 텍스트의 SHA-256 전체 hex 값이다. Rust 는 `doc_hash`, TypeScript 는 `docHash` 가 계산한다. `normalize` 와 `rev` 는 BOM 을 떼지 않는다. 그래야 rev 핀이 바뀌지 않는다. TypeScript 쪽 계산은 worker 에만 있다.
- 원격 workspace 는 지원하지 않는다. `hostId` 가 null 이나 `'local'` 이면 로컬 workspace 이고, 그 밖의 값이면 원격 workspace 다.
- 문서와 주석은 `/pstack:technical-writing` 규칙과 한국어 쉬운 문체를 따른다. 주석은 한국어다.
- 플러그인은 `@types/node` 를 쓰지 않는다. `orca-plugin/src/node.d.ts` 에 쓰는 함수의 타입만 직접 적는다. 플러그인에는 빌드와 설치 단계가 없어서 패키지를 받을 곳이 없다. `kang` 크레이트에도 의존성을 더하지 않는다. 그래서 이 파일은 임시 방법이 아니라 정한 방법이다.
- 플러그인은 저장소의 기존 관례를 따라 함수의 시작과 끝에 로그를 남기지 않고, 실패만 `console.debug` 로 남긴다.

## Interfaces

이 절은 모든 Task 가 쓰는 계약이다.

### 그래프 인덱스 `.kang/graph.kang.json`

```json
{"kang":"graph/1",
"docs":[
{"path":"CONTEXT","description":"…","hash":"<64 hex>","scope":[{"local":"문서","address":"CONTEXT.문서","doc":"CONTEXT"}]}
],
"keywords":[
{"path":"CONTEXT","name":"rev 핀","definition":"…"}
],
"edges":[
{"from":"docs/adr/0001-no-tombstones-for-retired-terms","to":"CONTEXT","symbols":["CONTEXT.묘비","CONTEXT.심볼"]}
]}
```

- 한 항목은 한 줄이다. 목록이 비면 `"docs":[\n]` 처럼 쓴다.
- `docs` 는 문서 경로 순서다. `keywords` 는 문서 경로 순서, 그 안에서 선언 순서다. `edges` 는 `(from, to)` 순서이고, `symbols` 는 import 순서다.
- keyword `name` 은 계층을 `.` 으로 잇는다 (예: `결제수단.카드`).
- `symbols` 는 kang 의 정식 주소다 (`docs/A.결제`, `docs/A#topic`, `docs/A!exception`). `check::symbol_address` 로 만든다.
- `scope` 는 그 문서 본문의 참조 문자열이 가리키는 심볼 목록이다. `local` 순서다. `local` 은 본문에 쓸 수 있는 이름, `address` 는 정식 주소, `doc` 은 그 심볼을 선언한 문서다. `SymbolTable::scope` 의 로컬 이름(자기 선언과 import 의 alias 나 정식 이름)과 `iknow` 대상을 담는다. `iknow` 대상은 정식 주소가 곧 `local` 이다. `local` 이 겹치면 스코프의 것을 남긴다.
- 문자열 escape 는 `"`, `\`, U+0000~U+001F 만 한다. 제어 문자는 `\u00XX` 로 쓴다.

### worker 요청 (`orca-plugin/src/protocol.ts`)

```ts
type ScopeEntry = { local: string; address: string; doc: string }
type GraphDoc = { path: string; description: string; hash: string; scope: ScopeEntry[] }
type Locate =
	| { status: 'ok'; kind: 'doc' | 'graph'; workspaceId: string; path: string | null; docs: GraphDoc[]; edges: GraphEdge[]; definitions: Record<string, string>; scope: ScopeEntry[]; stale: string[] }
	| { status: 'notIndexed' | 'ambiguous' | 'remoteOnly' }
'kang.locate': { input: { text: string }; output: Locate }
'kang.open': { input: { workspaceId: string; path: string }; output: { opened: boolean } }
```

- `kind` 는 받은 원문이 그래프 인덱스 파일과 같으면 `'graph'`, 문서 해시가 맞으면 `'doc'` 이다.
- `path` 는 `kind: 'doc'` 일 때 그 문서의 경로다. `kind: 'graph'` 이면 null 이다.
- `definitions` 의 key 는 keyword 주소 (`CONTEXT.문서`)이고 값은 정의다. 선언한 문서의 지금 해시가 인덱스와 다른 keyword 는 뺀다.
- `scope` 는 `kind: 'doc'` 일 때 그 문서의 `scope` 다. `kind: 'graph'` 이면 빈 목록이다.
- `stale` 은 지금 해시가 인덱스와 다른 문서 경로들이다.
- 인덱스의 항목 하나라도 필드 타입이 틀리면 worker 는 그 인덱스 전체를 없는 것으로 본다. 다른 workspace 는 그대로 찾는다.
- 인덱스는 커밋된 파일이라 어느 저장소에서든 올 수 있다. 그래서 worker 는 다음 인덱스도 없는 것으로 본다. `docs[].path`, `scope[].doc`, `keywords[].path`, `edges[].from`, `edges[].to` 중 하나라도 비었거나, 절대 경로이거나, 역슬래시가 있거나, `.` 나 `..` 조각이 있는 인덱스다. `docs[].path` 가 겹치는 인덱스와 `edges` 에 같은 `(from, to)` 쌍이 둘 이상인 인덱스도 없는 것으로 본다.
- worker 는 workspace 안의 보통 파일만 읽는다. symlink 는 따라간 뒤의 실제 경로를 본다. 실제 경로가 workspace 루트의 실제 경로 밖이면 없는 파일로 본다. 디렉토리, FIFO, 장치 파일, 32 MiB 보다 큰 파일도 없는 파일로 본다.
- 한 workspace 안에서 해시가 같은 문서가 둘 이상이면 `ambiguous` 다.
- 로컬 후보가 없을 때 focus 된 workspace(`workspace.list` 에서 이름과 branch 가 같은 것)가 원격이면 `remoteOnly` 다. 그 밖에는 `notIndexed` 다.
- `kang.open` 은 `path` 가 그 workspace 의 인덱스에 있고 `<path>.kang` 을 위 규칙으로 읽을 수 있을 때만 그 파일을 연다. 그 밖에는 `{ opened: false }` 다.

### 렌더링 파서 (`orca-plugin/src/render/parse.ts`)

```ts
export type KangView = { description: string; imports: { target: string; local: string }[]; body: string }
export function parseKang(text: string): KangView
export function renderBody(body: string): string      // 참조는 <a class="kang-ref" data-ref="참조 문자열">…</a> 로 그린다
export function lookupRef(scope: ScopeEntry[], ref: string): ScopeEntry | undefined
```

- 이름 풀이는 컴파일러가 한다. 화면은 `lookupRef` 로 `scope` 에서 찾기만 한다. 참조 문자열을 그대로 `local` 과 비교한다. 없으면 링크 없이 글자로 둔다.
- 참조 바로 앞의 `#` 와 `!` 는 참조에 넣지 않고 글자로 둔다. 컴파일러는 본문에서 백틱 쌍만 참조로 본다. 참조 앞의 `#` 가 문법인 자리는 keyword 줄 끝의 topic 뿐이다. 그 `#` 는 떼고 참조만 그린다.
- frontmatter 는 컴파일러와 같은 규칙으로 찾는다. 맨 앞 BOM 하나와 빈 줄을 건너뛴 첫 줄이 앞뒤 공백을 뗀 `---` 이면 열리고, 다음에 앞뒤 공백을 뗀 `---` 줄에서 닫힌다.
- `description` 은 앞뒤 공백을 뗀 줄이 `description:` 로 시작하는 첫 줄의 값이다. 값은 앞뒤 공백만 떼고 따옴표는 그대로 둔다.
- 코드 영역은 marked 가 아니라 컴파일러와 같은 줄 규칙으로 먼저 정한다. 앞 공백을 뗀 줄이 백틱 3개 이상으로 시작하면 펜스가 열린다. 여는 줄보다 적지 않은 백틱으로 시작하는 줄이 펜스를 닫는다. 그 줄에 글자가 더 있어도 닫는다. 목록이나 인용 안의 백틱 줄은 펜스가 아니다. 문단 바로 다음 줄이나 4칸 넘게 들여 쓴 줄도 같은 규칙을 따른다.
- 펜스 사이의 글은 marked 가 그린다. `~~~` 펜스, 들여 쓴 코드, HTML 블록, setext 제목(문단 아래의 `---` 나 `===`), 링크 정의는 없다. `<div>` 나 `<!--` 로 시작하는 줄도 글자이고, 그 안의 참조와 topic 제목은 그대로 그린다. `[참고]: …` 로 시작하는 줄도 글자이고, 그 줄의 참조는 링크다.
- keyword 줄 끝의 topic 표시 `#` 는 앞에 공백이 있을 때만 topic 표시다.
- 컴파일러처럼 앞 공백과 탭을 뗀 줄이 `##` 로 시작하고 그 다음이 `#` 가 아니면 topic 제목이다. `##` 뒤의 공백은 없어도 된다. 4칸 넘게 들여 써도 topic 제목이다. `// uncoded` 와 `// iknow …` 배지는 이 topic 제목에만 붙는다. `###` 이하 제목의 같은 글자는 그대로 글자다.
- 앞에 공백이나 탭이 있는 `keyword`, `exception`, `cover` 줄도 선언으로 그린다.
- 이미지는 주소와 상관없이 escape 한 alt 글자로 그린다. 상대 주소는 문서가 아니라 플러그인 화면을 기준으로 풀리기 때문이다.
- import 칩은 `target` 과 `address` 가 같은 `scope` 항목으로 찾는다. 칩의 문서 경로는 그 항목의 `doc` 이다. 찾지 못하면 `target` 을 보여 준다.
- 정의가 `definitions` 에 없는 참조에는 tooltip 을 달지 않는다.

---

### Task 1: `kang build` 가 그래프 인덱스를 쓴다

**Files:**
- Create: `crates/kang/src/graph.rs`
- Modify: `crates/kang/src/lib.rs` (모듈 등록), `crates/kang/src/main.rs` (`compile` 이 루트도 돌려주고, `build` 분기가 쓴다), `crates/kang/src/hash.rs` (`doc_hash` 추가), `.gitignore` 는 바꾸지 않는다
- Test: `crates/kang/tests/graph.rs`, `crates/kang/tests/cli.rs`

**Interfaces:**
- Produces: `pub fn doc_hash(text: &str) -> String` (hash.rs, 64 hex), `pub fn write_graph(project: &Project, table: &SymbolTable, sources: &HashMap<DocPath, String>, out: &mut impl Write) -> io::Result<()>`, `pub fn is_graph_format(content: &str) -> bool` (graph.rs), `pub fn kind(&self, id: SymbolId) -> SymbolKind`, `pub fn name(&self, id: SymbolId) -> &str` (resolve.rs `SymbolTable`, `scope` 의 주소를 만드는 데 쓴다)

- [x] **Step 1: 실패하는 테스트를 쓴다.** `tests/graph.rs` 에 세 테스트를 둔다. `writes_sorted_docs_keywords_edges` 는 문서 두 개(B 가 A 의 keyword 를 import)의 출력 전체를 기대 문자열과 비교한다. `escapes_quotes_backslash_and_control` 은 description 의 `"`, `\`, 탭이 `\"`, `\\`, `\u0009` 로 나오는지 본다. `doc_hash_matches_normalize_sha256` 은 `"a  \r\n\n\n b\n"` 의 결과가 `normalize` 뒤 SHA-256 hex 와 같고 길이가 64 인지 본다.
  - 확인: `tests/graph.rs` 에 세 테스트가 있다 (커밋 352dc6f). BOM 테스트는 23f5f61 에서 더했다.
- [x] **Step 2:** `cargo test -p kang --test graph` 가 컴파일 실패로 끝나는지 확인한다.
  - 확인: 첫 실행은 `graph` 모듈과 `doc_hash` 가 없어서 컴파일 실패로 끝났다.
- [x] **Step 3:** `doc_hash`, `write_graph`, `is_graph_format` 을 구현한다. `sources` 는 문서 경로에서 파일 원문으로 가는 맵이다.
  - 확인: 구현했다 (352dc6f). 23f5f61 에서 `doc_hash` 가 맨 앞 BOM 을 떼고, 문서마다 `scope` 를 쓴다.
- [x] **Step 4:** `cli.rs` 에 `build_writes_graph_index` (성공하면 `.kang/graph.kang.json` 이 생기고 첫 줄이 `{"kang":"graph/1",`), `build_does_not_write_graph_on_error`, `build_refuses_to_overwrite_foreign_graph_file` 를 더한다.
  - 확인: 세 CLI 테스트가 통과한다. 하위 디렉토리에서 실행해도 루트의 `.kang/graph.kang.json` 에 쓴다.
- [x] **Step 5:** `build` 분기를 구현한다. `compile()` 이 `(PathBuf, Project, SymbolTable)` 을 돌려주게 바꾸고, 다른 호출처는 루트를 버린다. 원문은 `root.join(path + ".kang")` 에서 다시 읽는다. 쓰기는 `bless::write_atomically` 와 덮어쓰기 방어를 쓰는 `generate` 의 쓰기 부분을 함수로 나눠 함께 쓴다.
  - 확인: `run_build` 와 `generate` 가 같은 쓰기 함수를 쓴다.
- [x] **Step 6:** `cargo test -p kang` 와 `cargo clippy -p kang -- -D warnings` 가 통과하는지 확인한다.
  - 확인: 2026-11-09 `cargo test` 382개 통과, `cargo clippy --workspace --all-targets -- -D warnings` 통과, `make index-check` 통과 (final-fix-report).
- [x] **Step 7:** 저장소에서 `cargo run -q -p kang -- build` 를 실행하고 `.kang/graph.kang.json` 을 커밋한다.
  - 확인: `kang build` 를 다시 실행하면 커밋된 `.kang/graph.kang.json` 과 바이트가 같다 (/review 1회차가 확인).

### Task 2: 플러그인 골격과 worker

**Files:**
- Create: `orca-plugin/orca-plugin.json`, `orca-plugin/tsconfig.json`, `orca-plugin/README.md`, `orca-plugin/src/protocol.ts`, `orca-plugin/src/worker.ts`, `orca-plugin/src/locate.ts`, `orca-plugin/src/locate.test.ts`
- Test: `orca-plugin/src/locate.test.ts`, `orca-plugin/src/hash-parity.test.ts`

**Interfaces:**
- Consumes: Task 1 의 그래프 인덱스 형식
- Produces: `normalize(text: string): string`, `docHash(text: string): string`, `pickWorkspace(candidates: Candidate[], focused: { branch: string; displayName: string } | null): Candidate | 'ambiguous'`, `locate(text, workspaces, focused, fs): Locate` (locate.ts). `fs` 는 `{ read(root: string, path: string): string | null }` 이고 테스트에서 가짜로 바꾼다. `root` 는 workspace 루트, `path` 는 그 안의 상대 경로다.

- [x] **Step 1:** 실패하는 테스트를 쓴다. `normalize` 는 줄 끝 공백 제거, 연속 빈 줄 합치기, 앞뒤 공백 제거를 하고, U+FEFF 는 공백으로 보지 않는다(Rust `char::is_whitespace` 와 같음). `hash-parity.test.ts` 는 Task 1 이 쓴 저장소의 `.kang/graph.kang.json` 에 있는 모든 문서의 hash 가 그 파일 원문의 `docHash` 와 같은지 본다. `locate` 테스트는 다섯 경우다. 하나만 맞음, 이름과 branch 로 고름, 같은 git 저장소의 worktree 둘 (정의를 주고 첫 번째를 고름), 다른 저장소 둘 (`ambiguous`), 원격만 남음 (`remoteOnly`). 그 밖에 `stale` 계산과 정의 제외를 하나씩 본다.
  - 확인: CRLF, BOM, 인덱스 없음, 깨진 JSON, 깨진 항목, 같은 문서 둘, 원격 focus 경우까지 테스트가 있다.
- [x] **Step 2:** `orca plugin test orca-plugin --json` 이 실패하는지 확인한다.
  - 확인: 구현 전 실행이 실패했다 (task-2-report).
- [x] **Step 3:** `locate.ts` 와 `worker.ts` 를 구현한다. worker 는 `workspace.list`, `workspace.readContext`, `editor.open` 을 부르고, `node:fs`, `node:crypto`, `node:path` 를 쓴다. 같은 git 저장소인지는 `.git` 이 디렉토리이면 그 경로, 파일이면 `gitdir:` 값의 `/worktrees/<이름>` 앞부분으로 비교한다.
  - 확인: 구현했다 (ef1894a). Orca 는 로컬 workspace 의 `hostId` 를 `"local"` 로 준다. 그래서 로컬 판단은 `null` 또는 `'local'` 이다 (1d82868).
- [x] **Step 4:** `orca plugin test orca-plugin --json` 와 `npx tsc -p orca-plugin` 가 통과하는지 확인한다.
  - 확인: `orca plugin test` 38개 통과, `tsc --noEmit` 오류 0 (9e0b443).

### Task 3: 렌더링 화면

**Files:**
- Create: `orca-plugin/src/render/parse.ts`, `orca-plugin/src/render/parse.test.ts`, `orca-plugin/src/render/main.ts`, `orca-plugin/src/render/style.css`, `orca-plugin/vendor/marked.esm.js`
- Modify: `orca-plugin/orca-plugin.json` (`.kang` custom editor)

**Interfaces:**
- Consumes: `kang.locate`, `kang.open` (Task 2), `parseKang`, `renderBody`, `lookupRef`

- [x] **Step 1:** 실패하는 테스트를 쓴다. `parseKang` 이 저장소의 `docs/adr/0002-flatten-on-read-import-on-write.kang` 원문에서 description, import 3개(`CONTEXT.topic`, `CONTEXT.owner`, `CONTEXT.문서`), 본문 링크를 만드는지 본다. `CONTEXT.kang` 에서 keyword 블록과 줄 끝 `#`무엇을 keyword 로 선언하나`` 링크를 만드는지 본다. 코드 펜스 안의 백틱은 링크가 되지 않는다. `\`` 는 백틱 문자로 남는다. `// uncoded`, `// iknow …`, `exception`, `cover`, `pending` 의 원본 기호가 출력에 남지 않는다. `lookupRef` 는 참조 문자열을 쓴 그대로 `scope` 의 `local` 과 비교해 찾는다. `scope` 에 없는 참조는 찾지 못한다.
  - 확인: 파서 테스트와 `lookupRef` 테스트가 통과한다. 백틱 펜스 안에는 링크가 생기지 않는다. 들여 쓴 줄, `~~~` 줄, 목록과 인용 안의 백틱 줄, HTML 블록처럼 보이는 줄은 컴파일러처럼 코드가 아니므로 링크가 생긴다.
- [x] **Step 2:** 테스트가 실패하는지 확인한다.
  - 확인: 구현 전 실행이 실패했다 (task-3-report).
- [x] **Step 3:** `parse.ts` 와 `main.ts` 를 구현한다. `main.ts` 는 `orca.document.onDidChange` 로 원문을 받고, `kang.locate` 를 부르고, `marked` 로 렌더링한다. 마우스를 올리면 정의를 보여 주고, 클릭하면 `kang.open` 을 부른다. 상태가 `ok` 가 아니면 안내 문구를 보여 준다. 테마는 `--orca-*` 변수를 쓴다.
  - 확인: 구현했다 (77bd423, 1d82868, 9e0b443). kang 문법은 marked 확장으로 처리한다.
- [x] **Step 4:** `orca plugin test`, `orca plugin check`, `npx tsc -p orca-plugin` 가 통과하는지 확인한다.
  - 확인: `orca plugin test`, `orca plugin check`, `orca plugin validate`, `tsc` 가 통과한다. check 경고는 vendor cytoscape 의 `editor_eval` 하나다.

### Task 4: 그래프 화면

**Files:**
- Create: `orca-plugin/src/graph/main.ts`, `orca-plugin/src/graph/model.ts`, `orca-plugin/src/graph/model.test.ts`, `orca-plugin/vendor/cytoscape.esm.min.mjs`
- Modify: `orca-plugin/orca-plugin.json` (`.kang.json` custom editor)

**Interfaces:**
- Consumes: `kang.locate`, `kang.open`
- Produces: `toElements(docs: GraphDoc[], edges: GraphEdge[]): ElementDefinition[]` (model.ts). 점 크기는 들어오는 연결 수로 정한다.

- [x] **Step 1:** 실패하는 테스트를 쓴다. `toElements` 가 문서마다 점 하나, 연결마다 화살표 하나를 만들고, 점의 `weight` 가 들어오는 연결 수인지 본다.
  - 확인: `model.test.ts` 가 통과한다.
- [x] **Step 2:** 테스트가 실패하는지 확인한다.
  - 확인: 구현 전 실행이 실패했다 (task-4-report).
- [x] **Step 3:** `model.ts` 와 `main.ts` 를 구현한다. `cose` 레이아웃을 쓴다. 점을 클릭하면 `kang.open`, 화살표에 마우스를 올리면 심볼 목록, `stale` 이 비어 있지 않으면 "인덱스가 오래되었습니다" 안내를 보여 준다.
  - 확인: 구현했다 (21c1e75). 실제 Orca 에서 빈 화면이 나와서 cose 애니메이션을 껐다 (ab63a44).
- [x] **Step 4:** `orca plugin test`, `orca plugin check`, `npx tsc -p orca-plugin` 가 통과하는지 확인한다.
  - 확인: 네 검사가 통과한다.

### Task 5: Orca 에 설치하고 화면으로 확인한다

- [x] **Step 1:** `orca plugin install ./orca-plugin --json` 을 실행한다. `approval_required` 가 나오면 Orca 가 준 fingerprint 로 다시 실행한다.
  - 확인: `orca plugin install` 대신 `orca plugin dev load ./orca-plugin --approve <fingerprint>` 로 등록했다. 이 worktree 를 고치면 Orca 가 바로 다시 읽는다. 플러그인 key 는 `local.kang`, 상태는 running 이다. Orca 의 kang 저장소 설정에서 "Orca 밖에서 만든 worktree 보이기" 를 켰다.
- [x] **Step 2:** `orca file open docs/adr/0002-flatten-on-read-import-on-write.kang --focus` 를 실행하고 화면을 캡처한다. 칩 3개, 용어 링크, 마크다운 제목이 보여야 한다.
  - 확인: `orca plugin shot local.kang --editor …0002….kang` 캡처에서 description 인용 블록, 칩 3개(topic, owner, 문서), 점선 밑줄 링크, 마크다운 제목과 목록이 보인다. ADR-0003 은 칩 5개가 보인다.
- [x] **Step 3:** 링크에 마우스를 올려 정의가 보이는지, 클릭해서 `CONTEXT.kang` 이 열리는지 화면으로 확인한다.
  - 확인: `kang.locate` 가 `ok`, 정의 19개, 문서별 scope 를 돌려준다. `kang.open` 이 `CONTEXT.kang` 을 새 탭으로 열고, 창 전체 캡처에서 그 탭이 용어 카드로 렌더링된 것을 보았다. 실제 마우스 hover 와 클릭은 macOS 접근성 권한이 막혀 넣지 못했다.
- [x] **Step 4:** `.kang/graph.kang.json` 을 열고 그래프 화면을 캡처한다. `CONTEXT` 로 화살표 3개가 들어와야 한다. 점을 클릭해 문서가 열리는지 확인한다.
  - 확인: 그래프 캡처에서 점 6개, `CONTEXT` 로 들어가는 화살표 3개가 보이고 `CONTEXT` 점이 가장 크다. 점 클릭이 부르는 `kang.open` 은 Step 3 에서 확인했다.
- [x] **Step 5:** 문서 하나를 고친 뒤 `kang build` 전에 그래프 화면을 다시 열어 "인덱스가 오래되었습니다" 안내를 확인하고, 고친 내용을 되돌린다.
  - 확인: `examples/ts-consumer/docs/proration.kang` 에 한 줄을 더한 뒤 그래프 화면 위에 "인덱스가 오래되었습니다. `kang build` 를 실행하세요." 가 보였다. 같은 문서의 렌더링 화면은 링크를 빼고 "`kang build` 를 실행하세요." 를 보였다. 파일은 원래대로 되돌렸다.

## Review Focus

- 원문에 CRLF 줄바꿈이나 BOM 이 있는 문서. 사용자는 정의가 정상으로 보이기를 기대한다. Task 2 의 `normalize` 테스트에 CRLF 와 BOM 을 넣는다.
- 그래프 인덱스가 없는 workspace. 사용자는 오류 없이 "`kang build` 를 실행하세요" 안내를 기대한다. Task 2 의 `locate` 테스트에 넣는다.
- 손상된 JSON 이 든 그래프 인덱스. worker 는 예외로 멈추지 않고 그 workspace 를 건너뛰어야 한다. Task 2 의 `locate` 테스트에 넣는다.
- 이름에 공백이나 한글이 든 keyword 와 topic. 링크와 정의가 모두 동작해야 한다. Task 3 의 테스트가 저장소 문서로 확인한다.
- 문서가 하나도 없는 프로젝트. `kang build` 는 빈 목록 세 개를 쓴다. Task 1 의 테스트에 넣는다.
