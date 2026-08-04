# V0002 — kang v1 구현 플랜

> **에이전트 실행용:** 이 플랜은 `superpowers:subagent-driven-development` 또는 `superpowers:executing-plans` 로 태스크 단위 실행한다. 각 단계는 체크박스로 추적한다.

**목표:** `V0001-kang-language-design.md` 의 v1 명세대로 kang 컴파일러와 CLI를 구현한다.

**아키텍처:** 단일 Rust 바이너리. 프로젝트 전체를 읽어 `Document` AST로 파싱하고, 심볼 테이블과 import 그래프를 만든 뒤, 진단 규칙을 돌린다. 진단에 error가 하나라도 있으면 어떤 조회 명령도 출력하지 않는다.

**기술 스택:** Rust 1.97 / 외부 의존성은 `sha2` 하나

## 전역 제약

- 스펙 원본은 `plans/DONES/V0001-kang-language-design.md` (완료 시 이동). 모든 규칙의 근거는 여기다.
- **의존성 추가 금지.** `sha2` 외에 크레이트를 넣지 않는다. 인자 파싱과 YAML 출력은 직접 쓴다 — v1에는 플래그가 하나도 없고 YAML 스키마가 고정이다.
- `kang build` 기본 심각도는 **error**. error 발생 시 종료 코드 1, 조회 명령은 아무것도 출력하지 않는다.
- 주석은 한글 TSDoc 대응 규격(rustdoc `///`)으로 작성한다. 함수·구조체·enum 전부.
- 로깅 규칙 적용 대상 아님 — CLI 단발 실행이라 로그 레벨 시스템을 두지 않는다. 진단 출력이 그 역할을 한다.
- 모든 진단 메시지는 **수정 위치와 방법**을 포함한다. LLM이 스스로 고칠 수 있어야 한다.
- 테스트는 `cargo test` 하나로 전부 돈다.

## 파일 구조

```
Cargo.toml
src/
  main.rs      CLI 디스패치, list/keywords/refs 출력
  ast.rs       AST 타입 정의
  parse.rs     렉서 + 파서 (파일 1개 → Document)
  hash.rs      정규화 + rev 해시
  resolve.rs   프로젝트 로드, 심볼 테이블, import 그래프
  check.rs     진단 규칙 + 진단 출력
  yaml.rs      YAML 이미터
  show.rs      show 출력 구성 (재귀 임베드 + 중복 제거)
  bless.rs     rev 핀 갱신
tests/
  parse.rs     파싱 단위 테스트
  check.rs     진단 규칙 테스트
  cli.rs       CLI 통합 테스트
```

책임 분리 기준: `parse`는 파일 하나만 안다. `resolve`는 프로젝트 전체를 안다. `check`는 규칙만 안다. `show`/`yaml`/`bless`는 출력과 수정만 한다.

---

## Task 1: 프로젝트 부트스트랩 + rev 해시

**파일**
- 생성: `Cargo.toml`, `src/main.rs`, `src/hash.rs`
- 테스트: `tests/parse.rs` (해시 테스트를 여기 둔다 — 파일 하나로 시작)

**인터페이스**
- 산출: `hash::normalize(&str) -> String`, `hash::rev(&str) -> String`

```rust
/// 해시 입력 텍스트를 정규화한다.
/// 앞뒤 공백 제거, 줄 끝 공백 제거, 연속 빈 줄 축약.
pub fn normalize(text: &str) -> String;

/// 정규화된 텍스트의 SHA-256 앞 6자리 hex 를 반환한다.
pub fn rev(text: &str) -> String;
```

- [ ] **Step 1: `cargo init --name kang` 실행, `Cargo.toml`에 `sha2` 추가**
- [ ] **Step 2: 실패하는 테스트 작성** — 시나리오 3개
  - `줄_끝_공백은_해시를_바꾸지_않는다`
  - `연속_빈_줄은_하나로_축약된다`
  - `본문이_다르면_해시가_다르다`
