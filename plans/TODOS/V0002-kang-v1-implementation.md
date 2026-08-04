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
.github/workflows/
  release.yml  태그 푸시 시 크로스 플랫폼 바이너리 빌드 및 릴리즈
tests/
  hash.rs      정규화와 rev 해시
  parse.rs     파싱 단위 테스트
  check.rs     진단 규칙 테스트
  cli.rs       CLI 통합 테스트
```

책임 분리 기준: `parse`는 파일 하나만 안다. `resolve`는 프로젝트 전체를 안다. `check`는 규칙만 안다. `show`/`yaml`/`bless`는 출력과 수정만 한다.

---

## Task 1: 프로젝트 부트스트랩 + rev 해시

**파일**
- 생성: `Cargo.toml`, `src/main.rs`, `src/hash.rs`
- 테스트: `tests/hash.rs`

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

/// 진단이 가리키는 위치 하나.
pub struct Location {
    pub doc: DocPath,
    pub line: usize,
    /// 이 위치가 왜 관련되는지. 순환 체인이나 iknow 누락처럼
    /// 여러 위치가 얽힌 진단에서 각 위치의 역할을 설명한다.
    pub note: String,
}

/// 진단이 제안하는 수정 하나. LLM 이 그대로 적용할 수 있어야 한다.
pub struct Fix {
    pub doc: DocPath,
    pub line: usize,
    /// 해당 줄을 무엇으로 바꾸거나, 무엇을 덧붙일지.
    pub action: String,
}

/// 진단 하나. iknow 누락과 순환 검출은 본질적으로 다중 위치이므로
/// 위치와 수정 모두 목록이다.
/// 파서부터 진단 규칙까지 전 단계가 이 타입을 공유하므로 ast 에 둔다.
pub struct Diagnostic {
    pub severity: Severity,
    pub code: &'static str,       // 진단 코드. 예: "K012"
    pub message: String,
    pub locations: Vec<Location>, // 최소 1개
    pub fixes: Vec<Fix>,
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

- [ ] **Step 1: 실패하는 테스트 작성** — 시나리오 10개
  - `frontmatter_description_을_읽는다`
  - `description_이_없으면_에러다`
  - `keyword_의_이름과_한줄정의를_읽는다`
  - `계층_키워드를_이름_배열로_읽는다`
  - `topic_헤딩과_본문을_잘라낸다`
  - `본문_백틱을_심볼_참조로_수집한다`
  - `이스케이프된_백틱과_코드펜스_안은_참조가_아니다`
  - `frontmatter_블록_자체가_없으면_에러다`
  - `keyword_에_한줄정의가_없으면_에러다`
  - `짝이_맞지_않는_백틱은_에러다`
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

- [ ] **Step 1: 실패하는 테스트 작성** — 시나리오 11개
  - `keyword_import_를_읽는다`
  - `topic_import_를_읽는다`
  - `exception_import_를_읽는다`
  - `as_alias_를_읽는다`
  - `rev_핀을_읽는다`
  - `exception_과_pending_을_읽는다`
  - `cover_를_읽는다`
  - `iknow_대상_목록을_읽는다`
  - `uncoded_modifier_는_body_에서_제외된다`
  - `구분자가_없는_import_는_에러다`
  - `알_수_없는_modifier_는_에러다`
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
    pub fn by_name(&self, name: &str) -> &[SymbolId];

    pub fn owner(&self, id: SymbolId) -> &DocPath;
    pub fn hash_source(&self, id: SymbolId) -> Option<&str>;
}
```

**구현 요점**
- 디렉토리 순회는 `std::fs::read_dir` 재귀로 직접 쓴다. `.kang` 확장자만 읽는다.
- `hash_source` 는 keyword면 한 줄 정의, topic이면 body, exception이면 `None` 을 준다. Task 8이 이걸 쓴다.
- `by_name` 은 `HashMap<String, Vec<SymbolId>>` 로 미리 색인해 둔다. 매번 선형 스캔하면 iknow 검사가 O(n²)가 된다.
- 읽기 실패와 잘못된 UTF-8 은 그 파일에 대한 진단으로 바꾸고 나머지 파일 처리를 계속한다. 한 파일 때문에 전체가 죽으면 안 된다.

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

## Task 13: 배포 파이프라인

**파일**
- 생성: `.github/workflows/release.yml`, `README.md` 설치 절

**인터페이스**
- 소비: `cargo build --release`
- 산출: 태그 푸시 시 GitHub Releases 에 바이너리 4종

