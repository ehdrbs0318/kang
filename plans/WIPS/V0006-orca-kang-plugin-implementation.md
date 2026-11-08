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
- 플러그인 id 는 `kang`, 폴더는 `orca-plugin/`, `engines.orca` 는 `>=1.4.248` 이다.
- 플러그인은 네트워크를 쓰지 않는다. `marked` 와 `cytoscape` 의 ESM 파일을 `orca-plugin/vendor/` 에 넣는다.
- 렌더링 화면은 읽기 전용이다. 플러그인은 `.kang` 파일을 바꾸지 않는다.
- 문서 해시는 `crates/kang/src/hash.rs` 의 `normalize` 와 같은 규칙으로 정규화한 텍스트의 SHA-256 전체 hex 값이다. TypeScript 쪽 계산은 worker 에만 있다.
- 원격 workspace (`hostId` 가 null 이 아닌 것)는 지원하지 않는다.
- 문서와 주석은 `/pstack:technical-writing` 규칙과 한국어 쉬운 문체를 따른다. 주석은 한국어다.

## Interfaces

이 절은 모든 Task 가 쓰는 계약이다.

### 그래프 인덱스 `.kang/graph.kang.json`

```json
{"kang":"graph/1",
"docs":[
{"path":"CONTEXT","description":"…","hash":"<64 hex>"}
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
- 문자열 escape 는 `"`, `\`, U+0000~U+001F 만 한다. 제어 문자는 `\u00XX` 로 쓴다.

### worker 요청 (`orca-plugin/src/protocol.ts`)

```ts
type Locate =
	| { status: 'ok'; kind: 'doc' | 'graph'; workspaceId: string; path: string | null; docs: GraphDoc[]; edges: GraphEdge[]; definitions: Record<string, string>; stale: string[] }
	| { status: 'notIndexed' | 'ambiguous' | 'remoteOnly' }
'kang/locate': { input: { text: string }; output: Locate }
'kang/open': { input: { workspaceId: string; path: string }; output: { opened: boolean } }
```

- `kind` 는 받은 원문이 그래프 인덱스 파일과 같으면 `'graph'`, 문서 해시가 맞으면 `'doc'` 이다.
- `path` 는 `kind: 'doc'` 일 때 그 문서의 경로다. `kind: 'graph'` 이면 null 이다.
- `definitions` 의 key 는 keyword 주소 (`CONTEXT.문서`)이고 값은 정의다. 선언한 문서의 지금 해시가 인덱스와 다른 keyword 는 뺀다.
- `stale` 은 지금 해시가 인덱스와 다른 문서 경로들이다.
- `kang/open` 은 `path` 가 그 workspace 의 인덱스에 있을 때만 `<path>.kang` 을 연다. 없으면 `{ opened: false }` 다.

### 렌더링 파서 (`orca-plugin/src/render/parse.ts`)

```ts
type Ref = { text: string; local: string }        // text 는 화면 문구, local 은 문서 안의 참조 문자열
type Block =
	| { kind: 'markdown'; text: string }             // 참조는 <a class="kang-ref" data-ref="…">…</a> 로 바뀐 상태
	| { kind: 'keyword'; name: string; definition: string; refs: string }   // refs 는 줄 끝 #topic 링크 HTML 또는 ''
export type KangView = { description: string; imports: { target: string; doc: string; local: string }[]; keywords: string[]; blocks: Block[] }
export function parseKang(text: string): KangView
export function resolveRef(view: KangView, selfPath: string, local: string): { address: string; doc: string } | null
```

---

### Task 1: `kang build` 가 그래프 인덱스를 쓴다

**Files:**
- Create: `crates/kang/src/graph.rs`
- Modify: `crates/kang/src/lib.rs` (모듈 등록), `crates/kang/src/main.rs` (`compile` 이 루트도 돌려주고, `build` 분기가 쓴다), `crates/kang/src/hash.rs` (`doc_hash` 추가), `.gitignore` 는 바꾸지 않는다
- Test: `crates/kang/tests/graph.rs`, `crates/kang/tests/cli.rs`

**Interfaces:**
- Produces: `pub fn doc_hash(text: &str) -> String` (hash.rs, 64 hex), `pub fn write_graph(project: &Project, table: &SymbolTable, sources: &HashMap<DocPath, String>, out: &mut impl Write) -> io::Result<()>`, `pub fn is_graph_format(content: &str) -> bool` (graph.rs)