- [ ] **Step 3: `cargo test` — 컴파일 실패 확인**
- [ ] **Step 4: `normalize`, `rev` 구현**
  - 줄 단위로 `trim_end`, 빈 줄 2개 이상은 1개로, 전체 `trim`
  - `sha2::Sha256` 결과를 hex로 만들고 앞 6자
- [ ] **Step 5: `cargo test` 통과 확인**
- [ ] **Step 6: 커밋** — `feat: rev 해시 산출과 정규화 규칙`

---

## Task 2: 파서 — frontmatter, keyword, topic

**파일**
- 생성: `src/ast.rs`, `src/parse.rs`
- 수정: `src/main.rs` (모듈 선언)
- 테스트: `tests/parse.rs`

**인터페이스**
- 소비: 없음
- 산출: 아래 AST 타입 전부와 `parse::parse_document(path, source) -> Result<Document, Vec<Diagnostic>>`

```rust
// ast.rs

pub enum Severity { Error, Warn }

/// 진단 하나. `fix` 는 LLM 이 그대로 적용할 수 있는 수정 안내다.
/// 파서부터 진단 규칙까지 전 단계가 이 타입을 공유하므로 ast 에 둔다.
pub struct Diagnostic {
    pub severity: Severity,
    pub doc: DocPath,
    pub line: usize,
    pub message: String,
    pub fix: Option<String>,
}

/// 문서 경로. `docs/A` 는 ["docs", "A"] 이다.
/// HashMap 키로 쓰이므로 Hash + Eq 를 파생한다.
pub struct DocPath(pub Vec<String>);

/// 키워드 이름. 계층 키워드 `결제수단`.`카드` 는 ["결제수단", "카드"] 이다.
pub struct KeywordName(pub Vec<String>);

pub enum SymbolKind { Keyword, Topic, Exception }

/// 다른 문서의 심볼을 가리키는 참조.
pub struct SymbolRef {
    pub doc: DocPath,
    pub kind: SymbolKind,
    pub name: Vec<String>,
}

pub struct Keyword {
    pub name: KeywordName,
    pub definition: String,
    pub detail: Option<String>,   // #`상세 topic` 이름
    pub iknow: Vec<SymbolRef>,
    pub line: usize,
}

pub struct Topic {
    pub name: String,
    pub body: String,             // 헤딩 포함 원문
    pub uncoded: bool,
    pub iknow: Vec<SymbolRef>,
    pub refs: Vec<(String, usize)>,   // 본문 백틱 심볼과 등장 줄
    pub exceptions: Vec<Exception>,
    pub covers: Vec<(String, usize)>,
    pub line: usize,
}

pub struct Exception {
    pub name: String,
    pub pending: bool,
    pub iknow: Vec<SymbolRef>,
    pub line: usize,
}

pub struct Import {
    pub target: SymbolRef,
    pub alias: Option<String>,
    pub rev: Option<String>,
    pub line: usize,
}

pub struct Document {
    pub path: DocPath,
    pub description: String,
    pub imports: Vec<Import>,
    pub keywords: Vec<Keyword>,
    pub topics: Vec<Topic>,
}
```

```rust
// parse.rs

/// 소스 한 파일을 Document 로 파싱한다.
pub fn parse_document(path: DocPath, source: &str) -> Result<Document, Vec<Diagnostic>>;
```

이 태스크에서는 `imports`, `exceptions`, `covers` 를 빈 벡터로 두고 Task 3에서 채운다.

**구현 요점**
- frontmatter는 `---` 로 감싼 블록. `description` 이 없으면 error.
- `keyword` 줄: 이름(계층 `.` 구분) → `:` → 한 줄 정의 → 선택적 `#`상세 topic`.
- `##` 로 시작하면 새 topic. 다음 `##` 직전까지가 body.
- 백틱 스캔: `` \` `` 는 리터럴, ` ``` ` 펜스 내부는 건너뛴다. 그 외 백틱 쌍은 심볼 참조로 `refs` 에 기록.
- 줄 번호를 모든 노드에 기록한다. 진단 품질이 여기 달려 있다.