**구현 요점**
kang 의 소비자는 다른 프로젝트의 LLM 에이전트다. 그 프로젝트는 TypeScript 일 가능성이 높고 Rust 툴체인이 있을 이유가 없다. 소스 빌드만으로는 설치 경로가 없다.

- 타깃: `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`
- 트리거는 `v*` 태그 푸시. 브랜치 푸시에서는 돌지 않는다.
- 아티팩트 이름에 타깃 트리플을 넣어 설치 스크립트가 `uname` 으로 고를 수 있게 한다.
- `README.md` 에 curl 한 줄 설치 예시를 넣는다.

- [ ] **Step 1: `release.yml` 작성** — 태그 트리거, 4 타깃 매트릭스 빌드, Releases 업로드
- [ ] **Step 2: 로컬에서 `cargo build --release --target aarch64-apple-darwin` 성공 확인**
- [ ] **Step 3: 테스트 태그를 푸시해 워크플로가 4개 아티팩트를 만드는지 확인**
- [ ] **Step 4: `README.md` 설치 절 작성**
- [ ] **Step 5: 커밋** — `ci: 크로스 플랫폼 릴리즈 워크플로`

---

## 완료 조건

- [ ] `cargo test` 전부 통과
- [ ] `cargo clippy -- -D warnings` 통과
- [ ] fixture 프로젝트에서 `kang build` 종료 코드 0
- [ ] 스펙 V0001 의 3~6절 전 항목이 구현됨 (7절 코드 연동은 v2, 8절은 비목표, 9절은 미결정 사항이라 구현 대상이 아님)
- [ ] 태그 푸시 시 릴리즈 워크플로가 4개 타깃 바이너리를 산출

---

## 구현 착수 전 해결 필요 (BLOCKING)

2026-08-05 `/plan-eng-review` 와 독립 리뷰에서 나온 것들이다. **아래 1~5번이 열려 있는 동안 Task 3, 6, 11 의 테스트를 작성할 수 없다.** 스펙 V0001 을 먼저 고쳐야 한다.

### 스펙 모순 (구현 차단)

1. **셸 백틱 충돌** — 스펙 6.3 의 `` kang refs docs/A.`결제` `` 는 bash/zsh 에서 명령 치환으로 해석되어 실행 불가다. CLI 인자에서 백틱을 뺄지, 인용 규약을 정할지 확정해야 한다. LLM 이 가장 자주 틀릴 지점이다.
2. **`iknow` 문법 미정의 + 순환 금지와 충돌** — `iknow` 는 4장에 문법 정의가 없다. 5.1 이 요구하는 상호 명시는 상호 참조이고, iknow 대상을 import 해야 한다면 순환 error 와 동시에 만족 불가능하다. import 없이 경로만 쓴다면 "미선언 참조는 에러" 원칙의 예외를 명세해야 한다.
3. **rev 핀 부트스트랩 데드락** — 새 import 를 쓸 때 rev 를 얻는 경로가 없다. build 는 핀 없으면 error 이고 `bless` 는 핀 없는 줄을 거부한다. 더미 해시를 손으로 적는 의식이 강제되며, 그 첫 `bless` 는 "검토했다"는 의미를 담지 못한다.
4. **`RefLocation` 줄 번호 주소 지정이 조용히 깨진다** — 스펙 6.0 의 정상 순서는 "문서를 고친 뒤 bless" 인데, 문서를 고치면 줄이 밀린다. 밀린 상태에서 build 출력을 넘기면 엉뚱한 줄의 rev 를 조용히 갱신한다. 주소는 줄이 아니라 심볼 식별자여야 한다.
5. **프로젝트 루트 미정의** — `load(root)` 의 root 를 정하는 규칙이 두 문서 어디에도 없다. import 경로가 루트 상대인지 파일 상대인지도 미정이다. 파서보다 먼저 정해야 한다.

### 설계 결함

