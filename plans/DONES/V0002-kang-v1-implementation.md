# V0002 — kang v1 구현 플랜

> **에이전트 실행용:** 이 플랜은 `superpowers:subagent-driven-development` 또는 `superpowers:executing-plans` 로 태스크 단위로 실행한다. 각 단계의 진행 상황은 체크박스로 기록한다.

**목표:** `V0001-kang-language-design.md` 의 v1 명세대로 kang 컴파일러와 CLI를 구현한다.

**아키텍처:** Rust 바이너리 하나로 만든다. 프로젝트 전체를 읽어 `Document` AST로 파싱한다. 그다음 심볼 테이블과 import 그래프를 만들고, 진단 규칙을 실행한다. 진단에 error가 하나라도 있으면 조회 명령은 아무것도 출력하지 않는다.

**기술 스택:** Rust 1.97 / 외부 의존성은 `sha2` 하나

## 전역 제약

- 스펙 원본은 `plans/DONES/V0001-kang-language-design.md` 이다(완료하면 이 경로로 옮긴다). 모든 규칙의 근거는 이 스펙이다.
- **의존성을 더하지 않는다.** `sha2` 말고는 크레이트를 넣지 않는다. 인자 파싱과 YAML 출력은 직접 구현한다. v1의 플래그는 `bless --import` 하나뿐이고 YAML 스키마는 고정되어 있기 때문이다.
- `kang build` 의 기본 심각도는 **error** 이다. error 가 생기면 종료 코드는 1 이고, 조회 명령은 아무것도 출력하지 않는다.
- 주석은 TSDoc 에 해당하는 한글 규격(rustdoc `///`)으로 쓴다. 함수, 구조체, enum 모두에 쓴다.
- 로깅 규칙은 적용하지 않는다. CLI 는 한 번 실행하고 끝나므로 로그 레벨 시스템을 두지 않는다. 진단 출력이 로그 역할을 한다.
- 모든 진단 메시지에는 **어디를 어떻게 고칠지**를 넣는다. LLM이 진단만 보고 스스로 고칠 수 있어야 한다.
- 모든 테스트는 `cargo test` 하나로 실행한다.
- **`FixKind::Shell` 의 `action` 에서는 경로와 이름을 반드시 인용한다.** 스펙 4.2, 6.0, 6.1 세 곳에서 정해 둔 조항이다. 근거는 "인용 여부를 스스로 판단하게 두면 틀린다" 이다. `format!("... ls -l {}", p)` 처럼 값을 인용 없이 그대로 넣으면, `/Users/x/My Project` 같은 경로를 셸이 두 인자로 나눈다. 그러면 사용자는 원인 진단 대신 새로 생긴 잘못된 정보를 받는다. 값은 작은따옴표로 감싸고, 값 안의 `'` 는 `'\''` 로 바꾼다.

**Task 4 에서 `resolve::shell_quote(value: &str) -> String` 을 `pub(crate)` 로 만들어 두었다. 셸 fix 를 새로 만드는 태스크는 반드시 이 함수를 쓴다.** 매개변수가 `&Path` 가 아니라 `&str` 인 이유가 있다. 경로뿐 아니라 심볼 이름도 인용해야 하기 때문이다 (`kang bless docs/B --import 'docs/A.결제'`, 스펙 6.0).

## 파일 구조

```
Cargo.toml
src/
  main.rs      CLI 디스패치, list/keywords/refs 출력
  ast.rs       AST 타입 정의
  parse.rs     렉서 + 파서 (파일 1개 → Document)
  hash.rs      정규화 + rev 해시
  resolve.rs   프로젝트 로드, 심볼 테이블
  check.rs     진단 규칙(순환 검출 포함) + 진단 출력
  yaml.rs      YAML 이미터
  show.rs      show 출력 구성 (재귀 임베드 + 중복 제거)
  bless.rs     rev 핀 갱신
  init.rs      에이전트 진입점 생성 (스킬·CLAUDE.md·AGENTS.md·첫 문서)
  skill.md     kang 스킬 원본. include_str! 로 임베드된다
.github/workflows/
  release.yml  태그 푸시 시 크로스 플랫폼 바이너리 빌드 및 릴리즈
tests/
  hash.rs      정규화와 rev 해시
  parse.rs     파싱 단위 테스트
  check.rs     진단 규칙 테스트
  cli.rs       CLI 통합 테스트
```

책임을 나누는 기준은 다음과 같다. `parse`는 파일 하나만 안다. `resolve`는 프로젝트 전체를 안다. `check`는 규칙만 안다. `show`/`yaml`/`bless`는 출력과 수정만 한다.

**경로 문자열을 만드는 코드는 한 곳에만 둔다.** `list`, `keywords`, `refs`, `show`, 진단이 모두 "전체 경로 문자열"을 만든다. 각자 따로 만들면 다섯 곳의 결과가 조금씩 달라진다. 그래서 `ast.rs` 에 `impl Display for DocPath` 와 `SymbolRef` 의 표시 함수를 두고, 모든 곳이 이 함수만 쓴다. CLI 인자 파싱(`ImportAddress::parse`)은 같은 문법을 반대 방향으로 읽는 일이므로 같은 모듈에 둔다.

---

## Task 1: 프로젝트 부트스트랩 + rev 해시

**파일**
- 생성: `Cargo.toml`, `src/main.rs`, `src/hash.rs`
- 테스트: `tests/hash.rs`

**인터페이스**
- 제공: `hash::normalize(&str) -> String`, `hash::rev(&str) -> String`

```rust
/// 해시 입력 텍스트를 정규화한다.
/// 앞뒤 공백 제거, 줄 끝 공백 제거, 연속 빈 줄 축약.
pub fn normalize(text: &str) -> String;

/// 정규화된 텍스트의 SHA-256 앞 6자리 hex 를 반환한다.
pub fn rev(text: &str) -> String;
```

- [x] **Step 1: `cargo init --name kang` 실행, `Cargo.toml`에 `sha2` 추가**
- [x] **Step 2: 실패하는 테스트 작성** — 시나리오 3개
  - `trailing_whitespace_does_not_change_hash`
  - `consecutive_blank_lines_collapse_to_one`
  - `different_body_gives_different_hash`
- [x] **Step 3: `cargo test` — 컴파일 실패 확인**
- [x] **Step 4: `normalize`, `rev` 구현**
  - 줄 단위로 `trim_end`, 빈 줄 2개 이상은 1개로, 전체 `trim`
  - `sha2::Sha256` 결과를 hex로 만들고 앞 6자
- [x] **Step 5: `cargo test` 통과 확인**
- [x] **Step 6: 커밋** — `feat: rev 해시 산출과 정규화 규칙`
  - c5eb31d, 14d6b81 / 테스트 3

**수행 내역** — c5eb31d, 14d6b81 / 테스트 3. normalize/rev 를 확정했다. lib 크레이트로 바꿔서 tests 가 내부 항목에 접근한다

---

## Task 2: 파서 — frontmatter, keyword, topic

**파일**
- 생성: `src/ast.rs`, `src/parse.rs`
- 수정: `src/main.rs` (모듈 선언)
- 테스트: `tests/parse.rs`

**인터페이스**
- 사용: 없음
- 제공: 아래 AST 타입 전부와 `parse::parse_document(path, source) -> Result<Document, Vec<Diagnostic>>`

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

/// 수정의 종류. 문법 규약이 갈리므로 판별자가 필요하다.
/// Edit 은 문서 문법(백틱 사용, 스펙 4.2), Shell 은 CLI 문법(백틱 금지·인용, 스펙 6.0).
pub enum FixKind { Edit, Shell }