- [ ] **Step 1: 실패하는 테스트 작성** — 시나리오 7개
  - `frontmatter_description_을_읽는다`
  - `description_이_없으면_에러다`
  - `keyword_의_이름과_한줄정의를_읽는다`
  - `계층_키워드를_이름_배열로_읽는다`
  - `topic_헤딩과_본문을_잘라낸다`
  - `본문_백틱을_심볼_참조로_수집한다`
  - `이스케이프된_백틱과_코드펜스_안은_참조가_아니다`
- [ ] **Step 2: `cargo test` — 실패 확인**
- [ ] **Step 3: `ast.rs` 타입 정의 (`Diagnostic` 포함)**
- [ ] **Step 4: `parse.rs` 구현 — frontmatter, keyword, topic, 백틱 스캔**
- [ ] **Step 5: `cargo test` 통과 확인**
- [ ] **Step 6: 커밋** — `feat: frontmatter/keyword/topic 파싱`

---

## Task 3: 파서 — import, exception, cover, modifier

**파일**
- 수정: `src/parse.rs`
- 테스트: `tests/parse.rs`

**인터페이스**
- 소비: Task 2의 AST 타입 전부, `parse::parse_document`
- 산출: 같은 함수가 `imports` / `exceptions` / `covers` / `iknow` / `uncoded` 를 채운다

**구현 요점**
- import 문법: `` import `docs`/`A`.`결제` as `A 결제` rev "a3f9c1" ``
  - `/` 경로, `.` keyword, `#` topic, `!` exception. 마지막 구분자로 `kind` 를 결정한다.
  - `.` 는 keyword 진입 후 계층에도 쓰인다. 첫 `.` 이후는 전부 키워드 이름 조각이다.
  - `as`, `rev` 는 선택 토큰. 그룹 문법은 없다 — 한 줄에 하나.
- `exception `이름` [pending]` 과 `cover `이름`` 은 topic 본문 안에서 인식한다.
- `// iknow <심볼 참조 목록>` 은 keyword / topic 헤딩 / exception 줄 뒤에 붙는다.
- `// uncoded` 는 topic 헤딩 줄 뒤에만 붙는다.
- 이 modifier들은 topic `body` 에서 제외한다 — rev 해시에 들어가면 안 된다.

- [ ] **Step 1: 실패하는 테스트 작성** — 시나리오 9개
  - `keyword_import_를_읽는다`
  - `topic_import_를_읽는다`
  - `exception_import_를_읽는다`
  - `as_alias_를_읽는다`
  - `rev_핀을_읽는다`
  - `exception_과_pending_을_읽는다`
  - `cover_를_읽는다`
  - `iknow_대상_목록을_읽는다`
  - `uncoded_modifier_는_body_에서_제외된다`
- [ ] **Step 2: `cargo test` — 실패 확인**
- [ ] **Step 3: import 파싱 구현 (경로 구분자 → kind 판정)**
- [ ] **Step 4: exception / cover / modifier 파싱 구현**
- [ ] **Step 5: `cargo test` 통과 확인**
- [ ] **Step 6: 커밋** — `feat: import/exception/cover/modifier 파싱`

---

## Task 4: 프로젝트 로드 + 심볼 테이블

**파일**
- 생성: `src/resolve.rs`
- 수정: `src/main.rs`
- 테스트: `tests/check.rs`

**인터페이스**
- 소비: `ast::*`, `parse::parse_document`
- 산출:

