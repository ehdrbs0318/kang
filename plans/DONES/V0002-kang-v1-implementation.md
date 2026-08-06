# V0002 — kang v1 구현 플랜

> **에이전트 실행용:** 이 플랜은 `superpowers:subagent-driven-development` 또는 `superpowers:executing-plans` 로 태스크 단위 실행한다. 각 단계는 체크박스로 추적한다.

**목표:** `V0001-kang-language-design.md` 의 v1 명세대로 kang 컴파일러와 CLI를 구현한다.

**아키텍처:** 단일 Rust 바이너리. 프로젝트 전체를 읽어 `Document` AST로 파싱하고, 심볼 테이블과 import 그래프를 만든 뒤, 진단 규칙을 돌린다. 진단에 error가 하나라도 있으면 어떤 조회 명령도 출력하지 않는다.

**기술 스택:** Rust 1.97 / 외부 의존성은 `sha2` 하나

## 전역 제약

- 스펙 원본은 `plans/DONES/V0001-kang-language-design.md` (완료 시 이동). 모든 규칙의 근거는 여기다.
- **의존성 추가 금지.** `sha2` 외에 크레이트를 넣지 않는다. 인자 파싱과 YAML 출력은 직접 쓴다 — v1의 플래그는 `bless --import` 하나뿐이고 YAML 스키마가 고정이다.
- `kang build` 기본 심각도는 **error**. error 발생 시 종료 코드 1, 조회 명령은 아무것도 출력하지 않는다.
- 주석은 한글 TSDoc 대응 규격(rustdoc `///`)으로 작성한다. 함수·구조체·enum 전부.
- 로깅 규칙 적용 대상 아님 — CLI 단발 실행이라 로그 레벨 시스템을 두지 않는다. 진단 출력이 그 역할을 한다.
- 모든 진단 메시지는 **수정 위치와 방법**을 포함한다. LLM이 스스로 고칠 수 있어야 한다.
- 테스트는 `cargo test` 하나로 전부 돈다.
- **`FixKind::Shell` 의 `action` 은 경로·이름을 반드시 인용한다.** 스펙 4.2·6.0·6.1 이 세 곳에서 못박은 조항이며 "인용 여부를 스스로 판단하게 두면 틀린다" 가 근거다. `format!("... ls -l {}", p)` 처럼 맨 보간을 하면 `/Users/x/My Project` 에서 셸이 두 인자로 쪼개 **원인 진단 대신 새로운 잘못된 사실**을 준다. 작은따옴표로 감싸되 값 안의 `'` 를 `'\''` 로 치환한다.

**Task 4 가 `resolve::셸_인용(value: &str) -> String` 을 `pub(crate)` 로 만들어 뒀다. 셸 fix 를 새로 만드는 태스크는 반드시 이것을 거친다.** 매개변수가 `&Path` 가 아니라 `&str` 인 이유는 경로만이 아니라 심볼 이름도 인용 대상이기 때문이다 (`kang bless docs/B --import 'docs/A.결제'`, 스펙 6.0).

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

책임 분리 기준: `parse`는 파일 하나만 안다. `resolve`는 프로젝트 전체를 안다. `check`는 규칙만 안다. `show`/`yaml`/`bless`는 출력과 수정만 한다.

**경로 포맷팅은 한 곳에만 있다.** `list`·`keywords`·`refs`·`show`·진단이 전부 "전체 경로 문자열"을 만드는데, 각자 만들면 다섯 곳이 조금씩 어긋난다. `ast.rs` 에 `impl Display for DocPath` 와 `SymbolRef` 의 표시 함수를 두고 전부 그것만 쓴다. CLI 인자 파싱(`ImportAddress::parse`)도 같은 문법의 역방향이므로 같은 모듈에 둔다.

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

- [x] **Step 1: `cargo init --name kang` 실행, `Cargo.toml`에 `sha2` 추가**
- [x] **Step 2: 실패하는 테스트 작성** — 시나리오 3개
  - `줄_끝_공백은_해시를_바꾸지_않는다`
  - `연속_빈_줄은_하나로_축약된다`
  - `본문이_다르면_해시가_다르다`
- [x] **Step 3: `cargo test` — 컴파일 실패 확인**
- [x] **Step 4: `normalize`, `rev` 구현**
  - 줄 단위로 `trim_end`, 빈 줄 2개 이상은 1개로, 전체 `trim`
  - `sha2::Sha256` 결과를 hex로 만들고 앞 6자
- [x] **Step 5: `cargo test` 통과 확인**
- [x] **Step 6: 커밋** — `feat: rev 해시 산출과 정규화 규칙`
  - c5eb31d·14d6b81 / 테스트 3

**수행 내역** — c5eb31d·14d6b81 / 테스트 3. normalize/rev 확정. lib 크레이트 전환으로 tests 가 내부 접근

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

이 태스크에서는 `imports`, `exceptions`, `covers` 를 빈 벡터로 두고 Task 3에서 채운다.

**구현 요점**
- frontmatter는 `---` 로 감싼 블록. `description` 이 없으면 error.
- `keyword` 줄: 이름(계층 `.` 구분) → `:` → 한 줄 정의 → 선택적 `` #`상세 topic` ``. 동의어 문법은 없다.
- `##` 로 시작하면 새 topic. 다음 `##` 직전까지가 body.
- 백틱 스캔: `` \` `` 는 리터럴, ` ``` ` 펜스 내부는 건너뛴다. 그 외 백틱 쌍은 심볼 참조로 `refs` 에 기록.
- 줄 번호를 모든 노드에 기록한다. 진단 품질이 여기 달려 있다.

- [x] **Step 1: 실패하는 테스트 작성** — 시나리오 10개
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
- [x] **Step 2: `cargo test` — 실패 확인**
- [x] **Step 3: `ast.rs` 타입 정의 (`Diagnostic` 포함)**
- [x] **Step 4: `parse.rs` 구현 — frontmatter, keyword, topic, 백틱 스캔**
- [x] **Step 5: `cargo test` 통과 확인**
- [x] **Step 6: 커밋** — `feat: frontmatter/keyword/topic 파싱`
  - c877e2e~1f40f96 (13커밋) / 테스트 29