- [ ] **Step 1: 실패하는 테스트를 쓴다.** `tests/graph.rs` 에 세 테스트를 둔다. `writes_sorted_docs_keywords_edges` 는 문서 두 개(B 가 A 의 keyword 를 import)의 출력 전체를 기대 문자열과 비교한다. `escapes_quotes_backslash_and_control` 은 description 의 `"`, `\`, 탭이 `\"`, `\\`, `\u0009` 로 나오는지 본다. `doc_hash_matches_normalize_sha256` 은 `"a  \r\n\n\n b\n"` 의 결과가 `normalize` 뒤 SHA-256 hex 와 같고 길이가 64 인지 본다.
- [ ] **Step 2:** `cargo test -p kang --test graph` 가 컴파일 실패로 끝나는지 확인한다.
- [ ] **Step 3:** `doc_hash`, `write_graph`, `is_graph_format` 을 구현한다. `sources` 는 문서 경로에서 파일 원문으로 가는 맵이다.
- [ ] **Step 4:** `cli.rs` 에 `build_writes_graph_index` (성공하면 `.kang/graph.kang.json` 이 생기고 첫 줄이 `{"kang":"graph/1",`), `build_does_not_write_graph_on_error`, `build_refuses_to_overwrite_foreign_graph_file` 를 더한다.
- [ ] **Step 5:** `build` 분기를 구현한다. `compile()` 이 `(PathBuf, Project, SymbolTable)` 을 돌려주게 바꾸고, 다른 호출처는 루트를 버린다. 원문은 `root.join(path + ".kang")` 에서 다시 읽는다. 쓰기는 `bless::write_atomically` 와 덮어쓰기 방어를 쓰는 `generate` 의 쓰기 부분을 함수로 나눠 함께 쓴다.
- [ ] **Step 6:** `cargo test -p kang` 와 `cargo clippy -p kang -- -D warnings` 가 통과하는지 확인한다.
- [ ] **Step 7:** 저장소에서 `cargo run -q -p kang -- build` 를 실행하고 `.kang/graph.kang.json` 을 커밋한다.

### Task 2: 플러그인 골격과 worker

**Files:**
- Create: `orca-plugin/orca-plugin.json`, `orca-plugin/tsconfig.json`, `orca-plugin/README.md`, `orca-plugin/src/protocol.ts`, `orca-plugin/src/worker.ts`, `orca-plugin/src/locate.ts`, `orca-plugin/src/locate.test.ts`
- Test: `orca-plugin/src/locate.test.ts`, `orca-plugin/src/hash-parity.test.ts`

**Interfaces:**
- Consumes: Task 1 의 그래프 인덱스 형식
- Produces: `normalize(text: string): string`, `docHash(text: string): string`, `pickWorkspace(candidates: Candidate[], focused: { branch: string; displayName: string } | null): Candidate | 'ambiguous'`, `locate(text, workspaces, focused, fs): Locate` (locate.ts). `fs` 는 `{ read(path: string): string | null }` 이고 테스트에서 가짜로 바꾼다.

- [ ] **Step 1:** 실패하는 테스트를 쓴다. `normalize` 는 줄 끝 공백 제거, 연속 빈 줄 합치기, 앞뒤 공백 제거를 하고, U+FEFF 는 공백으로 보지 않는다(Rust `char::is_whitespace` 와 같음). `hash-parity.test.ts` 는 Task 1 이 쓴 저장소의 `.kang/graph.kang.json` 에 있는 모든 문서의 hash 가 그 파일 원문의 `docHash` 와 같은지 본다. `locate` 테스트는 다섯 경우다. 하나만 맞음, 이름과 branch 로 고름, 같은 git 저장소의 worktree 둘 (정의를 주고 첫 번째를 고름), 다른 저장소 둘 (`ambiguous`), 원격만 남음 (`remoteOnly`). 그 밖에 `stale` 계산과 정의 제외를 하나씩 본다.
- [ ] **Step 2:** `orca plugin test orca-plugin --json` 이 실패하는지 확인한다.
- [ ] **Step 3:** `locate.ts` 와 `worker.ts` 를 구현한다. worker 는 `workspace.list`, `workspace.readContext`, `editor.open` 을 부르고, `node:fs`, `node:crypto`, `node:path` 를 쓴다. 같은 git 저장소인지는 `.git` 이 디렉토리이면 그 경로, 파일이면 `gitdir:` 값의 `/worktrees/<이름>` 앞부분으로 비교한다.
- [ ] **Step 4:** `orca plugin test orca-plugin --json` 와 `npx tsc -p orca-plugin` 가 통과하는지 확인한다.

### Task 3: 렌더링 화면

**Files:**
- Create: `orca-plugin/src/render/parse.ts`, `orca-plugin/src/render/parse.test.ts`, `orca-plugin/src/render/main.ts`, `orca-plugin/src/render/style.css`, `orca-plugin/vendor/marked.esm.js`
- Modify: `orca-plugin/orca-plugin.json` (`.kang` custom editor)

**Interfaces:**
- Consumes: `kang/locate`, `kang/open` (Task 2), `parseKang`, `resolveRef`