```rust
/// 파싱된 문서 전체를 담는다.
pub struct Project {
    pub docs: HashMap<DocPath, Document>,
}

/// 루트 디렉토리를 재귀 순회하며 .kang 파일을 전부 읽어 파싱한다.
pub fn load(root: &Path) -> (Project, Vec<Diagnostic>);

/// 심볼 하나를 가리키는 프로젝트 전역 식별자.
pub struct SymbolId(usize);

pub struct SymbolTable { /* 비공개 */ }

impl SymbolTable {
    /// 전역 심볼 테이블을 만든다. 이름 충돌은 진단으로 보고한다.
    pub fn build(project: &Project) -> (SymbolTable, Vec<Diagnostic>);

    /// 심볼 참조를 전역 식별자로 해석한다.
    pub fn resolve(&self, r: &SymbolRef) -> Option<SymbolId>;

    /// 한 문서 안에서 쓸 수 있는 로컬 이름 → 심볼 매핑.
    /// 자기 파일이 선언한 심볼과 import 한 alias 를 합친 것이다.
    pub fn scope(&self, doc: &DocPath) -> HashMap<String, SymbolId>;

    /// 같은 이름으로 선언된 심볼들을 모아 반환한다. iknow 검사에 쓴다.
    pub fn by_name(&self, name: &str) -> Vec<SymbolId>;

    pub fn kind(&self, id: SymbolId) -> SymbolKind;
    pub fn owner(&self, id: SymbolId) -> &DocPath;
    pub fn hash_source(&self, id: SymbolId) -> Option<&str>;
}
```

**구현 요점**
- 디렉토리 순회는 `std::fs::read_dir` 재귀로 직접 쓴다. `.kang` 확장자만 읽는다.
- `hash_source` 는 keyword면 한 줄 정의, topic이면 body, exception이면 `None` 을 준다. Task 8이 이걸 쓴다.

- [ ] **Step 1: 실패하는 테스트 작성** — 시나리오 5개
  - `하위_디렉토리의_kang_파일을_전부_읽는다`
  - `kang_이_아닌_파일은_무시한다`
  - `자기_파일의_심볼을_스코프에서_찾는다`
  - `import_한_alias_를_스코프에서_찾는다`
  - `같은_이름_심볼을_by_name_으로_모은다`
- [ ] **Step 2: `cargo test` — 실패 확인**
- [ ] **Step 3: `load` 구현 (재귀 순회 + 파싱)**
- [ ] **Step 4: `SymbolTable` 구현**
- [ ] **Step 5: `cargo test` 통과 확인**
- [ ] **Step 6: 커밋** — `feat: 프로젝트 로드와 전역 심볼 테이블`

---

## Task 5: import 그래프 + 순환 검출 + 참조 전파

**파일**
- 수정: `src/resolve.rs`
- 테스트: `tests/check.rs`

**인터페이스**
- 소비: `Project`, `SymbolTable`
- 산출:

```rust
pub struct ImportGraph { /* 비공개 */ }

impl ImportGraph {
    /// import 관계로 DAG 를 만든다. 순환이 있으면 체인 전체를 진단에 담는다.
    pub fn build(project: &Project) -> (ImportGraph, Vec<Diagnostic>);

    /// 주어진 topic 이 직간접적으로 import 하는 모든 상위 topic.
    /// 참조 전파(스펙 5.3)에 쓴다.
    pub fn ancestors(&self, doc: &DocPath, topic: &str) -> Vec<(DocPath, String)>;
}
```

**구현 요점**
- 노드는 문서가 아니라 **topic** 이다. 참조 전파가 topic 단위이기 때문이다.
- 순환 검출은 DFS + 방문 색칠. 순환 발견 시 스택을 그대로 체인으로 출력하고 "공통 개념을 상위 파일로 추출하라"를 `fix` 에 담는다.

- [ ] **Step 1: 실패하는 테스트 작성** — 시나리오 4개
  - `직접_순환을_검출한다`
  - `3단계_순환의_체인_전체를_출력한다`
  - `ancestors_가_상위_topic_을_재귀적으로_모은다`
  - `다이아몬드_의존에서_ancestors_가_중복되지_않는다`
