//! kang 문서의 추상 구문 트리(AST)와 진단 타입.
//!
//! 파서부터 진단 규칙, CLI 출력까지 전 단계가 이 타입들을 공유한다.
//! 이 모듈은 타입 정의와 표시 형식만 담고, 파싱 로직은 [`crate::parse`] 에 둔다.

// V0003 §4 가 적는 표기가 `#[kang::topic(...)]` 이다. 소비자는 의존성을 `kang` 으로 개명해
// 그 이름을 얻지만, 이 크레이트의 이름이 이미 `kang` 이라 개명하면 통합 테스트의
// `use kang::ast::…` 가 두 크레이트 사이에서 갈린다. 그래서 모듈 안에서만 별명을 준다 —
// 진단이 처방하는 속성 문면이 이 저장소에서도 그대로 참이 된다.
use kang_macros as kang;
use std::fmt;

/// 진단의 심각도. `Error` 는 컴파일 실패, `Warn` 은 통과하되 알림이다.
#[derive(Debug, PartialEq)]
pub enum Severity {
    /// 컴파일을 실패시키는 진단.
    Error,
    /// 컴파일은 통과하되 사용자에게 알리는 진단.
    Warn,
}

/// 진단이 가리키는 위치 하나.
#[derive(Debug)]
pub struct Location {
    /// 이 위치가 속한 문서.
    pub doc: DocPath,
    /// 문서 안의 줄 번호. 1-based 다.
    pub line: usize,
    /// 이 위치가 왜 관련되는지. 순환 체인이나 iknow 누락처럼
    /// 여러 위치가 얽힌 진단에서 각 위치의 역할을 설명한다.
    pub note: String,
}

/// 수정의 종류. 문법 규약이 갈리므로 판별자가 필요하다.
/// Edit 은 문서 문법(백틱 사용, 스펙 4.2), Shell 은 CLI 문법(백틱 금지·인용, 스펙 6.0).
#[derive(Debug, PartialEq)]
pub enum FixKind {
    /// 문서를 직접 고치는 수정. 백틱을 쓰는 문서 문법으로 기술한다.
    Edit,
    /// 셸에서 실행하는 수정. 백틱을 쓰지 않고 인용한다.
    Shell,
}

/// 진단이 제안하는 수정 하나. LLM 이 그대로 적용할 수 있어야 한다.
/// **줄 번호를 좌표로 쓰지 않는다** (ADR-0003).
/// `Diagnostic.fixes` 는 **순서 있는 목록**이며 앞에서부터 적용한다.
#[derive(Debug)]
pub struct Fix {
    /// 이 수정이 문서 편집인지 셸 명령인지.
    pub kind: FixKind,
    /// Edit 이면 대상 문서. Shell 이면 None.
    pub doc: Option<DocPath>,
    /// 어디에 무엇을 적용할지 한 문장으로.
    pub action: String,
}

/// 진단 하나. iknow 누락과 순환 검출은 본질적으로 다중 위치이므로
/// 위치와 수정 모두 목록이다.
/// 파서부터 진단 규칙까지 전 단계가 이 타입을 공유하므로 ast 에 둔다.
#[derive(Debug)]
pub struct Diagnostic {
    /// 심각도.
    pub severity: Severity,
    /// 진단 코드. 예: "K012". 규칙마다 고정이며 에이전트가 코드로 분기한다.
    pub code: &'static str,
    /// 왜 문제인지. 여러 문장이어도 머리글 한 줄로 찍는다 (스펙 5.1.1).
    pub message: String,
    /// 관련 위치 전부. 최소 1개.
    pub locations: Vec<Location>,
    /// 적용 순서대로 나열한 수정.
    pub fixes: Vec<Fix>,
}

/// 문서 경로. `docs/A` 는 ["docs", "A"] 이다.
/// HashMap 키로 쓰이므로 Hash + Eq 를 파생한다.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DocPath(pub Vec<String>);

impl fmt::Display for DocPath {
    /// 문서 경로를 `/` 로 이은 전체 경로로 출력한다. 확장자는 붙이지 않는다.
    ///
    /// 경로 표기는 이 구현 하나만 쓴다 — `list`·`keywords`·`refs`·`show`·진단이
    /// 각자 조립하면 서로 어긋난다.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.join("/"))
    }
}

/// 키워드 이름. 계층 키워드 `결제수단`.`카드` 는 ["결제수단", "카드"] 이다.
#[derive(Debug, PartialEq)]
pub struct KeywordName(pub Vec<String>);