6. **일괄 해제 금지가 실제로는 지켜지지 않는다** — 스펙 6.0 은 일괄 해제 수단이 없다고 선언하면서 build 출력을 그대로 `bless` 인자로 넘기도록 규정하고, Task 11·12 가 그것을 테스트로 못박는다. `kang build | xargs kang bless` 가 공식 지원되는 일괄 해제다. 더 근본적으로 마찰을 느낄 주체가 없다. 사용자가 LLM 에이전트이기 때문이다.
7. **마찰의 인센티브가 반대로 걸린다** — `unused import = error` 와 rev 핀 갱신 비용이 겹쳐, 참조를 유지하는 비용이 참조를 지우고 내용을 복붙하는 비용보다 크다. 도구가 막으려는 행동에 보상을 준다.
8. **`kang show` 의 깊이에 제동이 없다** — 다이아몬드 중복 제거는 있으나 DAG 깊이는 무제한이다. 출력이 임계를 넘으면 문서 구조가 잘못됐다는 신호로 보고 **build 를 error 로 막는** 방안을 검토한다. LLM 에게 선택권을 주지 않는다는 원칙과 양립한다.
9. **`exception` 이 rev 강제의 구멍** — exception 은 핀이 없으므로, 이름을 유지한 채 그것이 선언된 topic 의 의미를 바꾸면 커버 문서가 깨지지 않는다. `cover` 가 해당 exception 이 속한 topic 의 rev 를 핀해야 논리가 닫힌다.
10. **백틱 전면 심볼화의 채택 비용** — `200원`, `null`, `POST /payments` 가 전부 미해결 심볼 error 가 된다. 그리고 `.kang` 파일이 GitHub·에디터에서 어떻게 보이는지에 대한 결정이 스펙에 없다. `\`` 는 마크다운 코드 스팬 안에서 이스케이프로 동작하지 않으므로 사실상 호환 포기인데, 그 결정이 명시되지 않았다.

### 전략 (2026-08-05 `/grilling` 으로 검증하기로 결정)

11. **이름이 다른 개념 중복을 감지하지 못한다** — 중복 검출은 문자열이 정확히 같을 때만 작동한다. 실제 SoT 분열은 `결제` / `페이먼트` / `대금 지불` 처럼 다른 이름으로 일어난다. "확률적 기능을 쓰지 않는다" 원칙이 이를 잡을 유일한 수단을 봉쇄한다.
12. **더 단순한 대안이 검토되지 않았다** — 마크다운 유지 + frontmatter `id` + 섹션 SHA 계산 스크립트 + 코드 주석 + CI 대조로 문제 2번을 통째로 얻는다. 파서도 새 문법도 에디터 지원 상실도 없다.

### 미해결 (cross-model tension)

13. **`ancestors()` 의 v1 소비자** — `/plan-eng-review` 는 topic 그래프 이원화를 권고했고 승인됐다(D2). 독립 리뷰는 참조 전파가 코드 참조 전용이므로 v1 에 호출자가 없다고 본다. 확인 결과 후자가 맞아 보인다. `show` 의 재귀 임베드는 import 를 직접 순회하면 된다. Task 5 의 절반과 Task 12 의 전파 테스트를 v2 로 미룰지 미결.

---

## NOT in scope

- **코드 연동 (`kang inspect`)** — v2. 애노테이션 명세와 언어별 추출기가 별도 설계를 요구한다.
- **자연어 검색** — 확률적 기능을 쓰지 않는다는 원칙에 따라 영구 제외. 단, 위 BLOCKING 11번이 이 결정을 재검토 대상으로 만든다.
- **`--depth` 등 출력 축소 옵션** — LLM 에게 덜 읽을 선택권을 주지 않는다. BLOCKING 8번은 옵션이 아니라 error 로 처리하는 방향이다.
- **deprecated 정책 표현** — git 이 히스토리를 담당한다.
- **에디터 지원 (LSP, syntax highlight)** — v1 이후. BLOCKING 10번의 결론에 따라 우선순위가 바뀔 수 있다.
- **crates.io 발행** — GitHub Releases 로 시작한다. 이름 선점이 필요해지면 그때.

## What already exists

빈 저장소다. 재사용할 기존 코드가 없다. 외부에서 가져올 수 있는 것은 다음과 같고, 전부 의도적으로 쓰지 않는다.

| 후보 | 판단 |
|---|---|
| `chumsky` / `pest` / `nom` | 쓰지 않는다. kang 문법은 줄 단위 + 마크다운 혼재라 토큰 스트림 최적화 라이브러리와 궁합이 나쁘다. |
| `clap` | 쓰지 않는다. v1 에 플래그가 하나도 없어 `match` 로 충분하다. 플래그가 생기면 교체. |
| `serde_yaml` 계열 | 쓰지 않는다. 스키마가 고정이고 literal scalar 제어가 필요하다. 인용 규칙은 `scalar()` 테스트가 막는다. |
| `walkdir` | 쓰지 않는다. `std::fs::read_dir` 재귀 15줄. |
| `sha2` | **쓴다.** 표준 라이브러리에 SHA-256 이 없다. |