- [ ] **Step 2: `cargo test` — 실패 확인**
- [ ] **Step 3: 그래프 구성과 DFS 순환 검출 구현**
- [ ] **Step 4: `ancestors` 구현**
- [ ] **Step 5: `cargo test` 통과 확인**
- [ ] **Step 6: 커밋** — `feat: import 그래프와 순환 검출`

---

## Task 6: 진단 — 심볼 규칙

**파일**
- 생성: `src/check.rs`
- 테스트: `tests/check.rs`

**인터페이스**
- 소비: `Project`, `SymbolTable`, `ast::Diagnostic` (Task 2에서 정의)
- 산출:

```rust
/// 스펙 5.1 의 심볼 규칙을 검사한다.
pub fn check_symbols(project: &Project, table: &SymbolTable) -> Vec<Diagnostic>;

/// 진단 목록을 사람과 LLM 이 읽을 형태로 만든다.
pub fn report(diags: &[Diagnostic]) -> String;
```

**검사 항목** (스펙 5.1)

| 상황 | 심각도 |
|---|---|
| 본문 백틱 심볼이 스코프에 없음 | error |
| import 했으나 어떤 topic 에서도 미사용 | error |
| 한 심볼에 두 개 이상 alias | error |
| 서로 다른 파일이 같은 이름 심볼 선언, iknow 없음 | error |
| iknow 가 관련 파일 전체를 상호 명시하지 않음 | error |

- iknow 검사는 상호성까지 본다. N개 파일이 같은 이름을 선언하면 각 파일이 나머지 N-1개를 전부 명시해야 한다. 누락된 파일 경로를 `fix` 에 나열한다.
- 사용 여부는 topic 별로 추적한다. 파일 전체에서 한 번이라도 쓰였으면 통과.

- [ ] **Step 1: 실패하는 테스트 작성** — 시나리오 7개
  - `미선언_백틱_심볼은_에러다`
  - `사용하지_않는_import_는_에러다`
  - `한_심볼에_두_alias_는_에러다`
  - `이름_충돌에_iknow_가_없으면_에러다`
  - `iknow_가_한쪽에만_있으면_에러다`
  - `3개_파일_충돌은_각자_나머지_2개를_명시해야_한다`
  - `진단_메시지가_누락된_파일_경로를_포함한다`
- [ ] **Step 2: `cargo test` — 실패 확인**
- [ ] **Step 3: `report` 구현**
- [ ] **Step 4: `check_symbols` 의 5개 규칙 구현**
- [ ] **Step 5: `cargo test` 통과 확인**
- [ ] **Step 6: 커밋** — `feat: 심볼 진단 규칙`

---

## Task 7: 진단 — exception 상태 기계

**파일**
- 수정: `src/check.rs`
- 테스트: `tests/check.rs`

**인터페이스**
- 소비: `Project`, `SymbolTable`, `Diagnostic`
- 산출: `check::check_exceptions(project, table) -> Vec<Diagnostic>`

```rust
/// 스펙 5.2 의 exception 상태 기계를 검사한다.
pub fn check_exceptions(project: &Project, table: &SymbolTable) -> Vec<Diagnostic>;
```

**진리표** (스펙 5.2)

| exception 상태 | cover 있음 | cover 없음 |
|---|---|---|
| 일반 | 통과 | error |
| `pending` | error | warn |

추가로 **한 exception 을 둘 이상의 topic 이 cover 하면 error** 다.

- [ ] **Step 1: 실패하는 테스트 작성** — 시나리오 5개
  - `커버되지_않은_exception_은_에러다`
  - `커버된_exception_은_통과한다`
  - `pending_이고_커버가_없으면_warn_이다`
  - `pending_인데_커버가_있으면_에러다`
  - `한_exception_을_둘이_커버하면_에러다`