**수행 내역** — c877e2e~1f40f96 (13커밋) / 테스트 29. K101~K105. **리뷰가 컨트롤러의 `\\` 이스케이프 발명을 반박해 되돌림**(스펙 4.2 는 `` \` `` 하나만 정의)

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
  - **세 종류 모두 rev 핀을 가질 수 있다.** exception 도 예외가 아니다 (스펙 4.7).
- `` exception `이름` [pending] `` 과 `` cover `이름` `` 은 topic 본문 안에서 인식한다.
- `// iknow <대상>, <대상>, …` 은 keyword / topic 헤딩 / exception 줄 뒤에 붙는다. 쉼표로 여러 대상을 나열한다.
  - **`iknow` 는 import 가 아니다.** 파서는 대상을 `Vec<SymbolRef>` 로 담기만 하고, `imports` 에 넣지 않는다. 그래프 간선이 되지 않는 것이 스펙 4.4 의 핵심이다.
- `// uncoded` 는 topic 헤딩 줄 뒤에만 붙는다.
- 이 modifier들은 topic `body` 에서 제외한다 — rev 해시에 들어가면 안 된다.

**컨트롤러 이월 (Task 2 리뷰 2회에서 나옴 — 반드시 처리)**

- **`K105` 의 판정 범위를 좁혀야 한다.** Task 2 는 topic 헤딩 줄에 백틱이 하나라도 있으면 `K105` error 를 낸다 (근거: 스펙 6.0 이 CLI 인자에서 백틱을 금지하므로 헤딩에 백틱이 있으면 그 topic 은 주소를 댈 수 없다). 그런데 **스펙 4.4 는 `// iknow` 가 topic 헤딩에 붙는다고 하고 그 대상은 백틱을 포함한다** — `` ## 결제의 방법 // iknow `docs`/`B`.`결제의 방법` `` 은 합법 문서인데 지금 `K105` 로 거부된다.

  따라서 Task 3 은 **헤딩에서 modifier(`// iknow …`, `// uncoded`)를 먼저 잘라낸 뒤 남은 헤딩 텍스트에만 `K105` 를 적용**하도록 고쳐야 한다. 순서를 반대로 하면 합법 문서가 거부된다.

- **`Topic.name` 에서도 modifier 를 제거해야 한다.** 지금은 헤딩 원문 전체가 `name` 이 되므로, modifier 를 안 자르면 `` 조직의 문서 검토 절차 // uncoded `` 같은 조회 불가 이름이 **에러 없이** 만들어진다. `K105` 만 조용해지고 이름이 깨지는 것이 최악이다 — 둘을 함께 고친다.

- **topic 밖 줄의 백틱 짝 검사.** Task 2 의 `parse.rs` 는 첫 `##` 이전 줄에서 `topics.last_mut()` 가 `None` 이라 조기 `continue` 하므로 백틱 짝 검사(`K104`)를 받지 않는다. Task 3 이 `import` 줄을 파싱하며 이 구간을 다루게 되므로, **import 줄과 그 밖의 topic 외 줄에 백틱 짝 검사를 적용할지 여기서 결정하고 명시한다.** import 줄은 반드시 검사해야 한다 — 심볼 주소가 백틱으로 쓰인다.

- **타입 재정의 금지.** Task 2 가 확정한 `ast.rs` 의 14개 타입은 필드 이름·타입·순서가 고정이다. Task 3 은 빈 필드를 **채우기만** 한다. 필드를 늘려야 할 이유가 생기면 구현하지 말고 컨트롤러에 보고한다.

- [x] **Step 1: 실패하는 테스트 작성** — 시나리오 11개
  - `keyword_import_를_읽는다`
  - `topic_import_를_읽는다`
  - `exception_import_를_읽는다`
  - `as_alias_를_읽는다`
  - `rev_핀을_읽는다`
  - `exception_과_pending_을_읽는다`
  - `cover_를_읽는다`
  - `iknow_대상_목록을_쉼표로_읽는다`
  - `iknow_대상은_imports_에_들어가지_않는다`
  - `exception_import_도_rev_핀을_읽는다`
  - `uncoded_modifier_는_body_에서_제외된다`
  - `구분자가_없는_import_는_에러다`
  - `알_수_없는_modifier_는_에러다`
- [x] **Step 2: `cargo test` — 실패 확인**
- [x] **Step 3: import 파싱 구현 (경로 구분자 → kind 판정)**
- [x] **Step 4: exception / cover / modifier 파싱 구현**
- [x] **Step 5: `cargo test` 통과 확인**
- [x] **Step 6: 커밋** — `feat: import/exception/cover/modifier 파싱`
  - c99bf18~40eda89 (10커밋) / 테스트 80

**수행 내역** — c99bf18~40eda89 (10커밋) / 테스트 80. K106~K112. K105 범위를 topic 헤딩으로 좁힘, 스펙 3절에 「topic 밖 금지」 추가

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
- 디렉토리 순회는 `std::fs::read_dir` 재귀로 직접 쓴다. `.kang` 확장자만 읽는다.
- `hash_source` 는 세 종류 모두에 값이 있다 — keyword 는 한 줄 정의, topic 은 body, **exception 은 그 예외를 선언한 topic 의 body** 다 (스펙 4.8). Task 8이 이걸 쓴다.
- `by_name` 은 `HashMap<String, Vec<SymbolId>>` 로 미리 색인해 둔다. 매번 선형 스캔하면 iknow 검사가 O(n²)가 된다.
- 읽기 실패와 잘못된 UTF-8 은 그 파일에 대한 진단으로 바꾸고 나머지 파일 처리를 계속한다. 한 파일 때문에 전체가 죽으면 안 된다.

- [x] **Step 1: 실패하는 테스트 작성** — 시나리오 9개
  - `git_루트를_프로젝트_루트로_찾는다`
  - `하위_디렉토리에서_실행해도_DocPath_가_같다`
  - `git_저장소가_아니면_진단을_낸다`
  - `하위_디렉토리의_kang_파일을_전부_읽는다`
  - `kang_이_아닌_파일은_무시한다`
  - `자기_파일의_심볼을_스코프에서_찾는다`
  - `import_한_alias_를_스코프에서_찾는다`
  - `같은_이름_심볼을_by_name_으로_모은다`
- [x] **Step 2: `cargo test` — 실패 확인**
- [x] **Step 3: `find_root` 와 `load` 구현 (git 루트 탐색 + 재귀 순회 + 파싱)**
- [x] **Step 4: `SymbolTable` 구현**
- [x] **Step 5: `cargo test` 통과 확인**
- [x] **Step 6: 커밋** — `feat: 프로젝트 로드와 전역 심볼 테이블`
  - f230932~b06d6ca (4커밋) / 테스트 106

**수행 내역** — f230932~b06d6ca (4커밋) / 테스트 106. find_root/load/SymbolTable, K050~K052. `셸_인용` 을 전역 제약으로

---

## Task 5: 순환 검출

**파일**
- 생성: `src/check.rs`
- 테스트: `tests/check.rs`

**인터페이스**
- 소비: `Project`
- 산출:

```rust
/// 파일 단위 import 관계를 DFS 로 훑어 순환을 검출한다.
/// 그래프 값을 남기지 않는다 — v1 에 질의할 소비자가 없다.
pub fn check_cycles(project: &Project) -> Vec<Diagnostic>;
```

**구현 요점**
- **노드는 파일이다.** 스펙 5.1 의 순환 규칙이 파일 단위이기 때문이다. 파일 그래프가 DAG 면 topic 그래프도 DAG 다 — topic 간선 T→U 는 반드시 file(T)→file(U) 를 동반하므로 파일 단위 금지가 더 강하다.
- **`iknow` 는 간선이 아니다.** 상호 명시가 순환으로 잡히면 안 된다 (스펙 4.4). 간선은 `imports` 에서만 만든다.
- 순환 검출은 DFS + 방문 색칠. 순환 발견 시 스택을 그대로 체인으로 출력하고 "공통 개념을 상위 파일로 추출하라"를 `fix` 에 담는다.
- 그래프 값을 구조체로 남기지 않는다. `ancestors()` 를 v2 로 내린 뒤 질의 메서드가 없어져 `check.rs` 의 함수 하나로 충분하다. `ancestors()` 와 참조 전파는 **v2**다. 참조 전파는 코드가 topic 을 참조할 때 정의되는 규칙이고 코드 연동이 v2 이므로 v1 에 호출자가 없다. `show` 의 재귀 임베드는 `Document.imports` 를 직접 순회한다.

- [x] **Step 1: 실패하는 테스트 작성** — 시나리오 4개
  - `직접_순환을_검출한다`
  - `3단계_순환의_체인_전체를_출력한다`
  - `iknow_상호_명시는_순환이_아니다`
  - `자기_파일_import_는_순환이다`
- [x] **Step 2: `cargo test` — 실패 확인**
- [x] **Step 3: 파일 그래프 구성과 DFS 순환 검출 구현**
- [x] **Step 4: `cargo test` 통과 확인**
- [x] **Step 5: 커밋** — `feat: import 그래프와 순환 검출`
  - 76673a0~d93dd26 (4커밋) / 테스트 117

**수행 내역** — 76673a0~d93dd26 (4커밋) / 테스트 117. K040 파일 단위 순환. `compile()` 배선표를 Task 9 로 이월

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
| **import 대상 파일이나 심볼이 존재하지 않음** | error |
| import 했으나 어떤 topic 에서도 미사용 | error |
| 한 심볼에 두 개 이상 alias | error |
| 서로 다른 파일이 같은 이름 심볼 선언, iknow 없음 | error |
| iknow 가 관련 파일 전체를 상호 명시하지 않음 | error |
| **iknow 대상 파일이나 심볼이 존재하지 않음** | error |

- **이름 충돌 판정은 계층 전체 경로 기준이다.** `결제`.`상태` 와 `구독`.`상태` 는 다른 이름이므로 충돌이 아니다 (스펙 4.3).
- iknow 검사는 상호성까지 본다. N개 파일이 같은 이름을 선언하면 각 파일이 나머지 N-1개를 전부 명시해야 한다. 누락된 파일 경로를 `fixes` 에 나열한다.
- **iknow 는 import 가 아니므로 미사용 검사 대상이 아니다.** 경로 실재와 상호성만 본다 (스펙 4.4).
- 사용 여부는 topic 별로 추적한다. 파일 전체에서 한 번이라도 쓰였으면 통과.

**컨트롤러 이월 (Task 4 리뷰에서 나옴 — 반드시 처리)**

- **`scope()` 의 키는 계층 전체 경로인데 본문 `refs` 는 백틱 쌍마다 조각이다.** `Symbol.name` 과 `scope()` 키는 `"결제수단.카드"` 처럼 `.` 로 이은 전체 이름인데, 파서는 본문의 `` `결제수단`.`카드` `` 를 `("결제수단", n)` 과 `("카드", n)` **두 항목**으로 `refs` 에 넣는다.

  **`refs` 각 항목을 `scope()` 에서 그대로 조회하면 합법 문서에 미해결 심볼 error 가 난다.** 조회 전에 인접 조각을 합쳐야 한다 — `Topic.body` 가 헤딩 포함 원문을, `Keyword.definition` 이 원문 정의를 그대로 보관하므로 인접성 복원 경로가 열려 있다. resolve 쪽 선택(전체 경로 키)은 스펙 4.3·5.1 에 맞으므로 그쪽을 바꾸지 마라.

- **`SymbolId` 에는 kind·name·선언 줄 공개 접근자가 없다.** `by_name`/`owner` 로 파일을 좁힌 뒤 `Project.docs` 를 다시 훑어 `K012` 출력의 줄 번호를 만든다. 착수 시 이 왕복이 실제로 성립하는지 먼저 확인하고, 안 되면 구현하지 말고 컨트롤러에 보고하라.

**알려진 한계 — 파서 지원 없이는 못 고친다 (Task 6 재검증에서 확정)**

`Topic.refs` 와 `Keyword.refs` 는 `(이름, 줄)` 만 담고 **원문에서 조각들이 `.` 으로 이어져 있었는지를 담지 않는다.** Task 6 의 참조 해석은 그래서 스코프만 보고 조각을 합치며, 전부 해석되는 분할이 여럿이면 왼쪽 최장을 택한다.

그 결과 **합법 문서를 거부하는 경로가 하나 남는다.** 본문이 `` `A` 와 `B` `` 처럼 두 이름을 따로 언급했는데 `A.B` 도 스코프에 있으면 하나로 합쳐지고, 뒤 조각을 단독으로 import 한 줄이 **미사용(`K003`)으로 오인된다.** 놓치는 진단이 아니라 실제 거부다.

**근본 해결은 파서가 조각의 원문 인접성을 `refs` 에 싣는 것이다** — `Vec<(String, usize)>` 를 인접 그룹을 표현할 수 있는 형태로 바꾸는 `ast.rs`·`parse.rs` 변경이고, 그러면 Task 6 의 백트래킹 자체가 불필요해진다. **v1 범위에서는 감수하고, Task 12 통합 검증에서 실제 코퍼스 빈도를 측정한 뒤 승급 여부를 정한다.**

- **문서 간 이름 충돌은 Task 4 가 진단하지 않는다.** `SymbolTable::build` 는 한 문서 안의 로컬 이름 중복(`K052`)만 낸다. 다른 파일의 같은 이름은 `by_name` 으로 사실만 노출되며, "iknow 없음" 조건을 붙여 진단하는 것이 이 태스크의 몫이다 (`K010`-`K019` 대역).

**진단 출력 포맷** — **스펙 5.1.1 에 완성된 예시 3건이 있다.** 그 **형식**을 따른다. 즉흥으로 정하지 않는다.

테스트는 문자 단위 일치가 아니라 **구조 일치**를 본다. 예시에 박힌 경로와 줄 번호까지 맞추려면 fixture 를 역설계해야 하고 스펙 오타 수정마다 테스트가 깨진다. 검사할 것은 `code` 값, `locations` 개수와 각 `note` 유무, `fixes` 의 종류·순서, 셸 명령의 인용 여부다.

- 모든 진단은 `code`(`K001` 형식), `message`, `locations`(최소 1개), `fixes` 를 갖는다.
- `locations` 각 항목은 문서 경로·줄 번호·`note`(그 위치가 왜 관련되는지)를 갖는다. 순환 체인과 iknow 누락이 다중 위치의 대표 사례다.
- `fixes` 각 항목은 문서 경로와 **적용할 행동**을 갖는다. 줄 번호를 수정 좌표로 쓰지 않는다 (ADR-0003).
- 셸 명령을 담는 `fix` 는 **인용을 붙여 출력**한다. 에이전트가 복사해 그대로 실행할 수 있어야 한다.

- [x] **Step 1: 실패하는 테스트 작성** — 시나리오 13개
  - `미선언_백틱_심볼은_에러다`
  - `없는_파일을_import_하면_에러다`
  - `없는_심볼을_import_하면_에러다`
  - `사용하지_않는_import_는_에러다`
  - `한_심볼에_두_alias_는_에러다`
  - `이름_충돌에_iknow_가_없으면_에러다`
  - `iknow_가_한쪽에만_있으면_에러다`
  - `3개_파일_충돌은_각자_나머지_2개를_명시해야_한다`
  - `iknow_대상이_없으면_에러다`
  - `계층이_다르면_같은_말단_이름도_충돌이_아니다`
  - `진단이_관련_위치를_전부_담는다`
  - `셸_명령_fix_는_인용되어_출력된다`
  - `미해결_심볼_진단의_구조가_스펙_5_1_1_과_일치한다`
  - `iknow_누락_진단의_구조가_스펙_5_1_1_과_일치한다`
  - `rev_불일치_진단의_구조가_스펙_5_1_1_과_일치한다`
  - `edit_fix_는_문서_문법으로_shell_fix_는_CLI_문법으로_출력된다`
  - `fixes_는_적용_순서대로_나온다`
- [x] **Step 2: `cargo test` — 실패 확인**
- [x] **Step 3: `report` 구현** — 스펙 5.1.1 의 예시 3건과 구조가 일치해야 한다
- [x] **Step 4: `check_symbols` 의 8개 규칙 구현**
- [x] **Step 5: `cargo test` 통과 확인**
- [x] **Step 6: 커밋** — `feat: 심볼 진단 규칙`
  - 865fa42~1fd97a6 (17커밋) / 테스트 164

**수행 내역** — 865fa42~1fd97a6 (17커밋) / 테스트 164. K001~K012·K019. **합법 입력 거부를 다섯 번 잡음** — 계층 참조를 DP 분할로, 상위 import 를 하위 선언이 쓰는 것으로 인정

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

**컨트롤러 이월 (Task 6 에서 나옴 — 반드시 처리)**

**`cover` 대상이 존재하지 않으면 error 다.** Task 6 은 `Topic.covers` 를 **사용 여부 판정에만** 썼고 대상의 실재는 검사하지 않는다 — 그 층이 침묵하기로 한 것이므로 **여기서 진단하지 않으면 아무도 안 한다.**

`` cover `없는 예외` `` 처럼 어떤 topic 도 선언하지 않은 이름을 cover 하면, 진리표는 그것을 그냥 매칭 실패로 흘려보내고 문서는 통과한다. kang 이 막으려는 dangling 참조가 정확히 이 모양이다.

당신은 진리표를 위해 exception↔cover 매핑을 이미 만든다. 거기에 "매칭되지 않은 cover" 분기를 더하는 것이 Task 6 이 그 매핑을 다시 만드는 것보다 짧다.

- [x] **Step 1: 실패하는 테스트 작성** — 시나리오 5개
  - `커버되지_않은_exception_은_에러다`
  - `커버된_exception_은_통과한다`
  - `pending_이고_커버가_없으면_warn_이다`
  - `pending_인데_커버가_있으면_에러다`
  - `한_exception_을_둘이_커버하면_에러다`
- [x] **Step 2: `cargo test` — 실패 확인**
- [x] **Step 3: `check_exceptions` 구현**
- [x] **Step 4: `cargo test` 통과 확인**
- [x] **Step 5: 커밋** — `feat: exception 상태 기계 검증`
  - 69b0139~1f82d2b (4커밋) / 테스트 185

**수행 내역** — 69b0139~1f82d2b (4커밋) / 테스트 185. K030~K034. **fix 가 새 진단을 낳지 않는 것을 처음으로 증명**(6검사 테스트)

---

## Task 8: 진단 — rev 핀

**파일**
- 수정: `src/check.rs`
- 테스트: `tests/check.rs`

**인터페이스**
- 소비: `Project`, `SymbolTable`, `hash::rev`
- 산출: `check::check_revs(project, table) -> Vec<Diagnostic>`

```rust
/// 스펙 4.8 의 rev 핀을 검사한다.
pub fn check_revs(project: &Project, table: &SymbolTable) -> Vec<Diagnostic>;
```

**검사 항목**

| 상황 | 심각도 |
|---|---|
| import 에 rev 핀 없음 (keyword·topic·exception 세 종류 모두) | error |
| rev 핀이 대상의 현재 해시와 불일치 | error |

해시 입력은 `SymbolTable::hash_source` 가 준다 — keyword 는 한 줄 정의, topic 은 body, **exception 은 그 예외를 선언한 topic 의 body** 다.

- 불일치·부재 진단의 `fix` 는 `kang bless <문서> --import '<심볼>'` 형태를 담는다. Task 11의 `bless` 가 그대로 받는 형식이며, 셸 인용을 붙여 출력한다.
- **줄 번호를 쓰지 않는다** (ADR-0003). 문서를 고친 뒤 bless 하는 것이 정상 워크플로이므로 줄이 밀린다.
- **diff 를 출력하지 않는다.** kang 은 이전 본문을 저장하지 않는다. 무엇이 바뀌었는지는 `git diff` 가 보여준다는 안내만 넣는다.

- [x] **Step 1: 실패하는 테스트 작성** — 시나리오 6개
  - `rev_핀이_없으면_에러다`
  - `rev_핀이_일치하면_통과한다`
  - `대상_본문이_바뀌면_에러다`
  - `exception_핀은_선언_topic_본문의_해시다`
  - `이름을_유지한_채_선언_topic_을_바꾸면_커버_문서가_깨진다`
  - `진단_fix_가_bless_심볼_주소_형식이다`
- [x] **Step 2: `cargo test` — 실패 확인**
- [x] **Step 3: `check_revs` 구현**
- [x] **Step 4: `cargo test` 통과 확인**
- [x] **Step 5: 커밋** — `feat: rev 핀 검증`
  - 7625722~6bd6361 (3커밋) / 테스트 201

**수행 내역** — 7625722~6bd6361 (3커밋) / 테스트 201. K020·K021. 저장소 전역 이월 4건을 Task 12 로

---

## Task 9: CLI 골격 + build / list / keywords / refs

**파일**
- 수정: `src/main.rs`
- 테스트: `tests/cli.rs`

**인터페이스**
- 소비: `resolve::load`, `SymbolTable`, `check::*`
- 산출: `parse_project()` 와 `compile()` 두 진입점
- 산출: 실행 가능한 `kang` 바이너리

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
- **인자에 백틱을 쓰지 않는다.** 백틱은 셸 명령 치환이며 비대화형 호출에서도 터진다. 에이전트가 kang 을 부르는 방식이 정확히 비대화형이라 조용한 오작동이 된다.
- `kang refs docs/A.결제`, `kang show 'docs/A#결제의 방법'` 처럼 `/`·`.`·`#`·`!` 만으로 파싱한다.
- 공백이 있는 이름은 셸 인용이 필요하다. 정상 규약이다.

**출력 규칙** (스펙 6.3)
- 한 라인이 의미론적 완결성을 갖는다. 경로는 항상 전체 경로, 계층 축약 없음.
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

에이전트는 종료 코드로 분기한다. 1과 2를 섞으면 "문서를 고쳐야 한다" 와 "명령을 잘못 썼다" 를 구분할 수 없다.

**환경 오류에는 `--help` 를 출력하지 않는다.** git 저장소가 아닌 것은 명령을 잘못 쓴 게 아니다. 여기서 help 를 보여주면 에이전트가 철자를 의심하며 재시도한다. `git init` 지시만 낸다.

**컨트롤러 이월 (Task 4 에서 나옴) — `Location.line == 0` 은 "가리킬 줄이 없음" 이다.**

`K050`(git 저장소 아님)·`K051`(파일을 읽지 못함) 처럼 **줄을 가리킬 수 없는 진단**이 있다. `Location.line` 은 `usize` 라 `Option` 을 쓸 수 없고(타입 계약 고정), `1` 을 적으면 진단이 존재하지 않는 줄을 가리켜 거짓말이 된다. 그래서 Task 4 가 `0` 을 "줄 없음" 센티널로 쓴다.

`report()` 렌더러는 `line == 0` 을 **특별 처리해야 한다** — `docs/A:0` 으로 찍으면 안 된다. 파일 경로만 쓰거나(`K051`), 경로도 의미 없으면 위치 표기를 통째로 생략한다(`K050`).

**그리고 `line == 0` 인 진단은 `Location.doc` 이 문서 주소가 아니다.** `DocPath` 는 원래 "루트 기준 상대 경로 조각들" 인데, `K050`·`K051`(디렉토리)은 절대 경로 전체를 조각 하나에 담는다 (`DocPath(vec!["/Users/x/tmp"])`). `Display` 가 `/` join 이라 화면 출력은 맞지만 **문서로 열거나 주소로 재구성하면 안 된다.** `line == 0` 이 그 판별자다.

**`resolve::find_root` 는 절대 경로를 요구한다.** 상대 경로를 주면 `ancestors()` 의 마지막 빈 경로가 프로세스 cwd 기준으로 해석되어 빈 루트를 `Ok` 로 돌려준다. `std::env::current_dir()` 을 그대로 넘겨라 — 항상 절대 경로다.

**핵심 규칙**: **조회** 명령은 전부 `compile()` 을 먼저 거친다. error가 있으면 진단만 출력하고 종료 코드 1로 끝낸다. 문서는 한 줄도 출력하지 않는다.

**`bless` 와 `init` 은 `compile()` 을 거치지 않는다.** `bless` 가 필요한 상황은 정의상 전부 error 다 — 핀 없음도 error, 핀 불일치도 error. `compile()` 을 통과해야 실행된다면 `bless` 는 영원히 실행될 수 없다. 대신 `parse_project()` 까지만 쓴다.

```rust
/// 파싱만 한다. 진단 규칙을 돌리지 않는다.
/// bless 처럼 error 상태에서 실행되어야 하는 명령이 쓴다.
fn parse_project() -> Result<(Project, SymbolTable), Vec<Diagnostic>>;

/// parse_project + 모든 진단 규칙. error 가 하나라도 있으면 Err.
fn compile() -> Result<(Project, SymbolTable), Vec<Diagnostic>>;
```

**컨트롤러 이월 (Task 5 리뷰에서 나옴 — 최우선 연결점)**

**`compile()` 이 진단 함수를 하나라도 빠뜨리면 그 규칙은 조용히 발화하지 않는다.** Task 5 시점에 `check_cycles` 를 부르는 곳이 `src` 안에 없었다 — 함수와 테스트는 완성됐지만 프로덕션 경로에 연결되지 않은 상태다. 이후 태스크가 만드는 진단 함수도 같은 위험을 갖는다.

`compile()` 은 **다음 전부**를 돌리고 진단을 모은다. 하나라도 빠지면 그 스펙 규칙이 죽는다.

| 함수 | 모듈 | 태스크 | 스펙 |
|---|---|---|---|
| `parse_document` (파일마다) | `parse` | 2·3 | 4절 문법 |
| `find_root` · `load` | `resolve` | 4 | 3절 |
| `SymbolTable::build` | `resolve` | 4 | 4.3 |
| `check_cycles` | `check` | 5 | 5.3 |
| `check_symbols` | `check` | 6 | 5.1 |
| `check_exceptions` | `check` | 7 | 5.2 |
| `check_revs` | `check` | 8 | 4.8 |

**연결 자체를 테스트로 못박아라.** 각 규칙이 위반되는 최소 프로젝트를 만들고 `compile()` 이 그 진단을 내는지 확인한다 — 함수를 직접 부르는 단위 테스트만으로는 연결 누락을 잡지 못한다.

**컨트롤러 이월 (Task 7 에서 나옴) — 종료 코드는 진단 개수가 아니라 `Severity::Error` 개수로 판정한다.**

`K031`(`pending` 인데 cover 없음)이 **유일하게 `Severity::Warn`** 이다. 스펙 5.2 가 그 칸을 error 가 아니라 경고로 정했고, 전역 제약이 "`kang build` 기본 심각도는 error. error 발생 시 종료 코드 1" 이다.

따라서 `compile()` 이 `Err` 를 돌려주는 조건도, `main` 이 종료 코드 1 을 내는 조건도 **진단 벡터가 비어 있지 않은지가 아니라 그 안에 `Severity::Error` 가 있는지**다. `diagnostics.is_empty()` 로 판정하면 `K031` 하나만 있는 정상 문서가 빌드 실패로 처리된다.

`K031` 만 있는 프로젝트가 **종료 코드 0** 이고 그 경고가 출력에는 나오는 것을 테스트로 못박아라.

경계는 이렇다. **파싱이 실패하면 `bless` 도 실패한다** — 대상 import 줄을 찾을 수 없기 때문이다. 파싱이 성공하면 진단이 몇 개든 `bless` 는 진행한다.

**`--help` 는 에이전트의 첫 접점이다.** 인자를 틀린 에이전트가 다음에 하는 일이 `kang --help` 다. 여기서 명령·인자 형식·종료 코드를 전부 보여줘야 재시도가 성공한다. 사용법 오류 시에도 같은 텍스트를 출력한다.

`ponytail:` 인자 파싱을 직접 쓴다. 플래그는 `bless --import` 하나뿐이라 `match` 로 충분하다. 플래그가 늘면 clap 으로 교체.

- [x] **Step 1: 실패하는 테스트 작성** — 시나리오 10개
  - `build_는_정상_프로젝트에서_종료코드_0_이다`
  - `build_는_에러가_있으면_종료코드_1_이다`
  - `에러가_있으면_list_가_아무것도_출력하지_않는다`
  - `list_가_전체_경로로_출력한다`
  - `keywords_가_경로_스코프로_필터된다`
  - `refs_가_키워드를_참조하는_topic_을_출력한다`
  - `백틱_없는_인자를_파싱한다`
  - `알_수_없는_서브커맨드는_사용법을_출력하고_종료코드_2_다`
  - `인자가_부족하면_사용법을_출력한다`
  - `kang_파일이_0개면_그렇다고_알린다`
  - `inspect_는_v2_안내와_함께_종료코드_3_이다`
  - `help_이_명령과_인자_형식과_종료코드를_전부_보여준다`
  - `git_저장소가_아니면_help_대신_git_init_지시만_출력한다`
- [x] **Step 2: `cargo test` — 실패 확인**
- [x] **Step 3: 서브커맨드 디스패치와 `compile()` 구현**
- [x] **Step 4: `build` / `list` / `keywords` / `refs` 출력 구현**
- [x] **Step 5: `cargo test` 통과 확인**
- [x] **Step 6: 커밋** — `feat: CLI 골격과 build/list/keywords/refs`
  - 4270cf8~05ea6e9 (4커밋) / 테스트 230

**수행 내역** — 4270cf8~05ea6e9 (4커밋) / 테스트 230. **바이너리가 처음 동작.** EPIPE·비UTF-8 인자 패닉과 `refs` DP 불일치를 리뷰가 실행으로 잡음 → `check::이름_분할` 추출

---

## Task 10: YAML 이미터 + `kang show`

**파일**
- 생성: `src/yaml.rs`, `src/show.rs`
- 수정: `src/main.rs`
- 테스트: `tests/cli.rs`

**인터페이스**
- 소비: `Project`, `SymbolTable`
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

**출력 스키마** — 스펙 6.4 그대로. 최상위 키 순서는 `path`, `keywords`, `referencingKeywords`, `exceptions`, `covers`, `topics`.

- keyword 항목은 `name`·`description`·`referencedBy` 와, `#` 로 연결된 상세 topic 이 있으면 **`detail`(그 topic 의 전체 경로)** 을 담는다. 파싱만 하고 버리지 않는다.
- 재귀 임베드는 `Document.imports` 를 직접 순회한다. 참조 전파는 v2 다.

**중복 제거**: 이미 전개한 topic·키워드는 방문 집합에 넣고, 두 번째부터는 경로 문자열만 넣는다. **깊이 제한은 v1 에 두지 않는다** — 손댈 때는 읽기 시점 옵션이 아니라 빌드 시점 구조 린트로 만든다 (스펙 6.4).

`ponytail:` YAML 이미터를 직접 쓴다. 이미터 API 는 선언형 `seq`/`map` 만 노출하고 수동 들여쓰기 커서를 주지 않는다 — 짝 맞추기 실수가 구조적으로 불가능해야 한다. 한계는 인용 규칙이고, 한글 description 에 `: ` 가 들어가는 경우가 위험 지점이며 `scalar()` 테스트가 그걸 막는다. 스키마가 늘어나면 serde 기반 크레이트로 교체.

- [x] **Step 1: 실패하는 테스트 작성** — 시나리오 7개
  - `콜론이_포함된_설명은_인용된다`
  - `따옴표가_이스케이프된다`
  - `멀티라인_본문이_literal_scalar_로_나온다`
  - `show_가_정의_키워드와_참조_topic_을_출력한다`
  - `show_가_예외와_커버_본문을_임베드한다`
  - `show_가_참조_topic_을_재귀적으로_임베드한다`
  - `다이아몬드_의존에서_같은_topic_이_한_번만_전개된다`
- [x] **Step 2: `cargo test` — 실패 확인**
- [x] **Step 3: `yaml.rs` 이미터와 `scalar()` 구현**
- [x] **Step 4: `show.rs` 구현 (재귀 임베드 + 방문 집합)**
- [x] **Step 5: `main.rs` 에 `show` 서브커맨드 연결**
- [x] **Step 6: `cargo test` 통과 확인**
- [x] **Step 7: 커밋** — `feat: YAML 이미터와 kang show`
  - cf4c171·103b852 / 테스트 255

**수행 내역** — cf4c171·103b852 / 테스트 255. **리뷰가 유효하지 않은 YAML 을 내는 입력 3종을 두 파서로 재현**(제어문자·U+2028·`=`). 251 passed 상태에서 나왔다

---

## Task 11: `kang bless`

**파일**
- 생성: `src/bless.rs`
- 수정: `src/main.rs`
- 테스트: `tests/cli.rs`

**인터페이스**
- 소비: `parse_project()` 의 결과, `hash::rev`. **`compile()` 을 쓰지 않는다** — bless 가 필요한 상황은 전부 error 다
- 산출:

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
- **주소는 심볼이다.** 문서를 고친 뒤 bless 하는 것이 정상 워크플로라 줄이 밀리고, 줄 번호로 갱신하면 엉뚱한 핀을 조용히 바꾼다 (ADR-0003).
- 한 파일 안에서 같은 심볼을 두 번 import 하는 것은 이미 error 이므로 주소는 유일하게 결정된다.
- 해당 import 줄의 `rev "..."` 부분만 치환하고, **없으면 줄 끝에 삽입**한다. 나머지는 건드리지 않는다.
- 그 문서에 해당 import 가 없으면 에러로 거부한다.
- 일괄 해제 수단을 만들지 않는다. 심볼 이름만 주고 전체 참조처를 갱신하는 경로가 있으면 안 된다.

- [x] **Step 1: 실패하는 테스트 작성** — 시나리오 7개
  - `rev_핀을_현재_해시로_갱신한다`
  - `핀이_없으면_삽입한다`
  - `문서를_고쳐_줄이_밀려도_올바른_import_를_찾는다`
  - `그_문서에_없는_import_는_거부한다`
  - `갱신_후_build_가_통과한다`
  - `build_출력의_fix_문자열을_그대로_인자로_받는다`
  - `백틱_없는_심볼_주소를_파싱한다`
  - `진단이_있는_상태에서도_bless_가_실행된다`
  - `파싱이_실패하면_bless_도_실패한다`
- [x] **Step 2: `cargo test` — 실패 확인**
- [x] **Step 3: `ImportAddress::parse` 와 `bless` 구현**
- [x] **Step 4: `main.rs` 에 `bless` 서브커맨드 연결**
- [x] **Step 5: `cargo test` 통과 확인**
- [x] **Step 6: 커밋** — `feat: kang bless`
  - 48a2c47·1e23eeb / 테스트 275

**수행 내역** — 48a2c47·1e23eeb / 테스트 275. **rev 핀 벽이 열림.** TOCTOU 가드가 좌표를 안 봐 산문에 핀이 박히던 것을 재파싱으로 닫음. pid 수정 제안은 리뷰가 빌드해 측정하고 반박(조용한 유실 0/12→12/12)

---

## Task 12: 통합 검증

**파일**
- 생성: `tests/fixtures/` (예제 kang 프로젝트), `tests/cli.rs` 에 통합 시나리오 추가

**인터페이스**
- 소비: 전체

**구현 요점**
스펙 V0001 의 예제(`docs/A`, `docs/B`, `docs/C` — 결제·카드결제·무료결제)를 fixture 로 만들고, 전 명령을 실제로 돌린다.

- [x] **Step 1: fixture 프로젝트 작성** — 결제·카드결제·무료결제 3개 문서. git 저장소여야 루트 탐색이 동작하므로 fixture 도 저장소로 만든다
  - `tests/cli.rs` 의 `예제_프로젝트` / `예제_프로젝트_통과`. 핀 없이 쓰고 `kang bless` 가 넣게 한다 (스펙 4.8 3단계). 임시 디렉토리에 만든다 — `tests/` 안에 두면 이 저장소 워크트리에 중첩 저장소가 생긴다
- [x] **Step 2: 통합 테스트 작성** — 시나리오 9개 중 8개 (하나는 Task 14 로 이월)
  - [x] `fixture_프로젝트가_build_를_통과한다`
  - [x] `상위_문서_수정_후_모든_참조처가_깨진다`
  - [x] `build_출력을_bless_에_그대로_넘기면_전부_해제된다` — C1 의 회귀 테스트
  - [x] `참조처를_먼저_고친_뒤_bless_해도_올바른_핀이_갱신된다`
  - [x] `exception_선언_topic_을_바꾸면_커버_문서가_깨진다`
  - [x] `순환_import_를_만들면_체인이_출력된다`
  - [x] `show_출력이_유효한_yaml_이다` — python3+pyyaml 게이트, 없으면 건너뜀
  - [x] `error_상태에서는_어떤_조회도_출력되지_않는다`
  - [ ] `git_init_후_kang_init_과_build_세_명령으로_통과한다` — **Task 14 로 이월.** `kang init` 이 아직 종료 코드 3 이라 지금 만들 수 없다
  - [x] `진단_3종의_구조가_스펙_5_1_1_과_일치한다`
**컨트롤러 이월 (Task 8 리뷰에서 나옴 — 저장소 전역 일괄 처리)**

- **`[shell]` fix 의 `action` 이 한글 산문 접두사로 시작해 렌더된 줄을 그대로 실행할 수 없다.** `report()` 가 `[shell] {action}` 으로 찍는데 action 이 `이 import 에 rev 핀을 붙이세요: kang bless 'docs/b' --import 'docs/a#결제의 방법'` 이라, 통째로 복사하면 `command not found: 이` 다.

  스펙 5.1.1(V0001:261 "그대로 적용 가능한 fix", :306 예시 `[shell] kang bless ...`)과 6.1(:417 "인용까지 포함되어 출력되므로 복사해 실행하면 된다")이 요구하는 것은 **명령만 있는 줄**이다.

  **한 태스크의 문제가 아니다** — `K001`·`K020`·`K021`·`K050` 등 저장소의 모든 Shell fix 가 같은 모양이다. 산문은 `message` 나 `note` 로 옮기고 `action` 에는 명령만 남긴다. **일괄로 고치고 렌더 결과를 셸에 그대로 넣어 보는 테스트를 붙여라.**

- **fix 규약이 형제 사이에서 갈린다.** `K030`·`K031`·`K034` 의 fix 는 핀 없는 import 줄만 만들어 적용하면 `K020` 이 새로 뜬다(스펙 4.8 의 3단계 레시피 1단계라 결함은 아니다). 그런데 **`K001` 은 bless 를 짝지어 낸다** — 같은 상황에서 한쪽은 1왕복, 한쪽은 2왕복이다. 에이전트가 소비자이므로 왕복 수가 곧 비용이다. 실제 흐름을 돌려 보고 통일 여부를 정하라.

- **같은 topic 이 같은 예외를 두 번 cover 하는 것이 error 인가.** 스펙 5.2(V0001:335)는 "둘 이상의 **topic** 이" 인데 `K033` 은 cover **선언 개수**로 센다. 구현이 문면보다 넓다. 스펙 문면을 고칠지 판정을 topic 단위로 접을지 정하라.

- **참조 병합 천장의 실제 빈도를 측정하라.** Task 6 절의 "알려진 한계" 참조 — 두 이름을 따로 언급한 본문이 병합되면 뒤 조각의 import 가 `K003` 으로 오인된다. 도그푸딩 코퍼스에서 얼마나 자주 발생하는지 재고 승급 여부를 정하라.

- [x] **Step 3: `cargo test` 통과 확인** — 293 passed (275 → 290 → 293)
- [x] **Step 4: `cargo clippy -- -D warnings` 통과 확인** — `--all-targets` 로 clean, `cargo fmt --check` 도 clean
- [x] **Step 5: 커밋** — 판정 J1~J4 와 이월 검증 결과는 `.superpowers/sdd/V0002-kang-v1-implementation/task-12-report.md`
  - C1(렌더된 `[shell]` fix 를 그대로 실행할 수 없음) — 저장소의 **아홉 자리를 전부** 고쳤다. `check.rs` 3자리(`K020`·`K021`·`K001`) + `resolve.rs` 6자리(`K050`·`K051`×5). `K050` 의 `action` 은 `git init`, 대안은 `note` 로
  - **`utf8_아님` 의 둘째 fix 는 삭제했다** — `-f` 인자가 첫 fix 실행 뒤에야 정해지므로 명령으로 낼 수 없다. 변환 레시피는 `message` 로. **명령이 아닌 것을 `Shell` 로 낸 것이 C1 의 근본 원인이다**
  - M8(주소 파싱 통합) 은 `참조들` 이 `ImportAddress::parse` 를 쓰게 하고 `조회` 를 같은 분할 순서로 맞췄다. `main.rs` 606 → 597 줄. **통합하니 셋 중 `show` 하나가 정본에서 벗어나 있던 것이 드러났다**
  - J1 의 결론대로 `K034` 에 `bless` 를 짝지어 **1왕복**으로 닫았다. `K030`·`K031` 은 핀을 붙일 문서를 몰라 2왕복이 정본이다
  - **스펙 6.0 주소 제약을 재작성했다** (3라운드 리뷰). "마지막 조각 안의 구분자는 하나" 는 **틀렸다** — 스펙 `:68`·`:91` 의 계층 keyword 와 이 태스크 자신의 픽스처(`docs/B.결제수단.카드`)를 불법으로 만들었다. 금지는 셋뿐이다: 문서 파일 이름의 `.`·`#`·`!`(`K113`), 심볼 이름의 `/`(빌드 영구 봉쇄 — 우선순위 최상), keyword 이름 한 조각의 `.`

**수행 내역** — f3080b9~06a6921 (4커밋) / 테스트 293. **저장소 전역 fix 계약 성립** — 진단이 시킨 명령을 복사해 실행하면 실제로 낫는다. 주소 파싱 3벌→1벌 통합이 `show` 의 숨은 divergence 를 드러냄

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

- [x] **Step 1: `release.yml` 작성** — 태그 트리거, 4 타깃 매트릭스 빌드, Releases 업로드
- [x] **Step 2: 로컬에서 `cargo build --release --target aarch64-apple-darwin` 성공 확인**
- [x] **Step 3: 테스트 태그를 푸시해 워크플로가 4개 아티팩트를 만드는지 확인**
- [x] **Step 4: `README.md` 설치 절 작성**
- [x] **Step 5: 커밋** — `ci: 크로스 플랫폼 릴리즈 워크플로`
  - 172cc5e / 테스트 293

**수행 내역** — 172cc5e / 테스트 293. release.yml 4타깃 + README + `KANG_REQUIRE_YAML`. curl↔아티팩트 이름 네 조합 일치 실측. **태그 푸시는 remote 없어 미검증**

---

## Task 14: `kang init` — 에이전트 진입점과 스킬

**파일**
- 생성: `src/init.rs`, `src/skill.md`
- 수정: `src/main.rs`
- 테스트: `tests/cli.rs`

**인터페이스**
- 소비: 없음 (파일 생성만)
- 산출:

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

kang 의 주 사용자는 다른 프로젝트에서 일하는 LLM 에이전트다. 그런데 "LLM 은 원본을 보지 않는다" 는 원칙은 **강제할 수 없다** — 에이전트는 `.kang` 파일을 그냥 읽을 수 있다.

그래서 이 원칙은 에이전트가 kang 의 존재와 사용법을 알 때만 성립한다. 저장소에 `.kang` 파일만 있으면 에이전트는 그게 무엇인지 모르고 `cat` 한다. 그리고 `cat` 으로 읽은 `.kang` 은 import 간접 참조 때문에 **마크다운보다 읽기 나쁘다.** 도구를 도입하고 오히려 나빠진다.

**생성물**

| 파일 | 처리 |
|---|---|
| `.claude/skills/kang/SKILL.md` | `src/skill.md` 를 임베드한 내용. 이미 있으면 건너뛴다. **스킬 내용의 유일한 사본** |
| `AGENTS.md` | Codex 용. 내용을 복제하지 않고 `SKILL.md` 경로를 가리키는 섹션만 덧붙인다 |
| `CLAUDE.md` | 한 줄 덧붙임 — "이 프로젝트의 문서는 kang 으로 유지보수된다. kang 스킬을 사용하여야 한다" |
| `docs/example.kang` | frontmatter 가 채워진 첫 문서 템플릿. 이미 `.kang` 파일이 있으면 건너뛴다 |

**저장소에 커밋되는 프로젝트 스코프 파일로 만든다.** 전역 스킬 설치를 요구하면 설치 단계가 늘고 clone 하는 사람마다 상태가 달라진다.

**구현 요점**
- `src/skill.md` 는 `include_str!("skill.md")` 로 **컴파일 타임에 바이너리에 임베드한다.** 릴리즈된 바이너리는 저장소 파일에 접근할 수 없다.
- **스킬 내용은 `SKILL.md` 한 곳에만 쓴다.** `AGENTS.md`·`CLAUDE.md` 는 가리키기만 한다. 두 파일에 복제하면 kang 이 막으려는 SoT 분열이 kang 자신의 도구에서 일어난다.
- **git 저장소를 요구하지 않는다.** `init` 은 갓 만든 디렉토리에서 실행되는 첫 명령이다. git 저장소가 아니면 현재 디렉토리를 루트로 삼고 `git init` 안내를 함께 출력한다. 여기서 종료 코드 2로 죽으면 T0 벽이 된다.
- 생성한 `SKILL.md` 는 kang 버전이 올라가도 갱신되지 않는다. **알려진 한계다** — `init` 이 멱등 건너뛰기이므로 CLI 가 바뀌면 기존 저장소의 스킬이 낡는다. 사용자가 생긴 뒤 실측하고 판단한다.

**스킬 내용** — **스펙 6.1 의 다섯 케이스를 그대로 쓴다.** 여기 복제하지 않는다. 두 곳에 두면 갈라진다.

- [x] **Step 1: 실패하는 테스트 작성** — 시나리오 6개
  - `네_파일을_생성한다`
  - `기존_CLAUDE_md_를_덮어쓰지_않고_섹션만_덧붙인다`
  - `이미_kang_섹션이_있으면_건너뛴다`
  - `이미_kang_파일이_있으면_예제를_만들지_않는다`
  - `init_직후_build_가_통과한다`
  - `생성된_SKILL_md_가_비어있지_않다`
  - `git_저장소가_아니어도_init_이_성공하고_git_init_을_안내한다`
  - `다른_도구_섹션이_있는_CLAUDE_md_에_kang_섹션만_덧붙인다`
- [x] **Step 2: `cargo test` — 실패 확인**
- [x] **Step 3: `src/skill.md` 작성** — 위 다섯 케이스 전부
- [x] **Step 4: `init.rs` 구현 (섹션 덧붙임, 멱등성)**
- [x] **Step 5: `main.rs` 에 `init` 서브커맨드 연결**
- [x] **Step 6: `cargo test` 통과 확인**
- [x] **Step 7: 커밋** — `feat: kang init 과 에이전트 스킬`
  - 9a5410c / 테스트 302

**수행 내역** — 9a5410c / 테스트 302. **TTHW 성립** — `git init`→`init`→`build` 세 명령 exit 0. `--help` 양방향 게이트를 런타임으로 올림

---

## 완료 조건

- [x] `cargo test` 전부 통과
  - 309 passed / 0 failed (check 121, cli 60+, parse 77+, yaml 14, hash 3)
- [x] `cargo clippy -- -D warnings` 통과
  - `--all-targets` 로 exit 0. `cargo fmt --check` 도 exit 0. `#[allow(...)]` 전역 0건
- [x] fixture 프로젝트에서 `kang build` 종료 코드 0
  - `tests/cli.rs:2146` `fixture_프로젝트가_build_를_통과한다`. 스펙 예제 3문서를 `tests/cli.rs` 안 인라인 픽스처로 두었다 (별도 `tests/fixtures/` 디렉토리를 만들지 않음 — 중첩 git 저장소를 피하려 각 테스트가 임시 디렉토리에 만든다)
- [ ] 스펙 V0001 의 3~6절 전 항목이 구현됨 (7절은 v2 `V0003` 로 분리, 8절은 비목표, 9절은 미결정 사항이라 구현 대상이 아님)
  - **부분 달성.** 최종 리뷰가 절 단위로 대조한 결과 미구현은 **6.0 의 세 금지 중 둘**뿐이다 — 심볼 이름의 `/` 금지(`K105` 형제 판정, **셋 중 빌드를 봉쇄하는 쪽**)와 keyword 이름 한 조각의 `.` 금지. 문서 파일 이름의 `.`·`#`·`!` 는 `K113` 으로 구현됐다. 그 밖의 3~6절 항목은 전부 구현되었고, 코드에 있고 스펙에 없는 진단 번호 0건, 9절(미결정)이 슬쩍 구현된 자리 0건. **`/` 금지는 V0004 우선순위 2** — 코드 심볼 이름이 `crate::mod::fn` 을 담으므로 proc-macro 착수 전에 닫아야 한다
- [ ] 태그 푸시 시 릴리즈 워크플로가 4개 타깃 바이너리를 산출
  - **미검증 (사용자 승인된 결정 D3).** git remote 가 없고 `act` 도 없어 런타임을 확인할 수단이 없다. 검증한 것: `release.yml` 이 유효한 YAML 이고 트리거가 `{'push': {'tags': ['v*']}}`, `actionlint` exit 0(오염판으로 red-green 확인), 호스트·`x86_64-apple-darwin` 크로스 빌드 성공, **README 의 curl 이 만드는 이름 = 워크플로 아티팩트 이름 네 조합 일치**. Linux 두 타깃은 rustc 통과·링크만 실패(macOS 에 링커 없음) → CI 전용. README 의 `OWNER` 는 플레이스홀더라 remote 가 붙는 순간 한 단어를 고쳐야 404 를 피한다
- [x] **빈 디렉토리에서 `git init` → `kang init` → `kang build` 가 세 명령으로 통과한다** — TTHW 측정 기준. `kang init` 은 git 을 요구하지 않지만 `build` 는 요구하므로(스펙 3절) `git init` 이 반드시 낀다
  - 릴리즈 바이너리로 실측. `init` 이 네 파일을 만들고 exit 0, `build` 가 **exit 0 이며 stdout·stderr 가 완전히 빈다**. `init` 산출물만으로 `list`·`keywords`·`refs`·`show`·`--help` 8개 명령이 전부 통과하고, 문서를 하나 더해 `bless` 왕복까지 확인했다. git 없이 `init` 만 돌려도 exit 0 + `git init` 안내(T0 벽이 되지 않는다). 재실행은 바이트 동일이며 "건너뜁니다" 의 이유를 구분해 말한다
- [ ] **`kang` 이 자기 저장소에서 동작한다** — 도그푸딩 착수 조건. `plans/`·`docs/adr/`·`CONTEXT.md` 이관은 별도 플랜
  - **미달.** 저장소에서 `kang build`·`kang list` 가 exit 0 이지만 `.kang` 문서가 **Task 14 개발 중 흘러든 `docs/example.kang` 템플릿 하나뿐**이라 의미 있는 도그푸딩이 아니다. 루트에 추적되지 않는 `kang init` 산출물 넷(`.claude/`, `AGENTS.md`, `CLAUDE.md`, `docs/example.kang`)이 있고 랜딩 diff 에는 들어가지 않는다. **V0004 우선순위 5** — 이 이관이 참조 병합 천장(`check::이름_분할`)의 승급 조건을 재측정할 유일한 수단이다(현재 "충돌 0건" 은 마크다운 코퍼스에서 잰 값)

---

## 구현 착수 전 해결 필요 (BLOCKING)

2026-08-05 `/plan-eng-review` 와 독립 리뷰에서 나온 것들. 같은 날 `/grill-with-docs` 세션에서 대부분 해소했다.

### 해소됨 — 스펙 V0001 갱신 완료

| # | 항목 | 결론 |
|---|---|---|
| 1 | 셸 백틱 충돌 | **CLI 인자에서 백틱 제거.** 본문 백틱은 유지. `fix` 문자열은 인용을 붙여 출력. 스펙 6.0 |
| 2 | `iknow` 문법 + 순환 충돌 | **`iknow` 는 참조가 아니라 부인(disavowal).** import 간선을 만들지 않으므로 순환이 아니다. 경로와 상호성은 검증하고 rev 핀은 갖지 않는다. 스펙 4.4 |
| 3 | rev 부트스트랩 데드락 | **핀 없이 쓰고 `bless` 가 삽입한다.** 더미 해시가 사라진다. 스펙 4.8 |
| 4 | `bless` 줄 번호 주소 | **심볼 주소로 변경** — `kang bless <문서> --import <심볼>`. 스펙 6.2, [ADR-0003](../../docs/adr/0003-symbolic-addressing-not-line-numbers.md) |
| 5 | 프로젝트 루트 미정의 | **git 저장소 루트가 프로젝트 루트.** 설정 파일을 두지 않는다. 스펙 3절 |
| 11 | 이름이 다른 개념 중복 | **결정론으로 닫히지 않는다.** grilling 에서 동의어(`also`) 를 도입했다가 2차 독립 리뷰 후 **삭제**했다. 동의어는 owner 가 남이 쓸 변형을 예견해야 작동하는데, 예견할 수 있었다면 두 이름이 생기지 않았다. 처음부터 kang 으로 설계하면 변형이 생길 자리가 없고(모든 백틱이 심볼), 이관 시 만나는 변형은 정본으로 통합한다. 스펙 4.3 |
| 12 | 더 단순한 대안 | **kang 이 정당하다.** 참조 코퍼스(ax-conta)가 이미 `Load when`·`Related`·Ubiquitous Language·living-document 재작성을 손으로 하고 있으나 **강제가 없다.** 50줄 해시 스크립트는 미해결 참조 error·owner 유일성·예외 커버리지를 주지 못한다 |

부수 결정: 폐기 용어에 묘비를 두지 않는다 ([ADR-0001](../../docs/adr/0001-no-tombstones-for-retired-terms.md)), 읽을 때 평탄화·쓸 때 import ([ADR-0002](../../docs/adr/0002-flatten-on-read-import-on-write.md)), keyword 는 도메인 특수 용어만, 이름 충돌은 계층 전체 경로 기준.

### 알고 감수하는 한계

| # | 항목 | 판단 |
|---|---|---|
| 6 | 일괄 해제가 실제로는 가능하다 | `kang build` 출력을 그대로 `bless` 에 넘길 수 있으므로 마찰이 사실상 없다. 그리고 마찰을 느낄 주체가 LLM 이라 애초에 작동하지 않는다. **시스템으로 막을 방법이 없어 포기한다** (ADR-0002) |
| 7 | 마찰의 인센티브가 반대로 걸린다 | import 를 지우고 복붙하는 편이 싸다. 같은 이유로 시스템이 막을 수 없다. 작성 스킬이 `show` 선행을 지시하는 것이 완화 수단이다 |
| 10 | 백틱 전면 심볼화의 채택 비용 | `200원`·`null`·`POST /payments` 가 미해결 심볼 error 가 된다. **도그푸딩(kang 자기 문서 이관)으로 실측한 뒤 판단한다.** 마크다운 렌더링 호환 결정도 그때 함께 명시한다 |

### 나머지도 해소됨

| # | 항목 | 결론 |
|---|---|---|
| 8 | `kang show` 깊이 무제한 | **v1 미구현, 모양만 확정.** 임계값을 데이터 없이 고르면 추측이므로 도그푸딩에서 실측한다. 손댈 때는 읽기 시점이 아니라 **빌드 시점 구조 린트**로 만든다 — "이 문서가 참조하는 정책이 너무 많다". 스펙 6.4 |
| 9 | `exception` 이 rev 강제의 구멍 | **exception import 도 rev 핀을 갖는다.** 해시 입력은 그 예외를 선언한 topic 의 본문이다. 이름을 유지한 채 맥락을 바꿔도 커버 문서가 깨진다. 스펙 4.8 |
| 13 | `ancestors()` 의 v1 소비자 | **v2 로 미룬다.** 참조 전파는 코드 참조 전용이고 코드 연동이 v2 이므로 v1 에 호출자가 없다. Task 5 축소 |

**구현 차단 항목 없음.**

### 이관 대상

**kang 의 첫 이관 대상은 kang 자신의 문서다.** `plans/`·`docs/adr/`·`CONTEXT.md` 를 `.kang` 으로 옮긴다. ax-conta 는 설계 검증용 참조 코퍼스일 뿐 이관 대상이 아니다.

---

## NOT in scope

- **코드 연동 (`kang inspect`)** — v2. 애노테이션 명세와 언어별 추출기가 별도 설계를 요구한다.
- **자연어 검색** — 확률적 기능을 쓰지 않는다는 원칙에 따라 영구 제외. 단, 위 BLOCKING 11번이 이 결정을 재검토 대상으로 만든다.
- **`--depth` 등 출력 축소 옵션** — LLM 에게 덜 읽을 선택권을 주지 않는다. BLOCKING 8번은 옵션이 아니라 error 로 처리하는 방향이다.
- **deprecated 정책 표현** — git 이 히스토리를 담당한다.
- **에디터 지원 (LSP, syntax highlight)** — v1 이후. BLOCKING 10번의 결론에 따라 우선순위가 바뀔 수 있다.
- **crates.io 발행** — GitHub Releases 로 시작한다. 이름 선점이 필요해지면 그때.

## What already exists

외부에서 가져올 수 있는 것은 다음과 같고, `sha2` 외에는 전부 의도적으로 쓰지 않는다.

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
| `bless` 다중 위치 | 검증 중 실패로 파일이 반쯤 갱신됨 | Task 11 | 전체 검증 후 일괄 쓰기 | 보임 |
| `bless` 주소 지정 | 문서 수정으로 줄이 밀림 | Task 11 | 심볼 주소라 영향 없음 (ADR-0003) | 해당 없음 |
| `bless` 실행 조건 | error 상태라 `compile()` 이 막음 | Task 9·11 | `parse_project()` 만 거친다 | 보임 |
| `exception` 의미 변경 | 커버 문서가 안 깨짐 | Task 8 | 선언 topic 의 본문 해시를 핀 | 보임 |
| `kang show` 깊이 | 컨텍스트 초과, 잘린 입력으로 오답 | **없음** | **없음** | **안 보임 — v1 미구현, 스펙 6.4** |
| `load()` IO 실패 | 잘못된 UTF-8 파일 | Task 4 | 진단으로 변환, 나머지 계속 | 보임 |
| 미해결 import | 대상 파일·심볼 부재 | Task 6 | error | 보임 |
| 파서 실패 | 짝 안 맞는 백틱, 깨진 import | Task 2·3 | 진단 | 보임 |
| `init` 환경 | git 저장소 아님 | Task 14 | git 없이 진행, `git init` 안내 | 보임 |
| `iknow` 로 복제 축복 | 남의 정의를 복제하고 "다른 뜻" 선언 | **불가능** | **불가능** | **안 보임 — 감수하는 한계, 스펙 4.4** |

**critical gap 2건** — `show` 깊이는 v1 미구현이고 모양만 확정돼 있다. `iknow` 복제 경로는 자연어 판정이 필요해 기계적으로 막을 수 없으며 스펙 4.4 가 한계로 명시한다.

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

**실행 순서:** A + B + C 를 병렬 워크트리로 시작. B 완료 후 D + E 병렬. **D·E 와 C(Task 14) 가 모두 끝나면** F. Task 12 의 통합 시나리오가 `kang init` 을 호출하므로 C 가 F 의 선행이다.

**충돌 플래그:** Lane D 와 E 는 각각 `check.rs` 와 `yaml.rs`/`show.rs` 만 만지므로 충돌하지 않는다. 단 둘 다 Task 4 의 `SymbolTable` 시그니처에 의존하므로, B 가 끝나기 전에 D·E 를 띄우면 안 된다.

## Implementation Tasks

`/plan-eng-review` 와 독립 리뷰의 발견에서 나왔고, **2026-08-05 `/grill-with-docs` 세션에서 전부 처리했다.** 스펙 V0001 갱신 완료.

- [x] **T1** — CLI 인자 문법 확정. 백틱 제거, `fix` 문자열 인용 출력. 스펙 6.0
- [x] **T2** — `iknow` 문법 정의와 순환 충돌 해소. 부인(disavowal)이라 import 간선이 아니다. 스펙 4.4
- [x] **T3** — rev 핀 부트스트랩. 핀 없이 쓰고 `bless` 가 삽입한다. 스펙 4.8
- [x] **T4** — `bless` 주소를 심볼로 변경. 스펙 6.2, ADR-0003
- [x] **T5** — 프로젝트 루트 = git 저장소 루트. 스펙 3절
- [x] **T6** — exception import 도 rev 핀을 갖는다. 해시 입력은 선언 topic 본문. 스펙 4.8
- [x] **T7** — show 깊이. v1 미구현으로 결정하되 모양 확정 — 빌드 시점 구조 린트. 스펙 6.3
- [x] **T8** — 백틱 전면 심볼화의 채택 비용. **도그푸딩으로 실측한 뒤 판단**하기로 결정. 마크다운 렌더링 호환 결정도 그때 함께
- [x] **T9** — `ancestors()` 를 v2 로. Task 5 축소

### 다음 작업

- [ ] **T10 (P1)** — 갱신된 스펙에 맞춰 Task 1~13 의 시그니처와 테스트 시나리오를 재점검
  - 특히 Task 3(`iknow` 파싱), Task 8(exception rev), Task 11(`bless` 심볼 주소), Task 9(CLI 인자)
  - 검증: 스펙 4·5·6 절의 모든 규칙이 어느 태스크에 대응하는지 대조
- [ ] **T11 (P2)** — 도그푸딩 계획. `plans/`·`docs/adr/`·`CONTEXT.md` 를 `.kang` 으로 옮기는 순서와 성공 기준
  - 여기서 T8(백틱 비용)과 T7(깊이 임계)의 실측값이 나온다

## DX 리뷰 결과 (2026-08-05)

주 개발자는 **다른 프로젝트에서 일하는 LLM 에이전트**다. 사람은 결과를 검토할 뿐이다.

### 핵심 발견

**kang 의 창립 원칙이 DX 에 의해서만 강제된다.** "LLM 은 원본을 보지 않는다" 는 강제할 수 없다 — 에이전트는 `.kang` 파일을 그냥 읽을 수 있다. 그러면 원칙을 지키게 만드는 유일한 수단은 `kang show` 가 `cat` 보다 쉬운 것뿐이다. 그런데 `cat` 으로 읽은 `.kang` 은 import 간접 참조 때문에 **마크다운보다 읽기 나쁘다.** 도구를 도입하고 오히려 나빠지는 경로가 존재했다. Task 14(`kang init` + 스킬)가 이 구멍을 메운다.

### 차원별 점수

| 차원 | 이전 | 현재 | 남은 격차 |
|---|---|---|---|
| Getting Started (TTHW) | 0/10 | 8/10 | 설치 후 `init` → `build` 두 명령. curl 설치가 실제로 동작하는지 미검증 |
| 진단 품질 | 3/10 | 9/10 | 예시 3건 확정. 나머지 규칙은 같은 모양을 따르는지 구현 시 확인 |
| 발견(discovery) | 0/10 | 8/10 | 스킬이 저장소에 커밋된다. 스킬 내용의 실효성은 도그푸딩에서 측정 |
| CLI 인체공학 | 5/10 | 8/10 | 백틱 제거, 종료 코드 4종, `--help` 규정 |
| 점진적 공개 | 6/10 | 6/10 | 문서 하나를 쓰려면 frontmatter·keyword·import·rev 를 한 번에 알아야 한다 |
| 탈출구 | 5/10 | 5/10 | 의도적으로 최소다. 일괄 해제도 깊이 제한도 없다 |
| 업그레이드 | 3/10 | 3/10 | **미해결** — 언어 자체의 버전 정책이 없다 |
| 접근성 | 6/10 | 6/10 | 바이너리 4종. Windows 미포함. 사람의 작성 경험은 미측정(T8) |

### 반영된 것

- **Task 14 신설** — `kang init` 이 `.claude/skills/kang/SKILL.md`·`AGENTS.md`·`CLAUDE.md`·첫 문서를 생성. 저장소에 커밋되는 프로젝트 스코프 파일
- **스펙 5.1.1 신설** — 진단 출력 예시 3건(미해결 심볼·iknow 누락·rev 불일치)의 형식 확정
- **스펙 6절** — 종료 코드 0/1/2/3 규약. `kang inspect` 는 v2 안내와 함께 3
- **스펙 6.1** — `kang init` 절과 스킬 내용(케이스별 기대 동작 5종)
- **Task 9** — `--help` 가 명령·인자 형식·종료 코드를 전부 보여준다

### 남은 격차 (v1 이후)

- **언어 버전 정책 없음.** kang 문법이 바뀌면 기존 `.kang` 문서가 어떻게 되는지 정의되지 않았다. 도그푸딩 중에는 저자와 도구 작성자가 같아서 안 드러나고, 외부 도입 직전에 터진다.
- **점진적 공개 부재.** 첫 문서를 쓰려면 네 가지 문법을 동시에 알아야 한다. `kang init` 의 템플릿이 이걸 얼마나 덜어주는지는 도그푸딩에서 측정한다.
- **Windows 미지원.** 릴리즈 타깃 4종이 macOS·Linux 뿐이다.

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 0 | — | — |
| Codex Review | `/codex review` | Independent 2nd opinion | 0 | — | Codex CLI 가 ChatGPT 계정 모델 제약으로 실행 불가 |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 2 | CLEAR | 1차 19건, 2차 6건 — 전부 해소 |
| Design Review | `/plan-design-review` | UI/UX gaps | 0 | — | 해당 없음 (CLI) |
| DX Review | `/plan-devex-review` | Developer experience gaps | 1 | CLEAR | 8차원 평가, 5건 반영, 3건 v1 이후 |

**OUTSIDE VOICE:** 2회 실행. Codex 는 계정 제약으로 두 번 다 실패, Claude 서브에이전트로 폴백. 1차 13건(구현 차단 5건 포함) 전부 해소, 2차 13건 전부 해소. 2차에서 찾은 `bless` 실행 불가가 유일한 구현 차단이었다.

**GRILLING (2026-08-05):** `/grill-with-docs` 로 전략 전제와 스펙 모순 처리. 참조 코퍼스 `~/Project/ax-conta` 를 근거로 검증. 구현 차단 5건 해소, 설계 결함 3건 해소, 2건 감수, ADR 3건 신설.

**DX REVIEW (2026-08-05):** 주 개발자를 LLM 에이전트로 확정. 창립 원칙이 DX 에 의해서만 강제된다는 발견에 따라 Task 14(`kang init` + 스킬), 스펙 5.1.1(진단 예시), 종료 코드 규약을 추가.

**PONYTAIL (2026-08-05):** 7건 중 6건 적용. `ImportGraph` 구조체·`canonical_of_synonym`·`assets/` 디렉토리·중복 파일 제거.

**2차 ENG REVIEW 에서 되돌린 결정:** grilling 에서 도입한 **동의어(`also`) 를 삭제**했다. 동의어는 owner 가 남이 쓸 변형을 예견해야 작동하는데, 예견할 수 있었다면 두 이름이 생기지 않았을 것이다. 처음부터 kang 으로 설계하면 변형이 생길 자리가 없다.

**VERDICT:** ENG + DX CLEARED — 구현 착수 가능. 남은 미결정은 전부 v1 이후 항목이다.

**기록 실패:** `gstack-review-log` 가 최소 JSON 도 거부함. `bun` 미설치로 gstack bun 기반 바이너리 일부가 동작하지 않는다. 이 리포트가 유일한 기록이다.

**UNRESOLVED DECISIONS:**
- 언어 자체의 버전 정책 — kang 문법이 바뀔 때 기존 `.kang` 문서를 어떻게 할지 미정
- Windows 지원 여부 — 릴리즈 타깃 4종이 macOS·Linux 뿐
- `kang show` 깊이 임계값 — 모양은 빌드 시점 구조 린트로 확정, 숫자는 도그푸딩 실측 후