## Failure modes

| 코드패스 | 실패 시나리오 | 테스트 | 에러 처리 | 사용자에게 보이나 |
|---|---|---|---|---|
| `bless` 다중 위치 | 3번째에서 실패해 파일이 반쯤 갱신됨 | Task 11 (D5 반영 후) | 전체 검증 후 일괄 쓰기 | 보임 |
| `bless` 주소 지정 | 문서 수정으로 줄이 밀려 엉뚱한 줄 갱신 | **없음** | **없음** | **안 보임 — BLOCKING 4** |
| `exception` 의미 변경 | 커버 문서가 안 깨짐 | **없음** | **없음** | **안 보임 — BLOCKING 9** |
| `kang show` 깊이 | 컨텍스트 초과, 잘린 입력으로 LLM 오답 | **없음** | **없음** | **안 보임 — BLOCKING 8** |
| `load()` IO 실패 | 잘못된 UTF-8 파일 | Task 4 (D6 반영) | 진단으로 변환, 나머지 계속 | 보임 |
| 미해결 import | `None` 이후 동작 미정 | Task 6 (D6 반영) | 규칙 추가 예정 | 보임 |
| 파서 실패 | 짝 안 맞는 백틱, 깨진 import | Task 2·3 (D6 반영) | 진단 | 보임 |

**critical gap 3건** — BLOCKING 4, 8, 9 는 테스트도 에러 처리도 없고 실패가 조용하다. 셋 다 스펙 수정이 선행되어야 한다.

## Worktree parallelization

| 단계 | 모듈 | 의존 |
|---|---|---|
| Task 1 | `hash.rs` | — |
| Task 2·3 | `ast.rs`, `parse.rs` | — |
| Task 4·5 | `resolve.rs` | Task 2·3 |
| Task 6·7·8 | `check.rs` | Task 4·5 |
| Task 9 | `main.rs` | Task 6·7·8 |
| Task 10 | `yaml.rs`, `show.rs` | Task 4·5 |
| Task 11 | `bless.rs` | Task 8 |
| Task 12 | `tests/` | 전부 |
| Task 13 | `.github/`, `README.md` | — |

```
Lane A: Task 1                          (독립, hash.rs)
Lane B: Task 2 → 3 → 4 → 5              (순차, 파싱에서 해석까지)
Lane C: Task 13                         (독립, CI만)
        ↓ B 완료 후
Lane D: Task 6 → 7 → 8 → 11             (순차, check.rs 공유 후 bless)
Lane E: Task 10                         (D 와 병렬, yaml/show)
        ↓ D, E 완료 후
Lane F: Task 9 → 12
```

**실행 순서:** A + B + C 를 병렬 워크트리로 시작. B 완료 후 D + E 병렬. 둘 다 끝나면 F.

**충돌 플래그:** Lane D 와 E 는 각각 `check.rs` 와 `yaml.rs`/`show.rs` 만 만지므로 충돌하지 않는다. 단 둘 다 Task 4 의 `SymbolTable` 시그니처에 의존하므로, B 가 끝나기 전에 D·E 를 띄우면 안 된다.

## Implementation Tasks

이번 리뷰의 발견에서 나온 것들이다. Task 1~13 과 별개로, **스펙 수정이 먼저다.**

- [ ] **T1 (P1, human: ~2h / CC: ~20min)** — 스펙 V0001 — 셸에서 실행 가능한 CLI 인자 문법 확정
  - 발견 출처: Outside voice 1번 — `` kang refs docs/A.`결제` `` 는 셸 명령 치환
  - 파일: `plans/*/V0001-kang-language-design.md` 6절
  - 검증: 확정된 예시를 실제 zsh 에서 실행해 오류가 없는지 확인
- [ ] **T2 (P1, human: ~2h / CC: ~20min)** — 스펙 V0001 — `iknow` 문법 정의와 순환 금지 충돌 해소
  - 발견 출처: Outside voice 2번 — 상호 명시가 상호 참조가 되어 순환 error 와 동시 만족 불가
  - 파일: V0001 4장(문법 추가), 5.1
  - 검증: 3개 파일 이름 충돌 시나리오를 손으로 써서 규칙 위반이 없는지 확인