- [ ] **Step 2: `cargo test` — 실패 확인**
- [ ] **Step 3: `check_exceptions` 구현**
- [ ] **Step 4: `cargo test` 통과 확인**
- [ ] **Step 5: 커밋** — `feat: exception 상태 기계 검증`

---

## Task 8: 진단 — rev 핀

**파일**
- 수정: `src/check.rs`
- 테스트: `tests/check.rs`

**인터페이스**
- 소비: `Project`, `SymbolTable`, `hash::rev`
- 산출: `check::check_revs(project, table) -> Vec<Diagnostic>`

```rust
/// 스펙 4.7 의 rev 핀을 검사한다.
pub fn check_revs(project: &Project, table: &SymbolTable) -> Vec<Diagnostic>;
```

**검사 항목**

| 상황 | 심각도 |
|---|---|
| keyword·topic import 에 rev 핀 없음 | error |
| rev 핀이 대상의 현재 해시와 불일치 | error |
| exception import 에 rev 핀이 있음 | error |

- 불일치 진단의 `fix` 는 `kang bless <문서경로>:<줄번호>` 형태를 담는다. Task 11의 `bless` 가 그대로 받는 형식이어야 한다.
- **diff 를 출력하지 않는다.** kang 은 이전 본문을 저장하지 않는다. 무엇이 바뀌었는지는 `git diff` 가 보여준다는 안내만 넣는다.

- [ ] **Step 1: 실패하는 테스트 작성** — 시나리오 5개
  - `rev_핀이_없으면_에러다`
  - `rev_핀이_일치하면_통과한다`
  - `대상_본문이_바뀌면_에러다`
  - `exception_import_에_rev_가_있으면_에러다`
  - `진단_fix_가_bless_인자_형식이다`
- [ ] **Step 2: `cargo test` — 실패 확인**
- [ ] **Step 3: `check_revs` 구현**
- [ ] **Step 4: `cargo test` 통과 확인**
- [ ] **Step 5: 커밋** — `feat: rev 핀 검증`

---

## Task 9: CLI 골격 + build / list / keywords / refs

**파일**
- 수정: `src/main.rs`
- 테스트: `tests/cli.rs`

**인터페이스**
- 소비: `resolve::load`, `SymbolTable`, `ImportGraph`, `check::*`
- 산출: 실행 가능한 `kang` 바이너리

```rust
/// 서브커맨드를 파싱한다. v1 에는 플래그가 없고 위치 인자만 있다.
enum Command {
    Build,
    Bless(Vec<String>),
    List(Option<String>),
    Keywords(Option<String>),
    Refs(String),
    Show(String),
}

/// 프로젝트를 로드하고 모든 진단을 돌린다.
/// error 가 하나라도 있으면 Err 를 반환한다.
fn compile() -> Result<(Project, SymbolTable, ImportGraph), Vec<Diagnostic>>;
```

**출력 규칙** (스펙 6.1)
- 한 라인이 의미론적 완결성을 갖는다. 경로는 항상 전체 경로, 계층 축약 없음.
- `kang list [경로]` → `docs/A: {description}`
- `kang keywords [경로]` → `docs/A.결제: 사용자가 상품 대금을 지불하는 행위`
- `kang refs <키워드>` → `docs/A#결제의 방식`

**핵심 규칙**: 조회 명령은 전부 `compile()` 을 먼저 거친다. error가 있으면 진단만 출력하고 종료 코드 1로 끝낸다. 문서는 한 줄도 출력하지 않는다.

`ponytail:` 인자 파싱을 직접 쓴다. v1에는 플래그가 없어 `match` 로 충분하다. 플래그가 생기면 clap 으로 교체.

- [ ] **Step 1: 실패하는 테스트 작성** — 시나리오 6개
  - `build_는_정상_프로젝트에서_종료코드_0_이다`
  - `build_는_에러가_있으면_종료코드_1_이다`
  - `에러가_있으면_list_가_아무것도_출력하지_않는다`
  - `list_가_전체_경로로_출력한다`
  - `keywords_가_경로_스코프로_필터된다`
  - `refs_가_키워드를_참조하는_topic_을_출력한다`