/// 파일 밖으로 노출되는 심볼의 종류.
#[kang::keyword("CONTEXT.심볼", rev = "6c37e8")]
#[derive(Debug, PartialEq)]
pub enum SymbolKind {
    /// 도메인 특수 용어 선언.
    Keyword,
    /// `##` 로 시작하는 서술 단위.
    Topic,
    /// 정책의 예외 선언.
    Exception,
}

/// 다른 문서의 심볼을 가리키는 참조.
#[derive(Debug)]
pub struct SymbolRef {
    /// 대상 심볼이 선언된 문서.
    pub doc: DocPath,
    /// 대상 심볼의 종류.
    pub kind: SymbolKind,
    /// 대상 심볼의 이름. 계층 키워드면 조각이 여러 개다.
    pub name: Vec<String>,
}

/// 이 문서가 owner 인 도메인 특수 용어 선언 하나.
#[derive(Debug)]
pub struct Keyword {
    /// 키워드 이름. 계층이면 조각이 여러 개다.
    pub name: KeywordName,
    /// 필수인 한 줄 정의.
    pub definition: String,
    /// 상세 설명을 담은 topic 이름. 선언 줄 끝의 `#` 뒤 백틱 이름으로 연결한다.
    pub detail: Option<String>,
    /// 같은 이름을 선언한 다른 문서들에 대한 인지 선언.
    pub iknow: Vec<SymbolRef>,
    /// 한 줄 정의 안의 백틱 심볼과 등장 줄.
    /// 스펙 4.2 는 "본문과 **선언부**의 모든 백틱은 심볼 참조" 이므로
    /// keyword 정의 안의 참조도 Task 6 의 미해결 심볼 검사 대상이다.
    /// 이 필드가 없으면 keyword 정의가 kang 의 강제를 빠져나가는 은신처가 된다.
    pub refs: Vec<(String, usize)>,
    /// 선언이 등장한 줄 번호. 1-based 다.
    pub line: usize,
}

/// 파일 안에서 완결성을 갖는 서술 단위.
#[derive(Debug)]
pub struct Topic {
    /// `##` 헤딩 텍스트.
    pub name: String,
    /// 헤딩을 포함한 원문 본문.
    pub body: String,
    /// 대응하는 코드가 없는 것이 정상인 topic 인지.
    pub uncoded: bool,
    /// 같은 이름을 선언한 다른 문서들에 대한 인지 선언.
    pub iknow: Vec<SymbolRef>,
    /// 본문 백틱 심볼과 등장 줄.
    pub refs: Vec<(String, usize)>,
    /// 이 topic 이 선언한 예외들.
    pub exceptions: Vec<Exception>,
    /// 이 topic 이 커버하는 예외 이름과 선언 줄.
    pub covers: Vec<(String, usize)>,
    /// 헤딩이 등장한 줄 번호. 1-based 다.
    pub line: usize,
}

/// 어떤 정책에 예외가 존재한다는 선언. 본문 없이 이름만 갖는다.
#[derive(Debug)]
pub struct Exception {
    /// 예외 이름.
    pub name: String,
    /// 예외의 존재는 알지만 다루는 정책이 아직 결정되지 않았는지.
    pub pending: bool,
    /// 같은 이름을 선언한 다른 문서들에 대한 인지 선언.
    pub iknow: Vec<SymbolRef>,
    /// 선언이 등장한 줄 번호. 1-based 다.
    pub line: usize,
}

/// 다른 문서의 심볼을 이 문서로 들여오는 선언.
#[derive(Debug)]
pub struct Import {
    /// 들여올 대상 심볼.
    pub target: SymbolRef,
    /// 이 문서 안에서만 통하는 다른 이름.
    pub alias: Option<String>,
    /// 참조 시점 내용의 해시 핀.
    pub rev: Option<String>,
    /// 선언이 등장한 줄 번호. 1-based 다.
    pub line: usize,
}

/// 파싱을 마친 문서 하나.
#[derive(Debug)]
pub struct Document {
    /// 프로젝트 루트 기준 문서 경로.
    pub path: DocPath,
    /// frontmatter 의 `description` 값.
    pub description: String,
    /// 파일 최상단의 import 선언들.
    pub imports: Vec<Import>,
    /// 이 문서가 선언한 키워드들.
    pub keywords: Vec<Keyword>,
    /// 이 문서의 topic 들.
    pub topics: Vec<Topic>,
}