- [ ] **T3 (P1, human: ~1h / CC: ~15min)** — 스펙 V0001 — rev 핀 부트스트랩 경로 정의
  - 발견 출처: Outside voice 3번 — 새 import 에 핀을 얻을 방법이 없음
  - 파일: V0001 4.7, 6.0
  - 검증: 새 import 작성부터 build 통과까지 손으로 따라가 더미 해시 타이핑이 없는지 확인
- [ ] **T4 (P1, human: ~1h / CC: ~15min)** — 스펙 V0001 — bless 주소를 줄 번호에서 심볼 식별자로 변경
  - 발견 출처: Outside voice 4번 — 문서 수정 후 줄이 밀려 엉뚱한 줄을 조용히 갱신
  - 파일: V0001 6.0, V0002 Task 11
  - 검증: 문서를 고친 뒤 bless 하는 통합 테스트 시나리오 추가
- [ ] **T5 (P1, human: ~30min / CC: ~10min)** — 스펙 V0001 — 프로젝트 루트 결정 규칙 정의
  - 발견 출처: Outside voice 10번 — `load(root)` 의 root 가 어디에도 정의되지 않음
  - 파일: V0001 3절
  - 검증: 하위 디렉토리에서 실행해도 `DocPath` 가 동일한지 확인하는 테스트
- [ ] **T6 (P2, human: ~1h / CC: ~15min)** — 스펙 V0001 — `cover` 가 exception 소속 topic 의 rev 를 핀하도록 변경
  - 발견 출처: Outside voice 11번 — exception 경로에서 "100% 기계적 강제"가 거짓
  - 파일: V0001 4.5, 4.7, 5.2
  - 검증: exception 이름 유지 + topic 본문 변경 시 커버 문서가 깨지는 테스트
- [ ] **T7 (P2, human: ~1h / CC: ~15min)** — 스펙 V0001 — show 출력 크기 임계 초과 시 build error 규칙 추가
  - 발견 출처: Outside voice 8번 — DAG 깊이 무제한, 실패가 조용함
  - 파일: V0001 6.2
  - 검증: 깊은 체인 fixture 로 임계 초과 시 error 가 나는지 확인
- [ ] **T8 (P2, human: ~2h / CC: ~20min)** — 스펙 V0001 — 백틱 전면 심볼화의 마크다운 호환 결정 명시
  - 발견 출처: Outside voice 9번 — `200원`, `null` 이 전부 error, 에디터 렌더링 논의 부재
  - 파일: V0001 4.2, 8절
  - 검증: 실제 정책 문서 한 편을 kang 문법으로 옮겨 이스케이프 빈도 측정
- [ ] **T9 (P3, human: ~30min / CC: ~10min)** — V0002 Task 5·12 — `ancestors()` 를 v2 로 미룰지 결정
  - 발견 출처: Cross-model tension — v1 에 호출자가 없음
  - 파일: V0002 Task 5, Task 12
  - 검증: `show` 재귀가 import 직접 순회로 동작하는지 확인

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 0 | — | — |
| Codex Review | `/codex review` | Independent 2nd opinion | 0 | — | — |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | ISSUES_OPEN | 19 issues, 3 critical gaps |
| Design Review | `/plan-design-review` | UI/UX gaps | 0 | — | 해당 없음 (CLI) |
| DX Review | `/plan-devex-review` | Developer experience gaps | 0 | — | — |

**OUTSIDE VOICE:** Codex CLI 는 설치되어 있으나 ChatGPT 계정이 `gpt-5.4` / `gpt-5.1-codex-max` 를 거부해 Claude 서브에이전트로 폴백. 13건 발견, 그중 5건이 구현 차단 수준의 스펙 모순.

**CROSS-MODEL TENSION:** `ancestors()` 의 v1 필요성. Eng review 는 topic 그래프 이원화를 권고(D2 승인). Outside voice 는 참조 전파가 코드 참조 전용이므로 v1 에 호출자가 없다고 판단. 후자가 맞아 보이나 미결.

**VERDICT:** NOT CLEARED — 스펙 V0001 에 구현 차단 모순 5건(BLOCKING 1~5). `/grilling` 으로 전략 전제(BLOCKING 11, 12) 검증 후 스펙을 수정하고 재리뷰 필요.

**기록 실패:** `gstack-review-log` 가 최소 JSON 도 거부함. `bun` 미설치로 gstack bun 기반 바이너리 일부가 동작하지 않는다. 이 리포트가 유일한 기록이다.

**UNRESOLVED DECISIONS:**
- `ancestors()` 와 참조 전파를 v1 에 둘지 v2 로 미룰지 (T9, cross-model tension)