- [ ] **Step 2: `cargo test` — 실패 확인**
- [ ] **Step 3: 서브커맨드 디스패치와 `compile()` 구현**
- [ ] **Step 4: `build` / `list` / `keywords` / `refs` 출력 구현**
- [ ] **Step 5: `cargo test` 통과 확인**
- [ ] **Step 6: 커밋** — `feat: CLI 골격과 build/list/keywords/refs`

---

## Task 10: YAML 이미터 + `kang show`

**파일**
- 생성: `src/yaml.rs`, `src/show.rs`
- 수정: `src/main.rs`
- 테스트: `tests/cli.rs`

**인터페이스**
- 소비: `Project`, `SymbolTable`, `ImportGraph`
- 산출:

```rust
// yaml.rs

/// YAML 문서를 조립한다. 스키마가 고정이라 직접 쓴다.
pub struct Emitter { /* 비공개 */ }

impl Emitter {
    pub fn new() -> Self;
    pub fn pair(&mut self, key: &str, value: &str);
    /// literal scalar(`|`) 로 멀티라인 본문을 넣는다.
    /// folded scalar(`>`) 는 개행이 접혀 마크다운이 깨지므로 쓰지 않는다.
    pub fn block(&mut self, key: &str, body: &str);
    pub fn key(&mut self, key: &str);
    pub fn item(&mut self);
    pub fn indent(&mut self);
    pub fn dedent(&mut self);
    pub fn finish(self) -> String;
}

/// 스칼라를 안전하게 인용한다.
/// `: `, `#`, 앞뒤 공백, 따옴표를 포함하면 큰따옴표로 감싸고 이스케이프한다.
pub fn scalar(s: &str) -> String;
```

```rust
// show.rs

pub enum ShowTarget {
    Document(DocPath),
    Topic(DocPath, String),
}

/// 스펙 6.2 의 YAML 을 만든다.
pub fn show(
    project: &Project,
    table: &SymbolTable,
    graph: &ImportGraph,
    target: &ShowTarget,
) -> String;
```

**출력 스키마** — 스펙 6.2 그대로. 최상위 키 순서는 `path`, `keywords`, `referencingKeywords`, `exceptions`, `covers`, `topics`.

**중복 제거**: 이미 전개한 topic·키워드는 방문 집합에 넣고, 두 번째부터는 경로 문자열만 넣는다. 깊이 제한은 두지 않는다.

`ponytail:` YAML 이미터를 직접 쓴다. 한계는 인용 규칙 — 한글 description 에 `: ` 가 들어가는 경우가 위험 지점이고 `scalar()` 테스트가 그걸 막는다. 스키마가 늘어나면 serde 기반 크레이트로 교체.

- [ ] **Step 1: 실패하는 테스트 작성** — 시나리오 7개
  - `콜론이_포함된_설명은_인용된다`
  - `따옴표가_이스케이프된다`
  - `멀티라인_본문이_literal_scalar_로_나온다`
  - `show_가_정의_키워드와_참조_topic_을_출력한다`
  - `show_가_예외와_커버_본문을_임베드한다`
  - `show_가_참조_topic_을_재귀적으로_임베드한다`
  - `다이아몬드_의존에서_같은_topic_이_한_번만_전개된다`
- [ ] **Step 2: `cargo test` — 실패 확인**
- [ ] **Step 3: `yaml.rs` 이미터와 `scalar()` 구현**
- [ ] **Step 4: `show.rs` 구현 (재귀 임베드 + 방문 집합)**
- [ ] **Step 5: `main.rs` 에 `show` 서브커맨드 연결**
- [ ] **Step 6: `cargo test` 통과 확인**
- [ ] **Step 7: 커밋** — `feat: YAML 이미터와 kang show`

---

## Task 11: `kang bless`

**파일**
- 생성: `src/bless.rs`
- 수정: `src/main.rs`
- 테스트: `tests/cli.rs`

**인터페이스**
- 소비: `Project`, `SymbolTable`, `hash::rev`
- 산출:

```rust
/// rev 핀이 붙은 한 줄의 위치. `docs/A:12` 형식을 파싱한 결과다.
pub struct RefLocation {
    pub doc: DocPath,
    pub line: usize,
}