- [ ] **Step 1:** 실패하는 테스트를 쓴다. `parseKang` 이 저장소의 `docs/adr/0002-flatten-on-read-import-on-write.kang` 원문에서 description, import 3개(`CONTEXT.topic`, `CONTEXT.owner`, `CONTEXT.문서`), 본문 링크를 만드는지 본다. `CONTEXT.kang` 에서 keyword 블록과 줄 끝 `#`무엇을 keyword 로 선언하나`` 링크를 만드는지 본다. 코드 펜스 안의 백틱은 링크가 되지 않는다. `\`` 는 백틱 문자로 남는다. `// uncoded`, `// iknow …`, `exception`, `cover`, `pending` 의 원본 기호가 출력에 남지 않는다. `resolveRef` 는 alias, import 의 마지막 이름, 자기 keyword, 자기 topic 을 각각 주소로 바꾼다.
- [ ] **Step 2:** 테스트가 실패하는지 확인한다.
- [ ] **Step 3:** `parse.ts` 와 `main.ts` 를 구현한다. `main.ts` 는 `orca.document.onDidChange` 로 원문을 받고, `kang/locate` 를 부르고, `marked` 로 렌더링한다. 마우스를 올리면 정의를 보여 주고, 클릭하면 `kang/open` 을 부른다. 상태가 `ok` 가 아니면 안내 문구를 보여 준다. 테마는 `--orca-*` 변수를 쓴다.
- [ ] **Step 4:** `orca plugin test`, `orca plugin check`, `npx tsc -p orca-plugin` 가 통과하는지 확인한다.

### Task 4: 그래프 화면

**Files:**
- Create: `orca-plugin/src/graph/main.ts`, `orca-plugin/src/graph/model.ts`, `orca-plugin/src/graph/model.test.ts`, `orca-plugin/vendor/cytoscape.esm.min.mjs`
- Modify: `orca-plugin/orca-plugin.json` (`.kang.json` custom editor)

**Interfaces:**
- Consumes: `kang/locate`, `kang/open`
- Produces: `toElements(docs: GraphDoc[], edges: GraphEdge[]): ElementDefinition[]` (model.ts). 점 크기는 들어오는 연결 수로 정한다.

- [ ] **Step 1:** 실패하는 테스트를 쓴다. `toElements` 가 문서마다 점 하나, 연결마다 화살표 하나를 만들고, 점의 `weight` 가 들어오는 연결 수인지 본다.
- [ ] **Step 2:** 테스트가 실패하는지 확인한다.
- [ ] **Step 3:** `model.ts` 와 `main.ts` 를 구현한다. `cose` 레이아웃을 쓴다. 점을 클릭하면 `kang/open`, 화살표에 마우스를 올리면 심볼 목록, `stale` 이 비어 있지 않으면 "인덱스가 오래되었습니다" 안내를 보여 준다.
- [ ] **Step 4:** `orca plugin test`, `orca plugin check`, `npx tsc -p orca-plugin` 가 통과하는지 확인한다.

### Task 5: Orca 에 설치하고 화면으로 확인한다

- [ ] **Step 1:** `orca plugin install ./orca-plugin --json` 을 실행한다. `approval_required` 가 나오면 Orca 가 준 fingerprint 로 다시 실행한다.
- [ ] **Step 2:** `orca file open docs/adr/0002-flatten-on-read-import-on-write.kang --focus` 를 실행하고 화면을 캡처한다. 칩 3개, 용어 링크, 마크다운 제목이 보여야 한다.
- [ ] **Step 3:** 링크에 마우스를 올려 정의가 보이는지, 클릭해서 `CONTEXT.kang` 이 열리는지 화면으로 확인한다.
- [ ] **Step 4:** `.kang/graph.kang.json` 을 열고 그래프 화면을 캡처한다. `CONTEXT` 로 화살표 3개가 들어와야 한다. 점을 클릭해 문서가 열리는지 확인한다.
- [ ] **Step 5:** 문서 하나를 고친 뒤 `kang build` 전에 그래프 화면을 다시 열어 "인덱스가 오래되었습니다" 안내를 확인하고, 고친 내용을 되돌린다.

## Review Focus

- 원문에 CRLF 줄바꿈이나 BOM 이 있는 문서. 사용자는 정의가 정상으로 보이기를 기대한다. Task 2 의 `normalize` 테스트에 CRLF 와 BOM 을 넣는다.
- 그래프 인덱스가 없는 workspace. 사용자는 오류 없이 "`kang build` 를 실행하세요" 안내를 기대한다. Task 2 의 `locate` 테스트에 넣는다.
- 손상된 JSON 이 든 그래프 인덱스. worker 는 예외로 멈추지 않고 그 workspace 를 건너뛰어야 한다. Task 2 의 `locate` 테스트에 넣는다.
- 이름에 공백이나 한글이 든 keyword 와 topic. 링크와 정의가 모두 동작해야 한다. Task 3 의 테스트가 저장소 문서로 확인한다.
- 문서가 하나도 없는 프로젝트. `kang build` 는 빈 목록 세 개를 쓴다. Task 1 의 테스트에 넣는다.