/// 진단이 제안하는 수정 하나. LLM 이 그대로 적용할 수 있어야 한다.
/// **줄 번호를 좌표로 쓰지 않는다** (ADR-0003).
/// `Diagnostic.fixes` 는 **순서 있는 목록**이며 앞에서부터 적용한다.
pub struct Fix {
    pub kind: FixKind,
    /// Edit 이면 대상 문서. Shell 이면 None.
    pub doc: Option<DocPath>,
    /// 어디에 무엇을 적용할지 한 문장으로.
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
    /// 한 줄 정의 안의 백틱 심볼과 등장 줄.
    /// 스펙 4.2 는 "본문과 **선언부**의 모든 백틱은 심볼 참조" 이므로
    /// keyword 정의 안의 참조도 Task 6 의 미해결 심볼 검사 대상이다.
    /// 이 필드가 없으면 keyword 정의가 kang 의 강제를 빠져나가는 은신처가 된다.
    pub refs: Vec<(String, usize)>,
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

이 태스크에서는 `imports`, `exceptions`, `covers` 를 빈 벡터로 둔다. 이 필드는 Task 3에서 채운다.

**구현 요점**
- frontmatter는 `---` 로 감싼 블록이다. `description` 이 없으면 error 이다.
- `keyword` 줄의 순서: 이름(계층은 `.` 로 나눈다) → `:` → 한 줄 정의 → 선택 항목 `` #`상세 topic` ``. 동의어 문법은 없다.
- `##` 로 시작하는 줄에서 새 topic 이 시작한다. 다음 `##` 바로 앞까지가 body 이다.
- 백틱 찾기: `` \` `` 는 글자 그대로의 백틱이다. ` ``` ` 펜스 안은 건너뛴다. 나머지 백틱 쌍은 심볼 참조로 보고 `refs` 에 기록한다.
- 모든 노드에 줄 번호를 기록한다. 진단의 품질은 줄 번호에 달려 있다.

- [x] **Step 1: 실패하는 테스트 작성** — 시나리오 10개
  - `reads_frontmatter_description`
  - `missing_description_is_error`
  - `reads_keyword_name_and_one_line_definition`
  - `reads_hierarchical_keyword_as_name_array`
  - `splits_topic_heading_and_body`
  - `collects_body_backticks_as_symbol_refs`
  - `escaped_backticks_and_code_fence_are_not_refs`
  - `missing_frontmatter_block_is_error`
  - `keyword_without_one_line_definition_is_error`
  - `unmatched_backtick_is_error`
- [x] **Step 2: `cargo test` — 실패 확인**
- [x] **Step 3: `ast.rs` 타입 정의 (`Diagnostic` 포함)**
- [x] **Step 4: `parse.rs` 구현 — frontmatter, keyword, topic, 백틱 스캔**
- [x] **Step 5: `cargo test` 통과 확인**
- [x] **Step 6: 커밋** — `feat: frontmatter/keyword/topic 파싱`
  - c877e2e~1f40f96 (13커밋) / 테스트 29

**수행 내역** — c877e2e~1f40f96 (13커밋) / 테스트 29. K101~K105. **컨트롤러가 스펙에 없는 `\\` 이스케이프를 만들었고, 리뷰에서 이를 반박해 되돌렸다**(스펙 4.2 는 `` \` `` 하나만 정의한다)

---

## Task 3: 파서 — import, exception, cover, modifier

**파일**
- 수정: `src/parse.rs`
- 테스트: `tests/parse.rs`

**인터페이스**
- 사용: Task 2의 AST 타입 전부, `parse::parse_document`
- 제공: 같은 함수가 `imports` / `exceptions` / `covers` / `iknow` / `uncoded` 를 채운다

**구현 요점**
- import 문법: `` import `docs`/`A`.`결제` as `A 결제` rev "a3f9c1" ``
  - `/` 는 경로, `.` 는 keyword, `#` 는 topic, `!` 는 exception 이다. 마지막 구분자로 `kind` 를 정한다.
  - `.` 는 keyword 에 들어간 뒤 계층을 나눌 때에도 쓴다. 첫 `.` 뒤는 모두 키워드 이름 조각이다.
  - `as`, `rev` 는 선택 토큰이다. 여러 개를 묶는 문법은 없다. 한 줄에 import 하나만 쓴다.
  - **세 종류 모두 rev 핀을 가질 수 있다.** exception 도 마찬가지다 (스펙 4.7).
- `` exception `이름` [pending] `` 과 `` cover `이름` `` 은 topic 본문 안에서 인식한다.
- `// iknow <대상>, <대상>, …` 은 keyword 줄, topic 헤딩 줄, exception 줄 뒤에 붙는다. 대상이 여럿이면 쉼표로 나열한다.
  - **`iknow` 는 import 가 아니다.** 파서는 대상을 `Vec<SymbolRef>` 에 담기만 하고, `imports` 에는 넣지 않는다. iknow 가 그래프의 간선이 되지 않는다는 점이 스펙 4.4 의 핵심이다.
- `// uncoded` 는 topic 헤딩 줄 뒤에만 붙는다.
- 이 modifier들은 topic `body` 에서 뺀다. modifier 가 rev 해시에 들어가면 안 된다.

**컨트롤러가 넘긴 일 (Task 2 리뷰 2회에서 나왔다. 반드시 처리한다)**

- **`K105` 를 검사하는 범위를 좁혀야 한다.** Task 2 는 topic 헤딩 줄에 백틱이 하나라도 있으면 `K105` error 를 출력한다. 근거는 다음과 같다. 스펙 6.0 은 CLI 인자에 백틱을 쓰지 못하게 한다. 그러므로 헤딩에 백틱이 있으면 CLI 에서 그 topic 의 주소를 쓸 수 없다. 그런데 스펙 4.4 에 따르면 `// iknow` 는 topic 헤딩에 붙고, 그 대상에는 백틱이 들어간다. 예를 들어 `` ## 결제의 방법 // iknow `docs`/`B`.`결제의 방법` `` 은 올바른 문서인데, 지금은 `K105` 로 거부된다.

  따라서 Task 3 은 **헤딩에서 modifier(`// iknow …`, `// uncoded`)를 먼저 잘라 내고, 남은 헤딩 텍스트에만 `K105` 를 적용**하도록 고쳐야 한다. 순서를 반대로 하면 올바른 문서가 거부된다.

- **`Topic.name` 에서도 modifier 를 빼야 한다.** 지금은 헤딩 원문 전체가 `name` 이 된다. 그래서 modifier 를 잘라 내지 않으면 `` 조직의 문서 검토 절차 // uncoded `` 처럼 조회할 수 없는 이름이 에러 없이 만들어진다. `K105` 오류는 사라지는데 이름은 망가지는 경우가 가장 나쁘다. 그러므로 두 문제를 함께 고친다.

- **topic 밖 줄의 백틱 짝 검사.** Task 2 의 `parse.rs` 에서는 첫 `##` 앞의 줄일 때 `topics.last_mut()` 가 `None` 이다. 그래서 일찍 `continue` 하고, 그 줄은 백틱 짝 검사(`K104`)를 받지 않는다. Task 3 은 `import` 줄을 파싱하면서 이 부분을 다룬다. 그러므로 import 줄과 그 밖의 topic 밖 줄에 백틱 짝 검사를 적용할지 여기서 정하고 적어 둔다. import 줄은 반드시 검사해야 한다. 심볼 주소를 백틱으로 쓰기 때문이다.

- **타입을 다시 정의하지 않는다.** Task 2 가 확정한 `ast.rs` 의 14개 타입은 필드 이름, 타입, 순서가 고정이다. Task 3 은 빈 필드를 채우기만 한다. 필드를 늘려야 할 이유가 생기면, 구현하지 말고 컨트롤러에 보고한다.

- [x] **Step 1: 실패하는 테스트 작성** — 시나리오 13개
  - `reads_keyword_import`
  - `reads_topic_import`
  - `reads_exception_import`
  - `reads_as_alias`
  - `reads_rev_pin`
  - `reads_exception_and_pending`
  - `reads_cover`
  - `reads_comma_separated_iknow_targets`
  - `iknow_targets_are_not_in_imports`
  - `reads_rev_pin_on_exception_import`
  - `uncoded_modifier_is_excluded_from_body`
  - `import_without_delimiter_is_error`
  - `unknown_word_is_part_of_name_not_modifier`, `text_left_after_uncoded_is_error`
- [x] **Step 2: `cargo test` — 실패 확인**
- [x] **Step 3: import 파싱 구현 (경로 구분자로 kind 결정)**
- [x] **Step 4: exception / cover / modifier 파싱 구현**
- [x] **Step 5: `cargo test` 통과 확인**
- [x] **Step 6: 커밋** — `feat: import/exception/cover/modifier 파싱`
  - c99bf18~40eda89 (10커밋) / 테스트 80

**수행 내역** — c99bf18~40eda89 (10커밋) / 테스트 80. K106~K112. K105 범위를 topic 헤딩으로 좁혔다. 스펙 3절에 「topic 밖 금지」를 더했다

---

## Task 4: 프로젝트 로드 + 심볼 테이블

**파일**
- 생성: `src/resolve.rs`
- 수정: `src/main.rs`
- 테스트: `tests/check.rs`

**인터페이스**
- 사용: `ast::*`, `parse::parse_document`
- 제공:

```rust
/// 파싱된 문서 전체를 담는다.
pub struct Project {
    pub docs: HashMap<DocPath, Document>,
}

/// 프로젝트 루트를 찾는다. git 저장소 루트가 곧 kang 프로젝트 루트다.
/// git 저장소가 아니면 그 사실을 진단으로 돌려준다.
pub fn find_root(cwd: &Path) -> Result<PathBuf, Diagnostic>;

/// 루트를 재귀 순회하며 .kang 파일을 전부 읽어 파싱한다.
/// 모든 DocPath 는 루트 기준 상대 경로이므로 어느 하위 디렉토리에서 실행해도 동일하다.
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
    pub fn hash_source(&self, id: SymbolId) -> &str;
}
```

**구현 요점**
- 디렉토리 순회는 `std::fs::read_dir` 를 재귀로 호출해 직접 구현한다. 확장자가 `.kang` 인 파일만 읽는다.
- `hash_source` 는 세 종류 모두 값을 가진다. keyword 는 한 줄 정의, topic 은 body, **exception 은 그 예외를 선언한 topic 의 body** 이다 (스펙 4.8). Task 8이 이 값을 쓴다.
- `by_name` 은 `HashMap<String, Vec<SymbolId>>` 로 미리 색인을 만들어 둔다. 매번 처음부터 끝까지 찾으면 iknow 검사가 O(n²)가 된다.
- 파일을 읽지 못하거나 UTF-8 이 잘못되었으면, 그 파일에 대한 진단을 만들고 나머지 파일은 계속 처리한다. 파일 하나 때문에 전체 실행이 중단되면 안 된다.

- [x] **Step 1: 실패하는 테스트 작성** — 시나리오 8개
  - `finds_git_root_as_project_root`
  - `docpath_is_same_when_run_from_subdir`
  - `emits_diagnostic_outside_git_repo`
  - `reads_all_kang_files_in_subdirs`
  - `ignores_non_kang_files`
  - `finds_own_file_symbols_in_scope`
  - `finds_imported_alias_in_scope`
  - `groups_same_name_symbols_in_by_name`
- [x] **Step 2: `cargo test` — 실패 확인**
- [x] **Step 3: `find_root` 와 `load` 구현 (git 루트 탐색 + 재귀 순회 + 파싱)**
- [x] **Step 4: `SymbolTable` 구현**
- [x] **Step 5: `cargo test` 통과 확인**
- [x] **Step 6: 커밋** — `feat: 프로젝트 로드와 전역 심볼 테이블`
  - f230932~b06d6ca (4커밋) / 테스트 106

**수행 내역** — f230932~b06d6ca (4커밋) / 테스트 106. find_root/load/SymbolTable, K050~K052. `shell_quote` 을 전역 제약으로 옮겼다

---

## Task 5: 순환 검출

**파일**
- 생성: `src/check.rs`
- 테스트: `tests/check.rs`

**인터페이스**
- 사용: `Project`
- 제공:

```rust
/// 파일 단위 import 관계를 DFS 로 훑어 순환을 검출한다.
/// 그래프 값을 남기지 않는다 — v1 에 질의할 소비자가 없다.
pub fn check_cycles(project: &Project) -> Vec<Diagnostic>;
```

**구현 요점**
- **노드는 파일이다.** 스펙 5.1 의 순환 규칙이 파일 단위이기 때문이다. 파일 그래프가 DAG 이면 topic 그래프도 DAG 이다. topic 간선 T→U 가 있으면 반드시 file(T)→file(U) 간선도 있다. 그러므로 파일 단위로 금지하는 쪽이 더 강한 규칙이다.
- **`iknow` 는 간선이 아니다.** 서로 iknow 로 명시한 것을 순환으로 잡으면 안 된다 (스펙 4.4). 간선은 `imports` 에서만 만든다.
- 순환 검출은 DFS 로 하고, 방문한 노드에 색을 칠해 표시한다. 순환을 찾으면 스택을 그대로 체인으로 출력한다. `fix` 에는 "공통 개념을 상위 파일로 추출하라"를 넣는다.
- 그래프 값을 구조체로 남기지 않는다. `ancestors()` 를 v2 로 미룬 뒤로 조회 메서드가 없으므로, `check.rs` 의 함수 하나로 충분하다. `ancestors()` 와 참조 전파는 **v2** 범위다. 참조 전파는 코드가 topic 을 참조할 때 적용하는 규칙이다. 코드 연동은 v2 이므로 v1 에는 이 규칙을 호출할 곳이 없다. `show` 가 import 한 내용을 재귀로 펼쳐 넣을 때는 `Document.imports` 를 직접 순회한다.

- [x] **Step 1: 실패하는 테스트 작성** — 시나리오 4개
  - `detects_direct_cycle`
  - `prints_full_chain_of_three_step_cycle`
  - `mutual_iknow_is_not_cycle`
  - `self_import_is_cycle`
- [x] **Step 2: `cargo test` — 실패 확인**
- [x] **Step 3: 파일 그래프 구성과 DFS 순환 검출 구현**
- [x] **Step 4: `cargo test` 통과 확인**
- [x] **Step 5: 커밋** — `feat: import 그래프와 순환 검출`
  - 76673a0~d93dd26 (4커밋) / 테스트 117

**수행 내역** — 76673a0~d93dd26 (4커밋) / 테스트 117. K040 파일 단위 순환. `compile()` 의 연결 표는 Task 9 로 넘겼다

---

## Task 6: 진단 — 심볼 규칙

**파일**
- 수정: `src/check.rs`
- 테스트: `tests/check.rs`

**인터페이스**
- 사용: `Project`, `SymbolTable`, `ast::Diagnostic` (Task 2에서 정의)
- 제공:

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
| **import 대상 파일이나 심볼이 존재하지 않음** | error |
| import 했으나 어떤 topic 에서도 미사용 | error |
| 한 심볼에 두 개 이상 alias | error |
| 서로 다른 파일이 같은 이름 심볼 선언, iknow 없음 | error |
| iknow 가 관련 파일 전체를 상호 명시하지 않음 | error |
| **iknow 대상 파일이나 심볼이 존재하지 않음** | error |

- **이름 충돌은 계층 전체 경로로 판단한다.** `결제`.`상태` 와 `구독`.`상태` 는 다른 이름이므로 충돌이 아니다 (스펙 4.3).
- iknow 검사는 양쪽이 서로 명시했는지까지 본다. N개 파일이 같은 이름을 선언하면, 각 파일이 나머지 N-1개를 모두 명시해야 한다. 빠진 파일 경로는 `fixes` 에 나열한다.
- **iknow 는 import 가 아니므로 미사용 검사를 하지 않는다.** 경로가 실제로 있는지와 서로 명시했는지만 본다 (스펙 4.4).
- 사용 여부는 topic 별로 기록한다. 파일 전체에서 한 번이라도 썼으면 통과한다.

**컨트롤러가 넘긴 일 (Task 4 리뷰에서 나왔다. 반드시 처리한다)**

- **`scope()` 의 키는 계층 전체 경로인데, 본문 `refs` 는 백틱 쌍마다 따로 나뉜 조각이다.** `Symbol.name` 과 `scope()` 키는 `"결제수단.카드"` 처럼 `.` 로 이은 전체 이름이다. 그런데 파서는 본문의 `` `결제수단`.`카드` `` 를 `("결제수단", n)` 과 `("카드", n)` **두 항목**으로 나눠 `refs` 에 넣는다.

  **`refs` 의 각 항목을 `scope()` 에서 그대로 찾으면, 올바른 문서에서도 미해결 심볼 error 가 난다.** 그러므로 찾기 전에 붙어 있는 조각을 합쳐야 한다. `Topic.body` 는 헤딩을 포함한 원문을, `Keyword.definition` 은 원래 정의를 그대로 가지고 있다. 그래서 조각이 붙어 있었는지 원문에서 다시 알아낼 수 있다. resolve 쪽이 고른 방식(전체 경로를 키로 쓰기)은 스펙 4.3, 5.1 에 맞으므로 resolve 쪽은 바꾸지 않는다.

- **`SymbolId` 에는 kind, name, 선언 줄을 읽는 공개 접근자가 없다.** 그래서 `by_name`/`owner` 로 파일을 좁힌 뒤, `Project.docs` 를 다시 찾아 `K012` 출력의 줄 번호를 만든다. 작업을 시작할 때 이 방법이 실제로 동작하는지 먼저 확인한다. 동작하지 않으면 구현하지 말고 컨트롤러에 보고한다.

**알려진 한계 — 파서가 지원하지 않으면 고칠 수 없다 (Task 6 재검증에서 확정)**

`Topic.refs` 와 `Keyword.refs` 는 `(이름, 줄)` 만 가진다. **원문에서 조각들이 `.` 으로 이어져 있었는지는 가지지 않는다.** 그래서 Task 6 의 참조 해석은 스코프만 보고 조각을 합친다. 조각을 나누는 방법 중에서 모든 조각이 해석되는 방법이 여럿이면, 왼쪽부터 가장 길게 합치는 방법을 고른다.

그 결과 **올바른 문서를 거부하는 경우가 하나 남는다.** 본문이 `` `A` 와 `B` `` 처럼 두 이름을 따로 썼는데 스코프에 `A.B` 도 있으면, 두 조각이 하나로 합쳐진다. 그러면 뒤 조각을 따로 import 한 줄이 미사용(`K003`)으로 잘못 판단된다. 진단을 놓치는 문제가 아니라, 올바른 문서를 실제로 거부하는 문제다.

근본 해결 방법은 파서가 조각이 원문에서 붙어 있었는지를 `refs` 에 넣는 것이다. 이 방법은 `ast.rs`, `parse.rs` 를 바꾼다. `Vec<(String, usize)>` 를 붙어 있는 조각 묶음을 나타낼 수 있는 형태로 바꾸는 변경이다. 그렇게 하면 Task 6 의 백트래킹이 아예 필요 없어진다. **v1 범위에서는 이 한계를 받아들인다. Task 12 통합 검증에서 실제 문서 모음에서 얼마나 자주 생기는지 측정한 뒤, 이 변경을 할지 정한다.**

- **문서 사이의 이름 충돌은 Task 4 가 진단하지 않는다.** `SymbolTable::build` 는 한 문서 안의 로컬 이름 중복(`K052`)만 출력한다. 다른 파일에 같은 이름이 있다는 사실은 `by_name` 으로 알 수 있을 뿐이다. 여기에 "iknow 없음" 조건을 붙여 진단하는 일은 이 태스크가 맡는다 (`K010`-`K019` 범위).

**진단 출력 형식** — 스펙 5.1.1 에 완성된 예시 3건이 있다. 그 형식을 따른다. 그때그때 새로 정하지 않는다.

테스트는 글자 하나하나가 같은지가 아니라 **구조가 같은지**를 본다. 예시에 적힌 경로와 줄 번호까지 맞추려면 예시에 맞춰 fixture 를 거꾸로 만들어야 한다. 그러면 스펙의 오타를 고칠 때마다 테스트가 깨진다. 검사할 항목은 `code` 값, `locations` 개수와 각 `note` 가 있는지, `fixes` 의 종류와 순서, 셸 명령을 인용했는지이다.

- 모든 진단은 `code`(`K001` 형식), `message`, `locations`(최소 1개), `fixes` 를 가진다.
- `locations` 의 각 항목은 문서 경로, 줄 번호, `note`(그 위치가 왜 관련되는지)를 가진다. 위치가 여럿인 대표적인 예는 순환 체인과 iknow 누락이다.
- `fixes` 의 각 항목은 문서 경로와 **해야 할 동작**을 가진다. 줄 번호로 고칠 위치를 가리키지 않는다 (ADR-0003).
- 셸 명령이 들어 있는 `fix` 는 **인용을 붙여 출력**한다. 에이전트가 복사해서 그대로 실행할 수 있어야 한다.

- [x] **Step 1: 실패하는 테스트 작성** — 시나리오 17개
  - `undeclared_backtick_symbol_is_error`
  - `importing_missing_file_is_error`
  - `importing_missing_symbol_is_error`
  - `unused_import_is_error`
  - `two_aliases_for_one_symbol_is_error`
  - `name_conflict_without_iknow_is_error`
  - `one_sided_iknow_is_error`
  - `three_file_conflict_requires_naming_other_two`
  - `error_when_iknow_target_is_missing`
  - `different_hierarchy_same_leaf_is_not_conflict`
  - `diagnostic_includes_all_related_locations`
  - `shell_command_fix_is_printed_quoted`
  - `unresolved_symbol_diagnostic_matches_spec_5_1_1`
  - `iknow_missing_diagnostic_matches_spec_5_1_1`
  - `rev_mismatch_diagnostic_matches_spec_5_1_1`
  - `edit_fix_uses_doc_syntax_and_shell_fix_uses_cli_syntax`
  - `fixes_are_listed_in_apply_order`
- [x] **Step 2: `cargo test` — 실패 확인**
- [x] **Step 3: `report` 구현** — 스펙 5.1.1 의 예시 3건과 구조가 일치해야 한다
- [x] **Step 4: `check_symbols` 의 8개 규칙 구현**
- [x] **Step 5: `cargo test` 통과 확인**
- [x] **Step 6: 커밋** — `feat: 심볼 진단 규칙`
  - 865fa42~1fd97a6 (17커밋) / 테스트 164

**수행 내역** — 865fa42~1fd97a6 (17커밋) / 테스트 164. K001~K012, K019. **올바른 입력을 거부하는 문제를 다섯 번 찾아 고쳤다.** 계층 참조는 DP 로 나눠 해석하게 했다. 하위 선언이 상위 import 를 쓰면 그 import 를 사용한 것으로 인정했다

---

## Task 7: 진단 — exception 상태 기계

**파일**
- 수정: `src/check.rs`
- 테스트: `tests/check.rs`

**인터페이스**
- 사용: `Project`, `SymbolTable`, `Diagnostic`
- 제공: `check::check_exceptions(project, table) -> Vec<Diagnostic>`

```rust
/// 스펙 5.2 의 exception 상태 기계를 검사한다.
pub fn check_exceptions(project: &Project, table: &SymbolTable) -> Vec<Diagnostic>;
```

**진리표** (스펙 5.2)

| exception 상태 | cover 있음 | cover 없음 |
|---|---|---|
| 일반 | 통과 | error |
| `pending` | error | warn |

그리고 **한 exception 을 둘 이상의 topic 이 cover 하면 error** 이다.

**컨트롤러가 넘긴 일 (Task 6 에서 나왔다. 반드시 처리한다)**

**`cover` 대상이 없으면 error 이다.** Task 6 은 `Topic.covers` 를 사용 여부를 판단할 때만 썼다. 대상이 실제로 있는지는 검사하지 않는다. Task 6 은 이 문제를 진단하지 않기로 했다. 그러므로 여기서 진단하지 않으면 아무도 진단하지 않는다.

`` cover `없는 예외` `` 처럼 어떤 topic 도 선언하지 않은 이름을 cover 하는 경우를 보자. 진리표는 이 경우를 그냥 짝이 없는 것으로 넘기고, 문서는 통과한다. kang 이 막으려는 dangling 참조가 바로 이런 모양이다.

이 태스크는 진리표를 위해 exception↔cover 매핑을 이미 만든다. 그 매핑에 "짝이 없는 cover" 분기를 더하는 쪽이, Task 6 에서 같은 매핑을 다시 만드는 쪽보다 짧다.

- [x] **Step 1: 실패하는 테스트 작성** — 시나리오 5개
  - `uncovered_exception_is_error`
  - `covered_exception_passes`
  - `pending_without_cover_is_warn`
  - `pending_with_cover_is_error`
  - `exception_covered_twice_is_error`
- [x] **Step 2: `cargo test` — 실패 확인**
- [x] **Step 3: `check_exceptions` 구현**
- [x] **Step 4: `cargo test` 통과 확인**
- [x] **Step 5: 커밋** — `feat: exception 상태 기계 검증`
  - 69b0139~1f82d2b (4커밋) / 테스트 185

**수행 내역** — 69b0139~1f82d2b (4커밋) / 테스트 185. K030~K034. **fix 를 적용해도 새 진단이 생기지 않는다는 것을 처음으로 테스트로 확인했다**(6검사 테스트)

---

## Task 8: 진단 — rev 핀

**파일**
- 수정: `src/check.rs`
- 테스트: `tests/check.rs`

**인터페이스**
- 사용: `Project`, `SymbolTable`, `hash::rev`
- 제공: `check::check_revs(project, table) -> Vec<Diagnostic>`

```rust
/// 스펙 4.8 의 rev 핀을 검사한다.
pub fn check_revs(project: &Project, table: &SymbolTable) -> Vec<Diagnostic>;
```

**검사 항목**

| 상황 | 심각도 |
|---|---|
| import 에 rev 핀 없음 (keyword, topic, exception 세 종류 모두) | error |
| rev 핀이 대상의 현재 해시와 불일치 | error |

해시 입력은 `SymbolTable::hash_source` 에서 가져온다. keyword 는 한 줄 정의, topic 은 body, **exception 은 그 예외를 선언한 topic 의 body** 이다.

- 핀이 맞지 않거나 없을 때의 진단 `fix` 에는 `kang bless <문서> --import '<심볼>'` 형태를 넣는다. Task 11의 `bless` 가 그대로 받는 형식이다. 셸 인용을 붙여 출력한다.
- **줄 번호를 쓰지 않는다** (ADR-0003). 보통은 문서를 고친 뒤 bless 를 실행하므로, 그 사이에 줄 번호가 바뀐다.
- **diff 를 출력하지 않는다.** kang 은 이전 본문을 저장하지 않는다. 무엇이 바뀌었는지는 `git diff` 로 볼 수 있다는 안내만 넣는다.

- [x] **Step 1: 실패하는 테스트 작성** — 시나리오 6개
  - `missing_rev_pin_is_error`
  - `matching_rev_pin_passes`
  - `changed_target_body_is_error`
  - `exception_pin_is_hash_of_declaring_topic_body`
  - `changing_declaring_topic_keeping_name_breaks_cover_doc`
  - `diagnostic_fix_is_bless_symbol_address_form`
- [x] **Step 2: `cargo test` — 실패 확인**
- [x] **Step 3: `check_revs` 구현**
- [x] **Step 4: `cargo test` 통과 확인**
- [x] **Step 5: 커밋** — `feat: rev 핀 검증`
  - 7625722~6bd6361 (3커밋) / 테스트 201

**수행 내역** — 7625722~6bd6361 (3커밋) / 테스트 201. K020, K021. 저장소 전체에 걸친 일 4건을 Task 12 로 넘겼다

---

## Task 9: CLI 골격 + build / list / keywords / refs

**파일**
- 수정: `src/main.rs`
- 테스트: `tests/cli.rs`

**인터페이스**
- 사용: `resolve::load`, `SymbolTable`, `check::*`
- 제공: `parse_project()` 와 `compile()` 두 진입점
- 제공: 실행 가능한 `kang` 바이너리

```rust
/// 서브커맨드를 파싱한다.
enum Command {
    Init,
    Build,
    /// kang bless <문서> --import <심볼>
    Bless { doc: String, import: String },
    List(Option<String>),
    Keywords(Option<String>),
    Refs(String),
    Show(String),
    /// v2 기능. 종료 코드 3 으로 "아직 구현되지 않았다" 를 알린다.
    Inspect,
}

/// 프로젝트를 로드하고 모든 진단을 돌린다.
/// error 가 하나라도 있으면 Err 를 반환한다.
fn compile() -> Result<(Project, SymbolTable), Vec<Diagnostic>>;
```

**인자 문법** (스펙 6.0)
- **인자에 백틱을 쓰지 않는다.** 셸은 백틱을 명령 치환으로 처리하고, 비대화형 호출에서도 문제가 생긴다. 에이전트가 kang 을 호출하는 방식이 바로 비대화형 호출이다. 그래서 kang 은 오류 메시지 없이 잘못 동작한다.
- `kang refs docs/A.결제`, `kang show 'docs/A#결제의 방법'` 처럼 `/`, `.`, `#`, `!` 만으로 파싱한다.
- 공백이 있는 이름은 셸 인용을 해야 한다. 이것은 일반적인 셸 규칙이다.

**출력 규칙** (스펙 6.3)
- 한 줄만 읽어도 뜻을 완전히 알 수 있어야 한다. 경로는 항상 전체 경로로 쓰고, 계층을 줄여 쓰지 않는다.
- `kang list [경로]` → `docs/A: {description}`
- `kang keywords [경로]` → `docs/A.결제: 사용자가 상품 대금을 지불하는 행위`
- `kang refs <키워드>` → `docs/A#결제의 방식`
- `keywords` 스코프는 **경로 스코프만** 지원한다.

**종료 코드** (스펙 6절)

| 코드 | 의미 |
|---|---|
| 0 | 성공 |
| 1 | 컴파일 error 존재 |
| 2 | 사용법 오류 (알 수 없는 명령, 인자 부족) + 환경 오류 (git 저장소 아님) |
| 3 | v2 기능 호출 (`kang inspect`) |

에이전트는 종료 코드를 보고 다음 동작을 고른다. 1과 2를 같은 코드로 쓰면 "문서를 고쳐야 한다" 와 "명령을 잘못 썼다" 를 구분할 수 없다.

**환경 오류에는 `--help` 를 출력하지 않는다.** git 저장소가 아닌 것은 명령을 잘못 쓴 것이 아니다. 이때 help 를 보여 주면 에이전트는 철자가 틀렸다고 생각하고 다시 시도한다. 그러므로 `git init` 을 실행하라는 안내만 출력한다.

**컨트롤러가 넘긴 일 (Task 4 에서 나왔다). `Location.line == 0` 은 "가리킬 줄이 없음" 이다.**

`K050`(git 저장소 아님), `K051`(파일을 읽지 못함) 처럼 **가리킬 줄이 없는 진단**이 있다. `Location.line` 은 `usize` 이므로 `Option` 을 쓸 수 없다(타입 계약이 고정되어 있다). `1` 을 적으면 진단이 있지도 않은 줄을 가리키므로 틀린 정보가 된다. 그래서 Task 4 는 `0` 을 "줄 없음" 을 뜻하는 특수값으로 쓴다.

`report()` 의 출력 함수는 `line == 0` 을 **따로 처리해야 한다.** `docs/A:0` 으로 출력하면 안 된다. 파일 경로만 쓰거나(`K051`), 경로도 의미가 없으면 위치 표시를 모두 뺀다(`K050`).

**그리고 `line == 0` 인 진단에서는 `Location.doc` 이 문서 주소가 아니다.** `DocPath` 는 원래 "루트 기준 상대 경로 조각들" 이다. 그런데 `K050`, `K051`(디렉토리)은 절대 경로 전체를 조각 하나에 넣는다 (`DocPath(vec!["/Users/x/tmp"])`). `Display` 는 조각을 `/` 로 이어 붙이므로 화면 출력은 맞다. 그러나 이 값을 문서로 열거나 주소로 다시 만들면 안 된다. `line == 0` 인지 보고 이 경우를 구분한다.

**`resolve::find_root` 에는 절대 경로를 넘겨야 한다.** 상대 경로를 넘기면 `ancestors()` 의 마지막 빈 경로가 프로세스 cwd 기준으로 해석된다. 그러면 `find_root` 가 빈 루트를 `Ok` 로 돌려준다. `std::env::current_dir()` 의 값을 그대로 넘긴다. 이 값은 항상 절대 경로다.

**핵심 규칙**: 조회 명령은 모두 `compile()` 을 먼저 실행한다. error가 있으면 진단만 출력하고 종료 코드 1로 끝낸다. 문서 내용은 한 줄도 출력하지 않는다.

**`bless` 와 `init` 은 `compile()` 을 실행하지 않는다.** `bless` 가 필요한 상황은 정의에 따라 모두 error 이다. 핀이 없어도 error, 핀이 맞지 않아도 error 이다. `compile()` 을 통과해야 실행된다면 `bless` 는 절대 실행될 수 없다. 그래서 `bless` 는 `parse_project()` 까지만 쓴다.

```rust
/// 파싱만 한다. 진단 규칙을 돌리지 않는다.
/// bless 처럼 error 상태에서 실행되어야 하는 명령이 쓴다.
fn parse_project() -> Result<(Project, SymbolTable), Vec<Diagnostic>>;

/// parse_project + 모든 진단 규칙. error 가 하나라도 있으면 Err.
fn compile() -> Result<(Project, SymbolTable), Vec<Diagnostic>>;
```

**컨트롤러가 넘긴 일 (Task 5 리뷰에서 나왔다. 가장 먼저 연결할 곳)**

**`compile()` 이 진단 함수를 하나라도 빠뜨리면, 그 규칙은 아무 표시 없이 동작하지 않는다.** Task 5 때에는 `src` 안에 `check_cycles` 를 호출하는 곳이 없었다. 함수와 테스트는 완성했지만, 실제 실행 경로에는 연결하지 않은 상태였다. 다음 태스크에서 만드는 진단 함수도 같은 위험이 있다.

`compile()` 은 **다음 함수를 모두** 실행하고 진단을 모은다. 하나라도 빠지면 그 스펙 규칙은 동작하지 않는다.

| 함수 | 모듈 | 태스크 | 스펙 |
|---|---|---|---|
| `parse_document` (파일마다) | `parse` | 2, 3 | 4절 문법 |
| `find_root`, `load` | `resolve` | 4 | 3절 |
| `SymbolTable::build` | `resolve` | 4 | 4.3 |
| `check_cycles` | `check` | 5 | 5.3 |
| `check_symbols` | `check` | 6 | 5.1 |
| `check_exceptions` | `check` | 7 | 5.2 |
| `check_revs` | `check` | 8 | 4.8 |

**연결 여부를 테스트로 고정한다.** 규칙마다 그 규칙을 어기는 최소 프로젝트를 만들고, `compile()` 이 그 진단을 출력하는지 확인한다. 함수를 직접 호출하는 단위 테스트만으로는 연결이 빠진 것을 찾지 못한다.

**컨트롤러가 넘긴 일 (Task 7 에서 나왔다). 종료 코드는 진단 개수가 아니라 `Severity::Error` 개수로 정한다.**

`K031`(`pending` 인데 cover 없음)은 **`Severity::Warn` 인 유일한 진단**이다. 스펙 5.2 는 그 칸을 error 가 아니라 경고로 정했다. 그리고 전역 제약은 "`kang build` 의 기본 심각도는 **error** 이다. error 가 생기면 종료 코드는 1 이고, 조회 명령은 아무것도 출력하지 않는다." 이다.

따라서 `compile()` 이 `Err` 를 돌려주는 조건과 `main` 이 종료 코드 1 을 돌려주는 조건은 같다. **진단 벡터가 비어 있는지가 아니라, 그 안에 `Severity::Error` 가 있는지**로 정한다. `diagnostics.is_empty()` 로 판단하면, `K031` 하나만 있는 정상 문서도 빌드 실패가 된다.

`K031` 만 있는 프로젝트에서 **종료 코드가 0** 이고 그 경고는 출력되는지를 테스트로 고정한다.

`bless` 가 실행되는 범위는 다음과 같다. **파싱이 실패하면 `bless` 도 실패한다.** 대상 import 줄을 찾을 수 없기 때문이다. 파싱이 성공하면 진단이 몇 개이든 `bless` 는 계속 진행한다.

**에이전트가 가장 먼저 보는 것은 `--help` 이다.** 인자를 잘못 쓴 에이전트는 다음으로 `kang --help` 를 실행한다. 이 출력에서 명령, 인자 형식, 종료 코드를 모두 보여 주어야 다시 시도할 때 성공한다. 사용법 오류일 때도 같은 텍스트를 출력한다.

`ponytail:` 인자 파싱은 직접 구현한다. 플래그는 `bless --import` 하나뿐이므로 `match` 로 충분하다. 플래그가 늘어나면 clap 으로 바꾼다.

- [x] **Step 1: 실패하는 테스트 작성** — 시나리오 13개
  - `build_exits_0_on_valid_project`
  - `build_exits_1_on_error`
  - `query_commands_print_nothing_on_error`
  - `list_prints_full_paths`
  - `keywords_filtered_by_path_scope`
  - `refs_prints_topics_referencing_keyword`
  - `parses_argument_without_backticks`
  - `unknown_subcommand_prints_usage_and_exits_2`
  - `missing_arguments_prints_usage`
  - `reports_zero_kang_files`
  - `inspect_exits_3_with_v2_notice`
  - `help_shows_all_commands_arg_forms_and_exit_codes`
  - `non_git_repo_prints_only_git_init_hint_not_help`
- [x] **Step 2: `cargo test` — 실패 확인**
- [x] **Step 3: 서브커맨드 디스패치와 `compile()` 구현**
- [x] **Step 4: `build` / `list` / `keywords` / `refs` 출력 구현**
- [x] **Step 5: `cargo test` 통과 확인**
- [x] **Step 6: 커밋** — `feat: CLI 골격과 build/list/keywords/refs`
  - 4270cf8~05ea6e9 (4커밋) / 테스트 230

**수행 내역** — 4270cf8~05ea6e9 (4커밋) / 테스트 230. **바이너리가 처음으로 동작했다.** 리뷰에서 직접 실행해 세 가지 문제를 찾았다. EPIPE 패닉, UTF-8 이 아닌 인자의 패닉, `refs` 의 DP 결과가 맞지 않는 문제다 → `check::split_names` 로 추출했다

---

## Task 10: YAML 이미터 + `kang show`

**파일**
- 생성: `src/yaml.rs`, `src/show.rs`
- 수정: `src/main.rs`
- 테스트: `tests/cli.rs`

**인터페이스**
- 사용: `Project`, `SymbolTable`
- 제공:

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
    /// 키 아래에 항목 목록을 선언적으로 넣는다.
    pub fn seq(&mut self, key: &str, items: impl IntoIterator<Item = Emitter>);
    /// 키 아래에 중첩 매핑을 선언적으로 넣는다.
    pub fn map(&mut self, key: &str, body: Emitter);
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

/// 스펙 6.4 의 YAML 을 만든다.
pub fn show(
    project: &Project,
    table: &SymbolTable,
    target: &ShowTarget,
) -> String;
```

**출력 스키마** — 스펙 6.4 를 그대로 따른다. 최상위 키 순서는 `path`, `keywords`, `referencingKeywords`, `exceptions`, `covers`, `topics` 이다.

- keyword 항목에는 `name`, `description`, `referencedBy` 를 넣는다. `#` 로 연결한 상세 topic 이 있으면 **`detail`(그 topic 의 전체 경로)** 도 넣는다. 파싱한 값을 버리지 않는다.
- import 한 내용을 재귀로 펼쳐 넣을 때는 `Document.imports` 를 직접 순회한다. 참조 전파는 v2 범위다.

**중복 제거**: 이미 펼친 topic 과 키워드는 방문 집합에 넣는다. 두 번째부터는 경로 문자열만 넣는다. **v1 에서는 깊이 제한을 두지 않는다.** 나중에 깊이 제한을 만들 때는 읽을 때 쓰는 옵션이 아니라, 빌드할 때 구조를 검사하는 린트로 만든다 (스펙 6.4).

`ponytail:` YAML 이미터는 직접 구현한다. 이미터 API 는 선언형 `seq`/`map` 만 공개하고, 들여쓰기를 직접 조정하는 커서는 주지 않는다. 그래서 들여쓰기 짝을 잘못 맞추는 실수가 구조상 생길 수 없다. 남은 한계는 인용 규칙이다. 한글 description 에 `: ` 가 들어가는 경우가 위험하다. `scalar()` 테스트가 이 경우를 막는다. 스키마가 늘어나면 serde 기반 크레이트로 바꾼다.

- [x] **Step 1: 실패하는 테스트 작성** — 시나리오 7개
  - `description_with_colon_is_quoted`
  - `quotes_are_escaped`
  - `multiline_body_becomes_literal_scalar`
  - `show_prints_defined_keywords_and_ref_topics`
  - `show_embeds_exception_and_cover_body`
  - `show_embeds_ref_topics_recursively`
  - `diamond_dependency_expands_topic_once`
- [x] **Step 2: `cargo test` — 실패 확인**
- [x] **Step 3: `yaml.rs` 이미터와 `scalar()` 구현**
- [x] **Step 4: `show.rs` 구현 (재귀로 펼쳐 넣기 + 방문 집합)**
- [x] **Step 5: `main.rs` 에 `show` 서브커맨드 연결**
- [x] **Step 6: `cargo test` 통과 확인**
- [x] **Step 7: 커밋** — `feat: YAML 이미터와 kang show`
  - cf4c171, 103b852 / 테스트 255

**수행 내역** — cf4c171, 103b852 / 테스트 255. **리뷰에서 잘못된 YAML 을 출력하는 입력 3종을 찾았고, 두 파서로 재현했다**(제어문자, U+2028, `=`). 이 문제는 251 passed 상태에서 발견되었다

---

## Task 11: `kang bless`

**파일**
- 생성: `src/bless.rs`
- 수정: `src/main.rs`
- 테스트: `tests/cli.rs`

**인터페이스**
- 사용: `parse_project()` 의 결과, `hash::rev`. **`compile()` 을 쓰지 않는다.** bless 가 필요한 상황은 모두 error 이기 때문이다
- 제공:

```rust
/// 갱신 대상 import 를 가리키는 주소. `docs/A.결제` 를 파싱한 결과다.
/// 줄 번호를 쓰지 않는다 (ADR-0003).
pub struct ImportAddress {
    pub target: SymbolRef,
}

impl ImportAddress {
    /// `docs/A.결제`·`docs/A#결제의 방법`·`docs/A!무료 상품` 을 파싱한다.
    /// Task 8 의 진단 fix 가 이 형식으로 출력한다. 백틱은 쓰지 않는다.
    pub fn parse(s: &str) -> Result<ImportAddress, String>;
}

/// `doc` 안에서 `addr` 이 가리키는 import 의 rev 핀을 현재 해시로 맞춘다.
/// 핀이 있으면 갱신하고 없으면 삽입한다 (스펙 4.8).
pub fn bless(
    project: &Project,
    table: &SymbolTable,
    doc: &DocPath,
    addr: &ImportAddress,
) -> Result<(), String>;
```

**구현 요점**
- **주소는 심볼이다.** 보통은 문서를 고친 뒤 bless 를 실행하므로 줄 번호가 바뀐다. 줄 번호로 핀을 찾아 고치면, bless 가 아무 표시 없이 다른 핀을 바꾼다 (ADR-0003).
- 한 파일 안에서 같은 심볼을 두 번 import 하면 이미 error 이다. 그러므로 주소가 가리키는 import 는 하나뿐이다.
- 주소가 가리키는 import 줄의 `rev "..."` 부분만 바꾸고, **`rev` 가 없으면 줄 끝에 넣는다.** 나머지는 바꾸지 않는다.
- 주소가 가리키는 import 가 그 문서에 없으면 에러로 거부한다.
- 여러 핀을 한 번에 푸는 방법은 만들지 않는다. 심볼 이름만 주고 그 심볼을 참조하는 모든 곳의 핀을 고치는 방법이 있으면 안 된다.

- [x] **Step 1: 실패하는 테스트 작성** — 시나리오 9개
  - `bless_updates_wrong_pin_to_current_hash`
  - `bless_inserts_pin_into_import_without_pin`
  - `bless_fixes_only_given_import_when_lines_shift`
  - `bless_rejects_import_not_in_doc`
  - `build_passes_after_bless`
  - `running_build_fix_command_as_is_passes`
  - `bless_accepts_three_kinds_of_address_without_backticks`
  - `bless_runs_despite_other_diagnostics`
  - `bless_does_not_run_on_parse_failure`
- [x] **Step 2: `cargo test` — 실패 확인**
- [x] **Step 3: `ImportAddress::parse` 와 `bless` 구현**
- [x] **Step 4: `main.rs` 에 `bless` 서브커맨드 연결**
- [x] **Step 5: `cargo test` 통과 확인**
- [x] **Step 6: 커밋** — `feat: kang bless`
  - 48a2c47, 1e23eeb / 테스트 275

**수행 내역** — 48a2c47, 1e23eeb / 테스트 275. **이제 rev 핀 error 를 풀 수 있다.** TOCTOU 방어 코드가 위치를 확인하지 않아서, 핀이 import 줄이 아닌 일반 문장에 들어가는 문제가 있었다. 다시 파싱해서 위치를 확인하도록 고쳤다. pid 를 쓰자는 수정 제안이 있었는데, 리뷰에서 빌드해 측정하고 반박했다(조용한 유실 0/12→12/12)

---

## Task 12: 통합 검증

**파일**
- 생성: `tests/fixtures/` (예제 kang 프로젝트), `tests/cli.rs` 에 통합 시나리오 추가

**인터페이스**
- 사용: 전체

**구현 요점**
스펙 V0001 의 예제(`docs/A`, `docs/B`, `docs/C` — 결제, 카드결제, 무료결제)를 fixture 로 만들고, 모든 명령을 실제로 실행한다.

- [x] **Step 1: fixture 프로젝트 작성** — 결제, 카드결제, 무료결제 3개 문서. git 저장소여야 루트 찾기가 동작하므로 fixture 도 저장소로 만든다
  - `tests/cli.rs` 의 `example_project` / `example_project_passes`. 처음에는 핀 없이 쓰고, `kang bless` 가 핀을 넣게 한다 (스펙 4.8 3단계). fixture 는 임시 디렉토리에 만든다. `tests/` 안에 두면 이 저장소의 워크트리 안에 저장소가 하나 더 생긴다
- [x] **Step 2: 통합 테스트 작성** — 시나리오 10개 (하나는 Task 14 로 넘겨 그곳에서 만들었다)
  - [x] `fixture_project_passes_build`
  - [x] `editing_upstream_doc_breaks_all_referrers`
  - [x] `passing_build_output_to_bless_clears_all` — C1 의 회귀 테스트
  - [x] `bless_after_fixing_referrers_first_updates_correct_pin`
  - [x] `changing_exception_declaring_topic_breaks_cover_doc`
  - [x] `cyclic_import_prints_chain`
  - [x] `show_output_is_valid_yaml` — python3+pyyaml 이 있을 때만 실행하고, 없으면 건너뜀
  - [x] `no_query_prints_in_error_state`
  - [x] `git_init_kang_init_and_build_pass` — **Task 14 로 넘겼다.** Task 12 때는 `kang init` 이 종료 코드 3 을 돌려주어서 만들 수 없었다
    - Task 14 커밋 9a5410c 가 이 테스트를 더했다. `KANG_REQUIRE_INDEX=1 cargo test -p kang --test cli git_init_kang_init_and_build_pass -- --exact` 로 실행해 통과했다(1 passed)
  - [x] `three_diagnostic_kinds_structure_matches_spec_5_1_1`
**컨트롤러가 넘긴 일 (Task 8 리뷰에서 나왔다. 저장소 전체를 한 번에 처리한다)**

- **`[shell]` fix 의 `action` 이 한글 설명 문장으로 시작한다. 그래서 출력된 줄을 그대로 실행할 수 없다.** `report()` 는 `[shell] {action}` 형식으로 출력한다. 그런데 action 이 `이 import 에 rev 핀을 붙이세요: kang bless 'docs/b' --import 'docs/a#결제의 방법'` 이다. 그래서 줄 전체를 복사해 실행하면 `command not found: 이` 가 나온다.

  스펙 5.1.1("그대로 적용 가능한 fix", 예시 `[shell] kang bless ...`)과 6.1("인용까지 포함되어 출력되므로 복사해 실행하면 된다")이 요구하는 것은 **명령만 있는 줄**이다.

  한 태스크만의 문제가 아니다. `K001`, `K020`, `K021`, `K050` 등 저장소의 모든 Shell fix 가 같은 모양이다. 설명 문장은 `message` 나 `note` 로 옮기고, `action` 에는 명령만 남긴다. **모든 곳을 한 번에 고치고, 출력 결과를 셸에 그대로 넣어 실행해 보는 테스트를 더한다.**

- **비슷한 진단끼리 fix 규칙이 서로 다르다.** `K030`, `K031`, `K034` 의 fix 는 핀 없는 import 줄만 만든다. 그래서 적용하면 `K020` 이 새로 나온다. 핀 없는 import 줄은 스펙 4.8 의 3단계 절차 중 1단계이므로 결함은 아니다. 그런데 `K001` 은 bless 명령을 함께 출력한다. 같은 상황인데 한쪽은 "fix 적용 후 build 다시 실행" 을 1번, 다른 쪽은 2번 거쳐야 한다. 이 출력을 쓰는 쪽은 에이전트다. 그러므로 이 과정을 거치는 횟수가 곧 비용이다. 실제 흐름을 실행해 보고 통일할지 정한다.

- **같은 topic 이 같은 예외를 두 번 cover 하면 error 인가.** 스펙 5.2 의 문구는 "둘 이상의 topic 이" 이다. 그런데 `K033` 은 cover 선언 개수로 센다. 구현이 스펙 문구보다 넓게 검사한다. 스펙 문구를 고칠지, 검사를 topic 단위로 바꿀지 정한다.

- **참조 병합 한계가 실제로 얼마나 자주 생기는지 측정한다.** Task 6 절의 "알려진 한계"를 본다. 본문이 두 이름을 따로 썼는데 하나로 합쳐지면, 뒤 조각의 import 가 `K003` 으로 잘못 판단된다. 도그푸딩 문서 모음에서 이 문제가 얼마나 자주 생기는지 측정하고, 파서를 고칠지 정한다.

- [x] **Step 3: `cargo test` 통과 확인** — 293 passed (275 → 290 → 293)
- [x] **Step 4: `cargo clippy -- -D warnings` 통과 확인** — `--all-targets` 로 clean, `cargo fmt --check` 도 clean
- [x] **Step 5: 커밋** — 결정 J1~J4 와 넘겨받은 일의 검증 결과는 `.superpowers/sdd/V0002-kang-v1-implementation/task-12-report.md` 에 있다
  - C1(출력된 `[shell]` fix 를 그대로 실행할 수 없음) — 저장소에서 문제가 있는 **아홉 곳을 모두** 고쳤다. `check.rs` 3곳(`K020`, `K021`, `K001`)과 `resolve.rs` 6곳(`K050`, `K051`×5)이다. `K050` 의 `action` 은 `git init` 으로 하고, 다른 방법은 `note` 로 옮겼다
  - `not_utf8` 의 둘째 fix 는 삭제했다. `-f` 인자는 첫 fix 를 실행한 뒤에야 정해지므로 명령으로 출력할 수 없다. 변환 절차는 `message` 로 옮겼다. **명령이 아닌 것을 `Shell` 로 출력한 것이 C1 의 근본 원인이다**
  - M8(주소 파싱 통합): `refs` 가 `ImportAddress::parse` 를 쓰게 하고, `run_show` 도 같은 순서로 나누게 맞췄다. `main.rs` 는 606 → 597 줄이 되었다. **하나로 합치고 나서야, 셋 중 `show` 하나만 정본과 다르게 동작하고 있었다는 것을 알았다**
  - J1 의 결론에 따라 `K034` 에 `bless` 명령을 함께 넣어, **"fix 적용 후 build 다시 실행" 을 1번만** 거치면 끝나게 했다. `K030`, `K031` 은 핀을 붙일 문서를 알 수 없으므로 2번 거치는 것이 정본이다
  - **스펙 6.0 의 주소 제약을 다시 썼다** (3라운드 리뷰). "마지막 조각 안의 구분자는 하나" 라는 규칙은 틀렸다. 이 규칙은 스펙 `:68`, `:91` 의 계층 keyword 와 이 태스크의 픽스처(`docs/B.결제수단.카드`)를 잘못된 것으로 만들었다. 금지하는 것은 세 가지뿐이다. 문서 파일 이름의 `.`, `#`, `!`(`K113`), 심볼 이름의 `/`(빌드를 영원히 막으므로 우선순위가 가장 높다), keyword 이름 한 조각 안의 `.` 이다

**수행 내역** — f3080b9~06a6921 (4커밋) / 테스트 293. **저장소 전체의 fix 계약이 지켜진다.** 진단이 알려 준 명령을 복사해 실행하면 실제로 문제가 고쳐진다. 주소 파싱 코드 3개를 1개로 합치면서 `show` 가 다르게 동작하던 숨은 문제를 찾았다

---

## Task 13: 배포 파이프라인

**파일**
- 생성: `.github/workflows/release.yml`, `README.md` 설치 절

**인터페이스**
- 사용: `cargo build --release`
- 제공: 태그 푸시 시 GitHub Releases 에 바이너리 4종

**구현 요점**
kang 을 쓰는 것은 다른 프로젝트의 LLM 에이전트다. 그 프로젝트는 TypeScript 일 가능성이 높고, Rust 툴체인이 설치되어 있을 이유가 없다. 소스 빌드만 제공하면 설치할 방법이 없다.

- 타깃: `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`
- `v*` 태그를 푸시할 때 실행한다. 브랜치를 푸시할 때는 실행하지 않는다.
- 아티팩트 이름에 타깃 트리플을 넣어 설치 스크립트가 `uname` 으로 고를 수 있게 한다.
- `README.md` 에 curl 한 줄 설치 예시를 넣는다.

- [x] **Step 1: `release.yml` 작성** — 태그 트리거, 4 타깃 매트릭스 빌드, Releases 업로드
- [x] **Step 2: 로컬에서 `cargo build --release --target aarch64-apple-darwin` 성공 확인**
- [x] **Step 3: 테스트 태그를 푸시해 워크플로가 4개 아티팩트를 만드는지 확인**
- [x] **Step 4: `README.md` 설치 절 작성**
- [x] **Step 5: 커밋** — `ci: 크로스 플랫폼 릴리즈 워크플로`
  - 172cc5e / 테스트 293

**수행 내역** — 172cc5e / 테스트 293. release.yml 4타깃 + README + `KANG_REQUIRE_YAML`. curl 이 받는 이름과 아티팩트 이름이 네 조합 모두 같은지 직접 확인했다. **remote 가 없어서 태그 푸시는 확인하지 못했다**

---

## Task 14: `kang init` — 에이전트 진입점과 스킬

**파일**
- 생성: `src/init.rs`, `src/skill.md`
- 수정: `src/main.rs`
- 테스트: `tests/cli.rs`

**인터페이스**
- 사용: 없음 (파일 생성만)
- 제공:

```rust
/// 현재 프로젝트에 에이전트 진입점을 만든다.
/// 기존 파일은 덮어쓰지 않고 섹션만 덧붙인다.
pub fn init(root: &Path) -> Result<Vec<PathBuf>, String>;

/// 파일이 없으면 만들고, 있으면 marker 로 시작하는 섹션이 없을 때만 덧붙인다.
/// 네 산출물이 전부 이 한 함수로 처리된다 — 복사·섹션 덧붙임·한 줄 덧붙임·
/// 템플릿 생성은 전부 같은 규칙의 변형이다.
fn ensure_section(path: &Path, marker: &str, content: &str) -> Result<bool, String>;
```

**왜 필요한가**

kang 의 주 사용자는 다른 프로젝트에서 일하는 LLM 에이전트다. 그런데 "LLM 은 원본을 보지 않는다" 는 원칙은 **강제할 수 없다.** 에이전트는 `.kang` 파일을 그냥 읽을 수 있기 때문이다.

그래서 이 원칙은 에이전트가 kang 이 있다는 것과 그 사용법을 알 때만 지켜진다. 저장소에 `.kang` 파일만 있으면, 에이전트는 그 파일이 무엇인지 모르고 `cat` 으로 읽는다. 그런데 `cat` 으로 읽은 `.kang` 은 import 로 다른 곳을 가리키므로 **마크다운보다 읽기 어렵다.** 도구를 들여왔는데 오히려 더 나빠진다.

**생성물**

| 파일 | 처리 |
|---|---|
| `.claude/skills/kang/SKILL.md` | 바이너리에 넣어 둔 `src/skill.md` 의 내용. 이미 있으면 건너뛴다. **스킬 내용의 유일한 사본** |
| `AGENTS.md` | Codex 용. 내용을 복제하지 않고 `SKILL.md` 경로를 가리키는 섹션만 덧붙인다 |
| `CLAUDE.md` | 한 줄 덧붙임 — "이 프로젝트의 문서는 kang 으로 관리한다. kang 스킬을 사용한다" |
| `docs/example.kang` | frontmatter 가 채워진 첫 문서 템플릿. 이미 `.kang` 파일이 있으면 건너뛴다 |

**저장소에 커밋하는 프로젝트 범위 파일로 만든다.** 전역 스킬 설치를 요구하면 설치 단계가 늘어난다. 그리고 clone 하는 사람마다 상태가 달라진다.

**구현 요점**
- `src/skill.md` 는 `include_str!("skill.md")` 로 **컴파일할 때 바이너리에 넣는다.** 릴리즈한 바이너리는 저장소 파일을 읽을 수 없기 때문이다.
- **스킬 내용은 `SKILL.md` 한 곳에만 쓴다.** `AGENTS.md`, `CLAUDE.md` 는 그 파일을 가리키기만 한다. 두 파일에 내용을 복사하면, kang 이 막으려는 SoT 분열이 kang 자신의 도구에서 생긴다.
- **git 저장소를 요구하지 않는다.** `init` 은 방금 만든 디렉토리에서 실행하는 첫 명령이다. git 저장소가 아니면 현재 디렉토리를 루트로 쓰고, `git init` 안내를 함께 출력한다. 여기서 종료 코드 2로 끝나면, 사용자가 첫 실행에서 바로 막힌다(T0 벽).
- 생성한 `SKILL.md` 는 kang 버전이 올라가도 바뀌지 않는다. **알려진 한계다.** `init` 은 파일이 이미 있으면 건너뛰는 멱등 동작이므로, CLI 가 바뀌면 기존 저장소의 스킬은 오래된 내용으로 남는다. 사용자가 생긴 뒤 측정하고 결정한다.

**스킬 내용** — 스펙 6.1 의 다섯 케이스를 그대로 쓴다. 이 플랜에는 복사하지 않는다. 두 곳에 두면 서로 어긋난다.

- [x] **Step 1: 실패하는 테스트 작성** — 시나리오 8개
  - `creates_four_files`
  - `appends_section_without_overwriting_existing_claude_md`
  - `skips_when_kang_section_exists`
  - `skips_example_when_kang_files_exist`
  - `build_passes_right_after_init`
  - `generated_skill_md_is_not_empty`
  - `init_succeeds_outside_git_repo_and_suggests_git_init`
  - `appends_only_kang_section_to_claude_md_with_other_tools`
- [x] **Step 2: `cargo test` — 실패 확인**
- [x] **Step 3: `src/skill.md` 작성** — 위 다섯 케이스 전부
- [x] **Step 4: `init.rs` 구현 (섹션 덧붙임, 멱등성)**
- [x] **Step 5: `main.rs` 에 `init` 서브커맨드 연결**
- [x] **Step 6: `cargo test` 통과 확인**
- [x] **Step 7: 커밋** — `feat: kang init 과 에이전트 스킬`
  - 9a5410c / 테스트 302

**수행 내역** — 9a5410c / 테스트 302. **TTHW 목표를 달성했다.** `git init`→`init`→`build` 세 명령이 exit 0 이다. `--help` 양방향 검사를 런타임 검사로 옮겼다

---

## 완료 조건

- [x] `cargo test` 전부 통과
  - 309 passed / 0 failed (check 121, cli 60+, parse 77+, yaml 14, hash 3)
- [x] `cargo clippy -- -D warnings` 통과
  - `--all-targets` 로 exit 0. `cargo fmt --check` 도 exit 0. `#[allow(...)]` 전역 0건
- [x] fixture 프로젝트에서 `kang build` 종료 코드 0
  - `tests/cli.rs:2146` `fixture_project_passes_build`. 스펙 예제 3문서를 `tests/cli.rs` 안에 인라인 픽스처로 두었다 (따로 `tests/fixtures/` 디렉토리를 만들지 않았다. git 저장소 안에 저장소가 또 생기지 않도록, 각 테스트가 임시 디렉토리에 픽스처를 만든다)
- [x] 스펙 V0001 의 3~6절 전 항목이 구현됨 (7절은 v2 `V0003` 로 분리, 8절은 비목표, 9절은 미결정 사항이라 구현 대상이 아님)
  - **V0004 에서 해결했다.** 부족했던 두 가지는 이렇게 처리했다. 심볼 이름의 `/` 금지는 `K115`(`parse.rs`, 선언과
    참조 모두)로 구현했다. keyword 조각의 `.` 금지는 스펙에서 삭제했다(V0001 6.0). V0004 완료 조건을 본다.
  - **일부만 달성.** 최종 리뷰에서 절 단위로 비교한 결과, 구현하지 않은 것은 6.0 의 세 가지 금지 중 둘뿐이다. 하나는 심볼 이름의 `/` 금지다(`K105` 와 비슷한 검사, 셋 중 빌드를 막는 쪽). 다른 하나는 keyword 이름 한 조각 안의 `.` 금지다. 문서 파일 이름의 `.`, `#`, `!` 는 `K113` 으로 구현했다. 그 밖의 3~6절 항목은 모두 구현했다. 코드에는 있고 스펙에는 없는 진단 번호는 0건이다. 9절(미결정)을 스펙 없이 구현한 곳도 0건이다. `/` 금지는 V0004 우선순위 2 이다. 코드 심볼 이름에는 `crate::mod::fn` 이 들어가므로, proc-macro 작업을 시작하기 전에 해결해야 한다
- [x] 태그 푸시 시 릴리즈 워크플로가 4개 타깃 바이너리를 생성
  - **완료 (V0004 이후, remote 를 연결하고 실제로 확인했다).** 테스트용으로 쓰고 버릴 태그 `v0.0.1-rc1` 로 `release.yml`
    을 실행했다. 4타깃 모두 성공했고, 아티팩트 이름이 README 의 curl 네 조합과 같았다. 그 curl 을
    그대로 실행해 바이너리를 받고, `--help` 가 실행되는 것까지 확인했다. 근거는 V0004 의 완료
    조건에 적었다.
  - **확인하지 못함 (사용자가 승인한 결정 D3).** git remote 도 `act` 도 없어서 실제 실행을 확인할 방법이 없다. 확인한 것은 다음과 같다. `release.yml` 이 올바른 YAML 이고 트리거가 `{'push': {'tags': ['v*']}}` 이다. `actionlint` 가 exit 0 이다(일부러 잘못 만든 파일로 실패와 성공을 모두 확인했다). 호스트와 `x86_64-apple-darwin` 크로스 빌드가 성공했다. README 의 curl 이 만드는 이름과 워크플로 아티팩트 이름이 네 조합 모두 같다. Linux 두 타깃은 rustc 는 통과하고 링크만 실패한다(macOS 에 링커가 없다). 그래서 이 두 타깃은 CI 에서만 빌드한다. README 의 `OWNER` 는 자리 표시 값이다. remote 를 연결하면 바로 이 단어 하나를 고쳐야 한다. 고치지 않으면 404 가 난다
- [x] **빈 디렉토리에서 `git init` → `kang init` → `kang build` 가 세 명령으로 통과한다** — TTHW 측정 기준. `kang init` 은 git 을 요구하지 않지만 `build` 는 요구한다(스펙 3절). 그래서 `git init` 이 반드시 들어간다
  - 릴리즈 바이너리로 직접 실행해 확인했다. `init` 은 네 파일을 만들고 exit 0 이다. `build` 는 **exit 0 이고 stdout, stderr 가 완전히 비어 있다.** `init` 이 만든 파일만으로 `list`, `keywords`, `refs`, `show`, `--help` 8개 명령이 모두 통과한다. 문서를 하나 더해서 `bless` 를 실행하고 다시 build 하는 과정까지 확인했다. git 없이 `init` 만 실행해도 exit 0 이고 `git init` 안내가 나온다(첫 실행에서 막히지 않는다). 다시 실행해도 결과가 바이트 단위로 같고, "건너뜁니다" 의 이유를 경우별로 따로 알려 준다
- [x] **`kang` 이 자기 저장소에서 동작한다** — 도그푸딩을 시작하는 조건. `plans/`, `docs/adr/`, `CONTEXT.md` 를 옮기는 일은 별도 플랜
  - **V0004 에서 해결했다.** `CONTEXT.kang`(keyword 16, topic 2)과 ADR 셋을 옮겼다. `kang build` 는 exit 0 이다.
    그 뒤 원본 `.md` 네 개가 사본으로 남아 있던 것도 지웠다. 이제 CI 가 `kang build`, `list`,
    `keywords`, `show` 를 실제 문서에 실행한다.
  - **목표에 못 미침.** 저장소에서 `kang build`, `kang list` 는 exit 0 이다. 그러나 `.kang` 문서는 Task 14 개발 중에 들어온 `docs/example.kang` 템플릿 하나뿐이다. 그래서 의미 있는 도그푸딩이 아니다. 루트에는 git 이 추적하지 않는 `kang init` 생성물 네 개(`.claude/`, `AGENTS.md`, `CLAUDE.md`, `docs/example.kang`)가 있다. 이 파일은 병합 diff 에 들어가지 않는다. V0004 우선순위 5 이다. 참조 병합 한계(`check::split_names`)를 고칠지 정하는 조건을 다시 측정할 방법은 문서를 `.kang` 으로 옮기는 이 작업뿐이다(지금의 "충돌 0건" 은 마크다운 문서 모음에서 측정한 값이다)

---

## 구현 착수 전 해결 필요 (BLOCKING)

2026-08-05 `/plan-eng-review` 와 독립 리뷰에서 나온 항목이다. 같은 날 `/grill-with-docs` 세션에서 대부분 해결했다.

### 해소됨 — 스펙 V0001 갱신 완료

| # | 항목 | 결론 |
|---|---|---|
| 1 | 셸 백틱 충돌 | **CLI 인자에서 백틱을 뺀다.** 본문 백틱은 그대로 둔다. `fix` 문자열은 인용을 붙여 출력한다. 스펙 6.0 |
| 2 | `iknow` 문법 + 순환 충돌 | **`iknow` 는 참조가 아니라 부인(disavowal)이다.** import 간선을 만들지 않으므로 순환이 아니다. 경로가 있는지와 서로 명시했는지는 검사하고, rev 핀은 가지지 않는다. 스펙 4.4 |
| 3 | rev 부트스트랩 데드락 | **처음에는 핀 없이 쓰고, `bless` 가 핀을 넣는다.** 그래서 임시 해시가 필요 없다. 스펙 4.8 |
| 4 | `bless` 줄 번호 주소 | **심볼 주소로 바꾸었다** — `kang bless <문서> --import <심볼>`. 스펙 6.2, [ADR-0003](../../docs/adr/0003-symbolic-addressing-not-line-numbers.md) |
| 5 | 프로젝트 루트 미정의 | **git 저장소 루트가 프로젝트 루트다.** 설정 파일을 두지 않는다. 스펙 3절 |
| 11 | 이름이 다른 개념 중복 | **결정적인 규칙으로는 해결되지 않는다.** grilling 에서 동의어(`also`) 를 도입했다가, 2차 독립 리뷰 뒤에 삭제했다. 동의어가 동작하려면 owner 가 다른 사람이 쓸 변형 이름을 미리 알아야 한다. 그런데 미리 알 수 있었다면 처음부터 이름이 두 개 생기지 않았다. 처음부터 kang 으로 설계하면 변형 이름이 생길 곳이 없다(모든 백틱이 심볼이다). 기존 문서를 옮길 때 만나는 변형 이름은 정본 이름으로 합친다. 스펙 4.3 |
| 12 | 더 단순한 대안 | **kang 이 필요하다.** 참조 문서 모음(ax-conta)은 이미 `Load when`, `Related`, Ubiquitous Language, living-document 다시 쓰기를 손으로 하고 있다. 그러나 이 작업을 강제하는 장치가 없다. 50줄짜리 해시 스크립트로는 미해결 참조 error, owner 유일성, 예외 커버리지를 얻을 수 없다 |

함께 정한 결정: 폐기한 용어에 묘비를 두지 않는다 ([ADR-0001](../../docs/adr/0001-no-tombstones-for-retired-terms.md)). 읽을 때는 평탄화하고 쓸 때는 import 한다 ([ADR-0002](../../docs/adr/0002-flatten-on-read-import-on-write.md)). keyword 는 도메인 특수 용어에만 쓴다. 이름 충돌은 계층 전체 경로로 판단한다.

### 알고 받아들이는 한계

| # | 항목 | 판단 |
|---|---|---|
| 6 | 실제로는 여러 핀을 한 번에 풀 수 있다 | `kang build` 출력을 그대로 `bless` 에 넘길 수 있으므로 번거로움이 거의 없다. 그리고 번거로움을 느낄 쪽이 LLM 이므로, 번거로움으로 막는 방법은 처음부터 동작하지 않는다. **시스템으로 막을 방법이 없으므로 막지 않는다** (ADR-0002) |
| 7 | 번거로움이 오히려 잘못된 행동을 부추긴다 | import 를 지우고 내용을 복사해 붙이는 쪽이 더 쉽다. 같은 이유로 시스템이 막을 수 없다. 작성 스킬이 먼저 `show` 를 실행하라고 지시해서 이 문제를 줄인다 |
| 10 | 모든 백틱을 심볼로 보는 규칙의 도입 비용 | `200원`, `null`, `POST /payments` 가 미해결 심볼 error 가 된다. **도그푸딩(kang 자기 문서를 `.kang` 으로 옮기기)으로 측정한 뒤 판단한다.** 마크다운 렌더링 호환 결정도 그때 함께 적는다 |

### 나머지도 해소됨

| # | 항목 | 결론 |
|---|---|---|
| 8 | `kang show` 깊이 무제한 | **v1 에서는 구현하지 않고, 형태만 확정한다.** 데이터 없이 임계값을 고르면 추측일 뿐이므로, 도그푸딩에서 측정한다. 만들 때는 읽을 때가 아니라 빌드할 때 구조를 검사하는 린트로 만든다. 예: "이 문서가 참조하는 정책이 너무 많다". 스펙 6.4 |
| 9 | `exception` 이 rev 강제의 구멍 | **exception import 도 rev 핀을 가진다.** 해시 입력은 그 예외를 선언한 topic 의 본문이다. 이름은 그대로 두고 내용을 바꿔도 커버 문서가 깨진다. 스펙 4.8 |
| 13 | `ancestors()` 의 v1 사용처 | **v2 로 미룬다.** 참조 전파는 코드 참조에만 쓰고, 코드 연동은 v2 이므로 v1 에는 호출하는 곳이 없다. 그래서 Task 5 범위를 줄였다 |

**구현을 막는 항목은 없다.**

### 옮길 대상

**kang 으로 처음 옮길 대상은 kang 자신의 문서다.** `plans/`, `docs/adr/`, `CONTEXT.md` 를 `.kang` 으로 옮긴다. ax-conta 는 설계를 검증할 때 참고한 문서 모음일 뿐이고, 옮길 대상이 아니다.

---

## NOT in scope

- **코드 연동 (`kang inspect`)** — v2. 애노테이션 명세와 언어별 추출기는 따로 설계해야 한다.
- **자연어 검색** — 확률적 기능을 쓰지 않는다는 원칙에 따라 영구히 제외한다. 다만 위 BLOCKING 11번 때문에 이 결정을 다시 검토해야 한다.
- **`--depth` 같은 출력 축소 옵션** — LLM 에게 덜 읽는 방법을 주지 않는다. BLOCKING 8번은 옵션이 아니라 error 로 처리하는 방향이다.
- **deprecated 정책 표현** — 히스토리는 git 이 맡는다.
- **에디터 지원 (LSP, syntax highlight)** — v1 이후. BLOCKING 10번의 결론에 따라 우선순위가 바뀔 수 있다.
- **crates.io 발행** — GitHub Releases 로 시작한다. 이름을 먼저 차지해야 할 필요가 생기면 그때 발행한다.

## What already exists

외부에서 가져올 수 있는 것은 다음과 같다. `sha2` 말고는 모두 일부러 쓰지 않는다.

| 후보 | 판단 |
|---|---|
| `chumsky` / `pest` / `nom` | 쓰지 않는다. kang 문법은 줄 단위이고 마크다운이 섞여 있다. 그래서 토큰 스트림에 맞춘 라이브러리와 잘 맞지 않는다. |
| `clap` | 쓰지 않는다. v1 의 플래그는 `bless --import` 하나뿐이라서 `match` 로 충분하다. 플래그가 늘어나면 바꾼다. |
| `serde_yaml` 계열 | 쓰지 않는다. 스키마가 고정이고, literal scalar 를 직접 제어해야 한다. 인용 규칙의 위험은 `scalar()` 테스트가 막는다. |
| `walkdir` | 쓰지 않는다. `std::fs::read_dir` 를 재귀로 쓰면 15줄이다. |
| `sha2` | **쓴다.** 표준 라이브러리에 SHA-256 이 없다. |

## Failure modes

| 코드패스 | 실패 시나리오 | 테스트 | 에러 처리 | 사용자에게 보이나 |
|---|---|---|---|---|
| `bless` 다중 위치 | 검증 중에 실패해서 파일이 반만 바뀜 | Task 11 | 모두 검증한 뒤 한 번에 쓴다 | 보임 |
| `bless` 주소 지정 | 문서를 고쳐서 줄 번호가 바뀜 | Task 11 | 심볼 주소이므로 영향 없음 (ADR-0003) | 해당 없음 |
| `bless` 실행 조건 | error 상태여서 `compile()` 이 막음 | Task 9, 11 | `parse_project()` 만 실행한다 | 보임 |
| `exception` 의미 변경 | 커버 문서가 깨지지 않음 | Task 8 | 선언 topic 의 본문 해시를 핀으로 쓴다 | 보임 |
| `kang show` 깊이 | 컨텍스트를 넘어서, 잘린 입력으로 틀린 답을 냄 | **없음** | **없음** | **안 보임 — v1 미구현, 스펙 6.4** |
| `load()` IO 실패 | 잘못된 UTF-8 파일 | Task 4 | 진단으로 바꾸고, 나머지는 계속 처리 | 보임 |
| 미해결 import | 대상 파일이나 심볼이 없음 | Task 6 | error | 보임 |
| 파서 실패 | 짝이 맞지 않는 백틱, 깨진 import | Task 2, 3 | 진단 | 보임 |
| `init` 환경 | git 저장소 아님 | Task 14 | git 없이 진행하고 `git init` 을 안내 | 보임 |
| `iknow` 로 복제를 허용받음 | 다른 사람의 정의를 복사하고 "다른 뜻" 이라고 선언 | **불가능** | **불가능** | **안 보임 — 받아들이는 한계, 스펙 4.4** |

**critical gap 2건** — `show` 깊이는 v1 에서 구현하지 않았고, 형태만 확정했다. `iknow` 로 정의를 복제하는 경우는 자연어로 판단해야 하므로 기계적으로 막을 수 없다. 스펙 4.4 가 이를 한계로 적어 두었다.

## Worktree parallelization

| 단계 | 모듈 | 의존 |
|---|---|---|
| Task 1 | `hash.rs` | — |
| Task 2, 3 | `ast.rs`, `parse.rs` | — |
| Task 4, 5 | `resolve.rs` | Task 2, 3 |
| Task 6, 7, 8 | `check.rs` | Task 4, 5 |
| Task 9 | `main.rs` | Task 6, 7, 8 |
| Task 10 | `yaml.rs`, `show.rs` | Task 4, 5 |
| Task 11 | `bless.rs` | Task 8 |
| Task 12 | `tests/` | 전부 (**Task 14 포함** — 통합 시나리오가 `kang init` 을 쓴다) |
| Task 13 | `.github/`, `README.md` | — |
| Task 14 | `init.rs`, `skill.md` | — |

```
Lane A: Task 1                          (독립, hash.rs)
Lane B: Task 2 → 3 → 4 → 5              (순차, 파싱에서 해석까지)
Lane C: Task 13 + Task 14               (독립, CI·init·스킬)
        ↓ B 완료 후
Lane D: Task 6 → 7 → 8 → 11             (순차, check.rs 공유 후 bless)
Lane E: Task 10                         (D 와 병렬, yaml/show)
        ↓ D, E 완료 후
Lane F: Task 9 → 12   (Lane C 의 Task 14 도 끝나야 시작 가능)
```

**실행 순서:** A, B, C 를 병렬 워크트리로 시작한다. B 가 끝나면 D 와 E 를 병렬로 실행한다. **D, E 와 C(Task 14) 가 모두 끝나면** F 를 실행한다. Task 12 의 통합 시나리오가 `kang init` 을 호출하므로 C 가 F 보다 먼저 끝나야 한다.

**충돌 주의:** Lane D 는 `check.rs` 만, E 는 `yaml.rs`/`show.rs` 만 고치므로 서로 충돌하지 않는다. 다만 둘 다 Task 4 의 `SymbolTable` 시그니처를 쓰므로, B 가 끝나기 전에 D, E 를 시작하면 안 된다.

## Implementation Tasks

`/plan-eng-review` 와 독립 리뷰에서 찾은 문제에서 나온 항목이다. **2026-08-05 `/grill-with-docs` 세션에서 모두 처리했다.** 스펙 V0001 도 고쳤다.

- [x] **T1** — CLI 인자 문법 확정. 백틱을 빼고, `fix` 문자열은 인용해서 출력한다. 스펙 6.0
- [x] **T2** — `iknow` 문법 정의와 순환 충돌 해결. 부인(disavowal)이므로 import 간선이 아니다. 스펙 4.4
- [x] **T3** — rev 핀 부트스트랩. 처음에는 핀 없이 쓰고, `bless` 가 핀을 넣는다. 스펙 4.8
- [x] **T4** — `bless` 주소를 심볼로 바꾸었다. 스펙 6.2, ADR-0003
- [x] **T5** — 프로젝트 루트 = git 저장소 루트. 스펙 3절
- [x] **T6** — exception import 도 rev 핀을 가진다. 해시 입력은 선언 topic 의 본문이다. 스펙 4.8
- [x] **T7** — show 깊이. v1 에서는 구현하지 않고 형태만 정했다. 빌드할 때 구조를 검사하는 린트로 만든다. 스펙 6.4
- [x] **T8** — 모든 백틱을 심볼로 보는 규칙의 도입 비용. **도그푸딩으로 측정한 뒤 판단**하기로 했다. 마크다운 렌더링 호환 결정도 그때 함께 한다
- [x] **T9** — `ancestors()` 를 v2 로 미룬다. Task 5 범위를 줄였다

### 다음 작업

- [x] **T10 (P1)** — 고친 스펙에 맞춰 Task 1~13 의 시그니처와 테스트 시나리오를 다시 점검
  - **다른 방법으로 해결했다.** 태스크를 스펙에 맞추지 않고, 스펙을 구현에 맞춰 고쳤다
    (`1c4a9a5`, V0001 5.1.1, 6절). 구현은 이미 직접 실행해 검증한 상태였다. 그래서 구현 쪽이 사실에
    더 가깝다. 그 뒤 `K116`, `K117`, `K118` 도 스펙에 함께 적었다.
  - 특히 Task 3(`iknow` 파싱), Task 8(exception rev), Task 11(`bless` 심볼 주소), Task 9(CLI 인자)
  - 검증: 스펙 4, 5, 6 절의 모든 규칙이 어느 태스크에 해당하는지 비교
- [x] **T11 (P2)** — 도그푸딩 계획. `plans/`, `docs/adr/`, `CONTEXT.md` 를 `.kang` 으로 옮기는 순서와 성공 기준
  - **V0004 가 그 계획이다.** `CONTEXT` 를 먼저, ADR 을 나중에 옮겼다(용어집이 기준이고, ADR 이 용어집을 쓰는
    한 방향 관계). `plans/` 는 `- [ ]` 가 topic 밖 내용이어서 `K112` 가 나므로 옮기지 않기로 했다.
  - 여기서 T8(백틱 비용)과 T7(깊이 임계값)의 측정값이 나온다

## DX 리뷰 결과 (2026-08-05)

주 개발자는 **다른 프로젝트에서 일하는 LLM 에이전트**다. 사람은 결과를 검토할 뿐이다.

### 핵심 발견

**kang 의 기본 원칙은 DX 로만 지킬 수 있다.** "LLM 은 원본을 보지 않는다" 는 강제할 수 없다. 에이전트는 `.kang` 파일을 그냥 읽을 수 있기 때문이다. 그러므로 원칙을 지키게 하는 방법은 `kang show` 를 `cat` 보다 쉽게 만드는 것 하나뿐이다. 그런데 `cat` 으로 읽은 `.kang` 은 import 로 다른 곳을 가리키므로 마크다운보다 읽기 어렵다. 도구를 들여왔는데 오히려 더 나빠지는 경우가 있었다. Task 14(`kang init` + 스킬)가 이 구멍을 막는다.

### 차원별 점수

| 차원 | 이전 | 현재 | 남은 격차 |
|---|---|---|---|
| Getting Started (TTHW) | 0/10 | 8/10 | 설치한 뒤 `init` → `build` 두 명령. curl 설치가 실제로 동작하는지는 확인하지 않음 |
| 진단 품질 | 3/10 | 9/10 | 예시 3건 확정. 나머지 규칙이 같은 형태를 따르는지는 구현할 때 확인 |
| 발견(discovery) | 0/10 | 8/10 | 스킬을 저장소에 커밋한다. 스킬 내용이 실제로 효과가 있는지는 도그푸딩에서 측정 |
| CLI 인체공학 | 5/10 | 8/10 | 백틱 제거, 종료 코드 4종, `--help` 규칙 |
| 점진적 공개 | 6/10 | 6/10 | 문서 하나를 쓰려면 frontmatter, keyword, import, rev 를 한 번에 알아야 한다 |
| 탈출구 | 5/10 | 5/10 | 일부러 최소로 두었다. 여러 핀을 한 번에 푸는 방법도, 깊이 제한도 없다 |
| 업그레이드 | 3/10 | 3/10 | **미해결** — 언어 자체의 버전 정책이 없다 |
| 접근성 | 6/10 | 6/10 | 바이너리 4종. Windows 는 없다. 사람이 문서를 쓰는 경험은 측정하지 않음(T8) |

### 반영된 것

- **Task 14 추가** — `kang init` 이 `.claude/skills/kang/SKILL.md`, `AGENTS.md`, `CLAUDE.md`, 첫 문서를 만든다. 저장소에 커밋하는 프로젝트 범위 파일이다
- **스펙 5.1.1 추가** — 진단 출력 예시 3건(미해결 심볼, iknow 누락, rev 불일치)의 형식을 확정
- **스펙 6절** — 종료 코드 0/1/2/3 규칙. `kang inspect` 는 v2 안내와 함께 3 을 돌려준다
- **스펙 6.1** — `kang init` 절과 스킬 내용(케이스별 기대 동작 5종)
- **Task 9** — `--help` 가 명령, 인자 형식, 종료 코드를 모두 보여 준다

### 남은 격차 (v1 이후)

- **언어 버전 정책이 없다.** kang 문법이 바뀌면 기존 `.kang` 문서가 어떻게 되는지 정하지 않았다. 도그푸딩 중에는 문서 작성자와 도구 작성자가 같아서 이 문제가 보이지 않는다. 외부 프로젝트에 도입하기 직전에 문제가 된다.
- **점진적 공개가 없다.** 첫 문서를 쓰려면 네 가지 문법을 한 번에 알아야 한다. `kang init` 의 템플릿이 이 부담을 얼마나 줄이는지는 도그푸딩에서 측정한다.
- **Windows 를 지원하지 않는다.** 릴리즈 타깃 4종은 macOS 와 Linux 뿐이다.

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 0 | — | — |
| Codex Review | `/codex review` | Independent 2nd opinion | 0 | — | Codex CLI 가 ChatGPT 계정 모델 제약으로 실행 불가 |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 2 | CLEAR | 1차 19건, 2차 6건 — 전부 해소 |
| Design Review | `/plan-design-review` | UI/UX gaps | 0 | — | 해당 없음 (CLI) |
| DX Review | `/plan-devex-review` | Developer experience gaps | 1 | CLEAR | 8차원 평가, 5건 반영, 3건 v1 이후 |

**OUTSIDE VOICE:** 2회 실행했다. Codex 는 계정 제약으로 두 번 모두 실패해서, 대신 Claude 서브에이전트를 썼다. 1차 13건(구현을 막는 항목 5건 포함)과 2차 13건을 모두 해결했다. 2차에서 찾은 "`bless` 를 실행할 수 없음" 이 구현을 막는 유일한 항목이었다.

**GRILLING (2026-08-05):** `/grill-with-docs` 로 전략의 전제와 스펙의 모순을 처리했다. 참조 문서 모음 `~/Project/ax-conta` 를 근거로 검증했다. 구현을 막는 항목 5건과 설계 결함 3건을 해결했다. 2건은 한계로 받아들였다. ADR 3건을 새로 만들었다.

**DX REVIEW (2026-08-05):** 주 개발자를 LLM 에이전트로 확정했다. 기본 원칙은 DX 로만 지킬 수 있다는 결론에 따라 Task 14(`kang init` + 스킬), 스펙 5.1.1(진단 예시), 종료 코드 규칙을 더했다.

**PONYTAIL (2026-08-05):** 7건 중 6건을 적용했다. `ImportGraph` 구조체, `canonical_of_synonym`, `assets/` 디렉토리, 중복 파일을 없앴다.

**2차 ENG REVIEW 에서 되돌린 결정:** grilling 에서 도입한 동의어(`also`) 를 삭제했다. 동의어가 동작하려면 owner 가 다른 사람이 쓸 변형 이름을 미리 알아야 한다. 그런데 미리 알 수 있었다면 처음부터 이름이 두 개 생기지 않았을 것이다. 처음부터 kang 으로 설계하면 변형 이름이 생길 곳이 없다.

**VERDICT:** ENG + DX CLEARED — 구현을 시작할 수 있다. 아직 정하지 않은 것은 모두 v1 이후 항목이다.

**기록 실패:** `gstack-review-log` 가 최소한의 JSON 도 거부했다. `bun` 이 설치되어 있지 않아서 bun 기반 gstack 바이너리 일부가 동작하지 않는다. 이 리포트가 유일한 기록이다.

**UNRESOLVED DECISIONS:**
- 언어 자체의 버전 정책 — kang 문법이 바뀔 때 기존 `.kang` 문서를 어떻게 할지 정하지 않음
- Windows 지원 여부 — 릴리즈 타깃 4종이 macOS 와 Linux 뿐
- `kang show` 깊이 임계값 — 형태는 빌드할 때 구조를 검사하는 린트로 확정, 숫자는 도그푸딩에서 측정한 뒤 정함