impl RefLocation {
    /// `docs/A:12` 를 파싱한다. Task 8 의 진단 fix 가 이 형식으로 출력한다.
    pub fn parse(s: &str) -> Result<RefLocation, String>;
}

/// 주어진 위치들의 rev 핀을 대상의 현재 해시로 갱신한다.
/// 갱신한 개수를 반환한다.
pub fn bless(
    project: &Project,
    table: &SymbolTable,
    locations: &[RefLocation],
) -> Result<usize, String>;
```

**구현 요점**
- 해당 줄의 `rev "..."` 부분만 치환한다. 나머지는 건드리지 않는다.
- 지정한 줄에 rev 핀이 없으면 에러로 거부한다.
- 여러 위치를 한 번에 받는다. `kang build` 출력의 `fix` 문자열을 그대로 인자로 넘길 수 있어야 한다.
- 일괄 해제 수단을 만들지 않는다. 심볼 이름만 주고 전체를 갱신하는 경로가 있으면 안 된다.

- [ ] **Step 1: 실패하는 테스트 작성** — 시나리오 5개
  - `rev_핀을_현재_해시로_갱신한다`
  - `여러_위치를_한_번에_갱신한다`
  - `rev_핀이_없는_줄은_거부한다`
  - `갱신_후_build_가_통과한다`
  - `build_출력의_fix_문자열을_그대로_인자로_받는다`
- [ ] **Step 2: `cargo test` — 실패 확인**
- [ ] **Step 3: `RefLocation::parse` 와 `bless` 구현**
- [ ] **Step 4: `main.rs` 에 `bless` 서브커맨드 연결**
- [ ] **Step 5: `cargo test` 통과 확인**
- [ ] **Step 6: 커밋** — `feat: kang bless`

---

## Task 12: 통합 검증

**파일**
- 생성: `tests/fixtures/` (예제 kang 프로젝트), `tests/cli.rs` 에 통합 시나리오 추가

**인터페이스**
- 소비: 전체

**구현 요점**
스펙 V0001 의 예제(`docs/A`, `docs/B`, `docs/C` — 결제·카드결제·무료결제)를 fixture 로 만들고, 전 명령을 실제로 돌린다.

- [ ] **Step 1: fixture 프로젝트 작성** — 스펙 7.3 예제와 동일한 3개 문서
- [ ] **Step 2: 통합 테스트 작성** — 시나리오 6개
  - `fixture_프로젝트가_build_를_통과한다`
  - `상위_문서_수정_후_모든_참조처가_깨진다`
  - `build_출력을_bless_에_그대로_넘기면_전부_해제된다`
  - `순환_import_를_만들면_체인이_출력된다`
  - `show_출력이_유효한_YAML_이다`
  - `error_상태에서는_어떤_조회도_출력되지_않는다`
- [ ] **Step 3: `cargo test` 통과 확인**
- [ ] **Step 4: `cargo clippy -- -D warnings` 통과 확인**
- [ ] **Step 5: 커밋** — `test: 통합 검증과 fixture 프로젝트`

---

## 완료 조건

- [ ] `cargo test` 전부 통과
- [ ] `cargo clippy -- -D warnings` 통과
- [ ] fixture 프로젝트에서 `kang build` 종료 코드 0
- [ ] 스펙 V0001 의 v1 범위(1~6절, 8~9절) 전 항목이 구현됨
- [ ] 7절(코드 연동)은 v2 — 이 플랜의 범위 밖
