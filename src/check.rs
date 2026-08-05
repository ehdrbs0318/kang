//! 심볼 규칙과 exception 상태 기계와 import 그래프의 순환을 검사하고, 진단을 사람과 LLM 이
//! 읽을 형태로 찍는 층.
//!
//! [`crate::resolve`] 가 문서를 다 읽어 온 뒤, 이름이 실제로 해석되는지와 예외마다 그것을
//! 다루는 정책이 있는지와 문서들이 서로를 전제로 삼아 고리를 이루는지 본다
//! (스펙 5.1·5.2·5.3).
//!
//! 이 모듈이 내는 진단 코드는 심볼 해석 대역 `K001`-`K009`, iknow·이름 충돌 대역
//! `K010`-`K019`, rev 핀 대역 `K020`-`K029`, exception 상태 기계 대역 `K030`-`K039`,
//! 순환 대역 `K040`-`K049` 를 쓴다.
//!
//! | 코드 | 규칙 |
//! |---|---|
//! | `K001` | 본문·선언부의 백틱 심볼이 스코프에 없음 |
//! | `K002` | import 대상 문서나 심볼이 존재하지 않음 |
//! | `K003` | import 했으나 어떤 topic 에서도 사용하지 않음 |
//! | `K004` | 한 심볼에 두 개 이상의 로컬 이름이 붙음 |
//! | `K005` | 계층 keyword 의 상위가 같은 파일에 keyword 로 선언되지도 import 되지도 않음 |
//! | `K010` | iknow 대상 문서나 심볼이 존재하지 않음 |
//! | `K012` | 여러 문서가 같은 이름을 선언했는데 iknow 상호 명시가 완전하지 않음 |
//! | `K020` | import 에 rev 핀이 없음 |
//! | `K021` | import 의 rev 핀이 대상의 현재 해시와 불일치 |
//! | `K030` | 일반 exception 을 커버하는 topic 이 없음 |
//! | `K031` | `pending` exception 을 커버하는 topic 이 아직 없음 (**warn**) |
//! | `K032` | `pending` 이라고 선언했으나 커버하는 topic 이 있음 |
//! | `K033` | 한 exception 을 커버하는 cover 선언이 둘 이상임 |
//! | `K034` | cover 가 가리키는 exception 이 그 문서의 스코프에 없음 |
//! | `K040` | import 그래프에 순환이 있음 |
//!
//! `K001` 과 `K012` 와 `K021` 은 스펙 5.1.1 이 번호까지 못박은 코드다. 나머지는 대역 안에서
//! 정했다.
//!
//! `K031` 만이 [`crate::ast::Severity::Warn`] 이다. 스펙 5.2 가 그 칸을 "통과" 로 정했으므로
//! 종료 코드를 1 로 만들면 안 된다.
//!
//! **노드는 파일이다** (스펙 5.3). 파일 그래프가 DAG 면 topic 그래프도 DAG 다 —
//! topic 간선 T→U 는 반드시 file(T)→file(U) 를 동반하므로 파일 단위 금지가 더 강하다.
//!
//! **간선은 [`crate::ast::Document::imports`] 에서만 만든다.** `iknow` 는 참조가 아니라
//! 부인이므로 간선이 아니다 — 상호 명시가 순환 error 를 유발하면 안 된다 (스펙 4.4).
//!
//! **대상 문서가 실재하지 않는 import 는 간선이 아니다.** 그것은 미해결 import 이고
//! 판정은 심볼 층의 몫이다. 여기서 함께 진단하면 같은 사실이 두 번 보고된다.
//!
//! **그래프 값을 남기지 않는다.** v1 에는 참조 전파의 소비자가 없다 (스펙 5.3).
//!
//! # 보고 규약
//!
//! `A→B→C→A` 와 `B→C→A→B` 는 같은 순환이므로 보고 단위를 정해야 한다.
//!
//! - **되돌아오는 간선 하나에 진단 하나.** 체인은 되돌아온 문서부터 지금까지의 스택이다.
//!   겹치는 순환(`A↔B` 와 `B↔C`)은 닫는 간선이 둘이므로 진단도 둘이다.
//! - **시작점은 문서 경로 순.**
//! - **같은 대상으로 가는 import 가 여럿이면 파일에서 먼저 나온 줄만 본다.**
//!
//! ponytail: 순환이 하나라도 있으면 반드시 진단이 나오지만(어떤 DFS 에서도 모든 순환은
//! 되돌아오는 간선을 최소 하나 포함한다), **간선을 공유하는 서로 다른 순환은 체인
//! 하나로만 보고될 수 있다.** 그 간선을 고치고 다시 돌리면 남은 순환이 드러난다.
//! 한 번에 전부 열거하려면 강한 연결 요소를 세는 순회로 올린다.

use crate::ast::{
    Diagnostic, DocPath, Document, Exception, Fix, FixKind, Import, Keyword, Location, Severity,
    SymbolKind, SymbolRef,
};
use crate::resolve::{Project, SymbolId, SymbolTable, 셸_인용};
use std::collections::{BTreeMap, HashMap, HashSet};

/// 파일 단위 import 관계를 DFS 로 훑어 순환을 검출한다.
///
/// 그래프 값을 남기지 않는다 — v1 에 질의할 소비자가 없다 (스펙 5.3).
///
/// # 매개변수
/// - `project`: 파싱을 마친 프로젝트. 여기 없는 문서로 가는 import 는 간선이 아니다
///
/// # 반환값
/// 순환마다 체인 전체를 담은 `K040` 진단들. 순환이 없으면 빈 벡터
pub fn check_cycles(project: &Project) -> Vec<Diagnostic> {
    // HashMap 의 나열 순서는 보장되지 않는다. 정렬해야 시작점 선택이 실행마다 같고,
    // 그래야 체인과 진단 순서를 골든 파일로 고정할 수 있다.
    let mut 순서: Vec<&DocPath> = project.docs.keys().collect();
    // DocPath 는 Vec<String> 래퍼이므로 조각을 그대로 비교한다.
    // to_string() 으로 비교하면 비교마다 String 을 새로 할당한다.
    순서.sort_by(|a, b| a.0.cmp(&b.0));

    let mut 완료: HashSet<&DocPath> = HashSet::new();
    let mut 스택: Vec<(&DocPath, usize)> = Vec::new();
    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    // 모든 문서를 시작점 후보로 돈다. 진입 차수가 0 인 문서만 시작점으로 삼는 최적화는
    // 순환만으로 이루어진 부분 그래프에서 그런 문서가 아예 없어 순환을 놓친다.
    for doc in 순서 {
        // 이미 다 본 문서 아래에는 순환이 없다는 것이 확인되었다.
        if !완료.contains(doc) {
            방문(project, doc, &mut 스택, &mut 완료, &mut diagnostics);
        }
    }

    diagnostics
}

/// 문서 하나에서 DFS 를 이어 간다. 아직 스택에 있는 문서로 되돌아오면 순환이다.
///
/// ponytail: 재귀로 훑는다. 스택 깊이는 문서 수를 넘지 않고 프레임이 작아, 수만 개
/// 문서가 한 줄로 이어져야 넘칠 수 있다. 실제로 그런 프로젝트가 나오면 명시적 스택으로
/// 바꾼다.
///
/// # 매개변수
/// - `project`: 파싱을 마친 프로젝트
/// - `doc`: 지금 들어가는 문서
/// - `스택`: 지금 내려온 경로. 각 항목은 문서와 **그 문서가 다음 문서로 나가는 import 줄**
/// - `완료`: 아래를 다 훑어 순환이 없다고 확인된 문서들
/// - `diagnostics`: 진단을 모을 곳
fn 방문<'a>(
    project: &'a Project,
    doc: &'a DocPath,
    스택: &mut Vec<(&'a DocPath, usize)>,
    완료: &mut HashSet<&'a DocPath>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // 나가는 줄은 아직 모른다. 간선을 따라갈 때마다 채운다.
    스택.push((doc, 0));

    // 같은 문서를 두 번 import 하면 간선이 둘이다. 첫 줄만 따라간다 —
    // 둘 다 보면 같은 순환이 두 번 보고된다.
    let mut 본_대상: HashSet<&DocPath> = HashSet::new();

    // 이 문서의 import 를 파일 순서대로 따라간다. `iknow` 는 간선이 아니므로 보지 않는다.
    for import in &project.docs[doc].imports {
        let 대상 = &import.target.doc;

        // 대상 문서가 실재하지 않으면 따라갈 간선이 없다. 미해결 import 진단은
        // 심볼 층의 몫이므로 여기서는 조용히 건너뛴다.
        if !project.docs.contains_key(대상) {
            continue;
        }

        // 이미 따라간 대상이면 건너뛴다.
        if !본_대상.insert(대상) {
            continue;
        }

        // 지금 따라가는 간선의 줄. 체인이 이 문서를 지날 때 가리킬 위치다.
        // 되돌아옴 검사보다 **먼저** 기록해야 체인의 note 가 참이 된다.
        스택
            .last_mut()
            .expect("방문 진입에서 자기 프레임을 밀어 넣었다")
            .1 = import.line;

        // position 을 match 검사식에 그대로 두면 반복자 임시값이 match 전체 동안 살아
        // 스택을 빌리고, 그러면 재귀 호출이 스택을 다시 빌릴 수 없다.
        let 되돌아온_자리 = 스택.iter().position(|(있는, _)| *있는 == 대상);

        match 되돌아온_자리 {
            // 아직 스택에 있는 문서로 되돌아왔다 = 순환. 그 자리부터가 체인이다.
            Some(시작) => diagnostics.push(순환(&스택[시작..])),
            // 아래를 아직 안 본 문서면 내려간다.
            None if !완료.contains(대상) => 방문(project, 대상, 스택, 완료, diagnostics),
            // 이미 다 본 문서. 그 아래에 순환이 없음이 확인되었으므로 다시 내려가지 않는다.
            None => {}
        }
    }

    완료.insert(doc);
    스택.pop();
}

/// 순환 체인 하나를 `K040` 진단으로 만든다.
///
/// 체인은 순환이 닫히며 다시 가리켜지는 문서에서 시작한다 — 마지막 문서의 import 가
/// 첫 문서를 가리켜 고리가 닫힌다. 위치는 각 문서가 **다음 문서를 import 하는 줄**이다.
///
/// # 매개변수
/// - `체인`: 순환에 든 문서와 각 문서가 다음 문서로 나가는 import 줄. 최소 1개
///
/// # 반환값
/// `K040` 진단
fn 순환(체인: &[(&DocPath, usize)]) -> Diagnostic {
    // 체인은 되돌아온 자리에서 잘라 낸 스택 조각이므로 비어 있을 수 없다.
    let 시작_문서 = 체인
        .first()
        .expect("순환 체인에는 되돌아온 문서가 최소 하나 있다")
        .0;
    // 마지막 문서의 import 가 시작 문서를 다시 가리켜 고리가 닫힌다. 그 문서가 고칠 대상이다.
    let 닫는_문서 = 체인[체인.len() - 1].0;

    // 체인이 하나면 자기 자신을 import 한 것이다. '서로' 라고 할 상대가 없고, 문서가
    // 하나뿐이라 "공통 개념을 상위 문서로" 도 뜻이 없다 — 설명과 처방이 함께 갈린다.
    let 자기_import = 체인.len() == 1;

    // 표시는 고리가 닫히는 것을 보여야 하므로 시작 문서를 끝에 한 번 더 적는다.
    let mut 경로: Vec<String> = 체인.iter().map(|(doc, _)| doc.to_string()).collect();
    경로.push(시작_문서.to_string());
    let 표시 = 경로.join(" → ");

    // Edit 수정은 문서 문법이므로 경로를 백틱으로 적는다 (스펙 5.1.1) — `docs/a` 가 아니라
    // `` `docs`/`a` `` 여야 에이전트가 파일에서 찾을 실제 텍스트와 같다.
    let 주소: String = 시작_문서
        .0
        .iter()
        .map(|조각| format!("`{조각}`"))
        .collect::<Vec<String>>()
        .join("/");

    Diagnostic {
        severity: Severity::Error,
        code: "K040",
        message: if 자기_import {
            format!(
                "import 그래프에 순환이 있습니다 — {표시}. 이 문서가 자기 자신을 전제로 삼고 있습니다."
            )
        } else {
            format!(
                "import 그래프에 순환이 있습니다 — {표시}. 서로가 서로의 전제가 되어 어느 문서가 상위 정책인지 정할 수 없습니다."
            )
        },
        // 스펙 5.1.1 은 "관련 위치 전부" 를 요구한다. 체인의 문서 하나가 항목 하나이고
        // note 는 그 줄이 실제로 import 하는 대상을 말한다.
        locations: 체인
            .iter()
            .enumerate()
            .map(|(자리, &(doc, line))| {
                // 다음 문서. 마지막 문서의 다음은 시작 문서이며 그 간선이 고리를 닫는다.
                let 다음 = 체인[(자리 + 1) % 체인.len()].0;
                Location {
                    doc: doc.clone(),
                    line,
                    // 마지막 항목만 고리가 닫히는 자리라고 덧붙인다.
                    note: if 자리 + 1 == 체인.len() {
                        format!("여기서 import 하는 대상: {다음} — 순환이 닫힙니다.")
                    } else {
                        format!("여기서 import 하는 대상: {다음}")
                    },
                }
            })
            .collect(),
        // 고리를 닫는 import 줄을 지우는 것이 순환을 끊는 가장 작은 편집이다.
        // 줄 번호를 좌표로 쓰지 않는다 (ADR-0003) — 무엇을 지울지는 import 대상으로 지정한다.
        fixes: vec![Fix {
            kind: FixKind::Edit,
            doc: Some(닫는_문서.clone()),
            action: if 자기_import {
                "이 문서는 자기 자신을 import 하고 있습니다. 자기 문서가 선언한 심볼은 import 없이 그대로 쓸 수 있으므로 이 import 줄을 지우세요.".to_string()
            } else {
                format!(
                    "이 문서의 import 줄 가운데 대상이 {주소} 인 것을 지워 순환을 끊으세요. 순환에 든 문서들이 함께 쓰는 개념이 있다면, 그 개념만 담은 상위 문서를 새로 만들어 각 문서가 그것을 import 하게 하세요."
                )
            },
        }],
    }
}

/// 심볼 선언 하나의 자리.
///
/// [`SymbolTable`] 은 이름으로 [`SymbolId`] 를 주지만 종류·줄 번호를 되돌려 주는
/// 접근자가 없다. 진단은 "어느 줄의 어떤 종류 선언인지" 를 말해야 하므로
/// [`Project::docs`] 에서 직접 모은다.
struct 선언<'a> {
    /// 이 선언이 있는 문서.
    doc: &'a DocPath,
    /// 심볼의 종류. 주소의 구분 기호가 종류마다 다르다 (스펙 4.1).
    kind: SymbolKind,
    /// 전체 이름. keyword 계층은 `.` 로 이어 붙인다 (스펙 4.3).
    name: String,
    /// 선언이 등장한 줄 번호. 1-based 다.
    line: usize,
    /// 이 선언에 붙은 인지 선언.
    iknow: &'a [SymbolRef],
}

/// 스펙 5.1 의 심볼 규칙을 검사한다.
///
/// 진단 순서는 **문서 경로 순**이고, 이름 충돌만 이름 순으로 맨 뒤에 붙는다.
/// 충돌은 문서 하나에 속하지 않으므로 문서 순서에 끼워 넣을 자리가 없다.
///
/// # 매개변수
/// - `project`: 파싱을 마친 프로젝트
/// - `table`: [`SymbolTable::build`] 가 만든 전역 심볼 테이블
///
/// # 반환값
/// 규칙을 어긴 자리마다 만들어진 진단들. 어긴 것이 없으면 빈 벡터
pub fn check_symbols(project: &Project, table: &SymbolTable) -> Vec<Diagnostic> {
    // HashMap 의 나열 순서는 보장되지 않는다. 정렬해야 진단 순서가 실행마다 같다.
    let mut 순서: Vec<&DocPath> = project.docs.keys().collect();
    // DocPath 는 Vec<String> 래퍼이므로 조각을 그대로 비교한다.
    let 정렬 = |a: &&DocPath, b: &&DocPath| a.0.cmp(&b.0);
    순서.sort_by(정렬);

    // 이름 충돌과 미해결 심볼은 둘 다 "이 이름을 선언한 문서가 누구인가" 를 물으므로
    // 색인을 먼저 완성한다. BTreeMap 이라 이름 순회가 결정적이다.
    let mut 선언들: BTreeMap<String, Vec<선언>> = BTreeMap::new();
    for doc in &순서 {
        for 하나 in 선언_훑기(&project.docs[*doc]) {
            선언들.entry(하나.name.clone()).or_default().push(하나);
        }
    }

    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    for doc in &순서 {
        let document = &project.docs[*doc];
        let scope = table.scope(doc);
        let (참조, 사용) = 참조_해석(document, &scope);
        iknow_실재_검사(document, project, table, &mut diagnostics);
        import_검사(document, project, table, &사용, &mut diagnostics);
        계층_상위_검사(document, &선언들, &mut diagnostics);
        미해결_검사(document, &참조, &scope, &선언들, &mut diagnostics);
    }

    이름_충돌_검사(&선언들, &mut diagnostics);
    diagnostics
}

/// `cover` 선언 하나와 그것이 그 문서에서 가리키는 심볼.
///
/// **`cover` 이름은 로컬 이름이다.** 스펙 4.6 의 정본 예시는 exception 을 alias 로 import 한
/// 뒤 그 alias 로 커버한다 — 선언 이름 `무료 상품에 대한 청구서` 와 cover 이름
/// `무료상품 청구서 예외` 는 다르고 둘을 잇는 것은 import 뿐이다. 그래서 짝은 이름 비교가
/// 아니라 [`SymbolTable::scope`] 로 맞춘다. 이름으로 맞추면 스펙의 정본 예시가 거부된다.
struct 커버<'a> {
    /// 이 문서의 스코프에서 cover 이름이 해석된 심볼. 해석되지 않으면 `None`.
    대상: Option<SymbolId>,
    /// cover 가 적힌 문서.
    doc: &'a DocPath,
    /// cover 가 적힌 topic 의 이름.
    topic: &'a str,
    /// cover 줄에 적힌 이름. 로컬 이름이므로 alias 일 수 있다.
    이름: &'a str,
    /// 선언이 등장한 줄 번호. 1-based 다.
    line: usize,
}

/// 스펙 5.2 의 exception 상태 기계를 검사한다.
///
/// | exception 상태 | cover 있음 | cover 없음 |
/// |---|---|---|
/// | 일반 | 통과 | `K030` error |
/// | `pending` | `K032` error | `K031` warn |
///
/// 한 예외를 커버하는 cover 선언이 둘 이상이면 `K033` error 다 — 예외 하나에 정책 하나를
/// 강제한다. 어떤 예외도 가리키지 않는 cover 는 `K034` error 다.
///
/// **다섯 규칙에 다섯 코드를 준다.** 원인마다 고쳐야 할 자리와 고치는 방법이 다르고,
/// 스펙 5.1.1 은 "에이전트가 코드로 분기할 수 있어야 한다" 고 못박는다. 같은 코드로 묶으면
/// 분기가 불가능해진다.
///
/// **`K034` 는 이 층의 몫이다.** [`참조_해석`] 이 cover 를 미해결 판정에서 뺐으므로
/// (짝 맞추기가 상태 기계의 일이기 때문이다) 여기서 보지 않으면 아무도 보지 않는다.
///
/// 진단 순서는 **문서 경로 순**이고, `K034` 는 그 뒤에 다시 문서 경로 순으로 붙는다.
/// 대상을 찾지 못한 cover 는 어느 예외에도 속하지 않아 진리표 순회에 끼워 넣을 자리가 없다.
///
/// ponytail: `pending` 이면서 커버가 둘 이상이면 `K032` 만 내고 `K033` 은 내지 않는다.
/// 선언이 스스로와 모순되는 것이 먼저 풀려야 할 사실이고, `pending` 을 떼면 다음 실행에서
/// `K033` 이 드러난다. 한 예외에 두 진단을 겹쳐 내려면 진리표를 규칙 목록으로 바꿔야 한다.
///
/// # 매개변수
/// - `project`: 파싱을 마친 프로젝트
/// - `table`: [`SymbolTable::build`] 가 만든 전역 심볼 테이블. cover 이름을 그 문서의
///   스코프로 해석하는 데 쓴다
///
/// # 반환값
/// 상태 기계를 어긴 자리마다 만들어진 진단들. 어긴 것이 없으면 빈 벡터
pub fn check_exceptions(project: &Project, table: &SymbolTable) -> Vec<Diagnostic> {
    // HashMap 의 나열 순서는 보장되지 않는다. 정렬해야 진단 순서가 실행마다 같다.
    let mut 순서: Vec<&DocPath> = project.docs.keys().collect();
    순서.sort_by(|a, b| a.0.cmp(&b.0));

    // cover 를 프로젝트 전체에서 먼저 모은다. 진리표는 "이 예외를 커버하는 것이 몇 개인가"
    // 를 묻는데 커버는 다른 파일에 있을 수 있어, 한 문서만 보고는 답할 수 없다.
    let mut 커버_전부: Vec<커버> = Vec::new();
    for doc in &순서 {
        // 스코프를 문서마다 한 번만 뜬다. `scope` 는 매 호출 매핑을 복제한다.
        let scope = table.scope(doc);
        // 한 문서의 topic 을 돌며 그 topic 이 선언한 cover 를 모은다.
        for topic in &project.docs[*doc].topics {
            // 한 topic 이 여러 예외를 커버할 수 있다.
            for (이름, line) in &topic.covers {
                커버_전부.push(커버 {
                    대상: scope.get(이름).copied(),
                    doc,
                    topic: &topic.name,
                    이름,
                    line: *line,
                });
            }
        }
    }

    // 예외의 식별자와 그 이름을 선언한 문서. 앞은 cover 가 예외를 가리키는지 판정하는 데,
    // 뒤는 import 하지 않은 cover 에게 어디서 가져오면 되는지 말하는 데 쓴다.
    let mut 예외_전부: Vec<(SymbolId, &DocPath, &Exception)> = Vec::new();
    // 값이 목록인 것은 같은 이름의 exception 이 iknow 로 여러 문서에 합법적으로 있을 수
    // 있기 때문이다 (스펙 4.4·5.1). 하나로 줄이면 `K034` 의 수정이 첫 문서를 단정하게 된다.
    let mut 예외_소유자: BTreeMap<&str, Vec<&DocPath>> = BTreeMap::new();
    for doc in &순서 {
        // exception 은 topic 안에서만 선언된다 (파서가 강제한다).
        for topic in &project.docs[*doc].topics {
            for exception in &topic.exceptions {
                // 같은 문서가 같은 이름을 두 번 선언하면 후보가 겹친다. 그 문서는 `K052`
                // 를 따로 받으므로 여기서는 수정이 두 번 나오지 않게만 막는다.
                let 후보 = 예외_소유자.entry(&exception.name).or_default();
                if !후보.contains(doc) {
                    후보.push(doc);
                }
                // **불변식: 이 해석은 항상 성공한다.** [`SymbolTable::build`] 가
                // [`Project::docs`] 의 모든 exception 을 (문서, 종류, 이름) 으로 넣고
                // ([`crate::resolve`] 의 1단계), [`SymbolTable::resolve`] 가 그 셋으로 찾는다.
                //
                // 그래도 `expect` 로 단정하지 않는다. 불변식이 깨지는 날 문서 컴파일러가
                // 사용자 문서를 앞에 두고 패닉하는 것보다, 그 예외 하나가 검사되지 않는 편이
                // 낫다. 침묵의 대가는 진단 누락이고 패닉의 대가는 도구 전체다.
                let Some(id) = table.resolve(&SymbolRef {
                    doc: (*doc).clone(),
                    kind: SymbolKind::Exception,
                    name: vec![exception.name.clone()],
                }) else {
                    continue;
                };
                예외_전부.push((id, doc, exception));
            }
        }
    }

    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    // 진리표. 예외 하나가 한 칸을 고른다.
    //
    // ponytail: 예외마다 cover 전부를 훑어 O(예외 × cover) 다. 한 문서의 선언 수가 수백을
    // 넘지 않는 문서 프로젝트에서는 무해하다. 커지면 `대상` 으로 키를 잡은 색인을 한 번
    // 만들어 O(예외 + cover) 로 내린다.
    for (id, doc, exception) in &예외_전부 {
        let 짝: Vec<&커버> = 커버_전부.iter().filter(|c| c.대상 == Some(*id)).collect();
        let 이름 = &exception.name;
        // Edit 수정은 문서 문법이므로 주소를 백틱으로 적는다 (스펙 5.1.1).
        //
        // 아래 세 값은 전부 클로저다. `(일반, 커버 하나)` 통과 칸이 예외마다 이것들을
        // 버리는데, 통과가 정상이고 다수인 경로다.
        let 주소 = || 심볼_주소(doc, &SymbolKind::Exception, 이름, true);
        // 커버 자리들. 어느 topic 이 커버하는지가 note 다.
        let 커버_위치 = || -> Vec<Location> {
            짝.iter()
                .map(|c| Location {
                    // 이름 뒤에 조사를 붙이지 않는다 ([`대상_설명`] 과 같은 이유다) —
                    // 끝소리에 따라 "이/가" 가 갈리는데 컴파일러가 판정할 수 없어,
                    // 붙이면 어느 한쪽에서 반드시 어색해진다. 줄표로 받는다.
                    doc: c.doc.clone(),
                    line: c.line,
                    note: format!("topic `{}` — 여기서 이 예외를 커버합니다.", c.topic),
                })
                .collect()
        };
        // 지울 cover 줄을 줄 번호 없이 지목한다 (ADR-0003). cover 에 적힌 이름은 alias 일 수
        // 있으므로 예외의 정본 이름이 아니라 **그 줄에 적힌 이름**을 그대로 적는다.
        //
        // 각 항목이 "줄" 로 끝나므로 뒤에 붙는 "을"·"입니다" 는 끝소리가 고정이다.
        let 커버_설명 = || -> String {
            짝.iter()
                .map(|c| {
                    format!(
                        "{} 의 cover `{}` 줄",
                        심볼_주소(c.doc, &SymbolKind::Topic, c.topic, true),
                        c.이름
                    )
                })
                .collect::<Vec<String>>()
                .join(", ")
        };

        let diagnostic = match (exception.pending, 짝.len()) {
            // (일반, 커버 하나) — 통과다. 유일하게 진단이 없는 칸이다.
            (false, 1) => continue,
            // (일반, 커버 없음) — 예외를 선언했으나 그것을 다루는 정책이 없다.
            (false, 0) => Diagnostic {
                severity: Severity::Error,
                code: "K030",
                message: format!(
                    "이 예외를 커버하는 정책이 없습니다 — `{이름}`. 예외를 선언했으면 그것을 다루는 topic 이 있어야 합니다 (스펙 5.2)."
                ),
                locations: vec![Location {
                    doc: (*doc).clone(),
                    line: exception.line,
                    // "프로젝트에 없습니다" 라고 쓰면 grep 으로 반증된다 — 이름만 같고
                    // import 되지 않은 cover 줄이 문자 그대로 있을 수 있고, 그때 같은
                    // 실행의 `K034` 가 그 줄을 지목해 두 진단이 서로를 반박하는 모양이
                    // 된다. 해석 기준으로 좁혀 말한다 ([`대상_설명`] 과 같은 기준이다).
                    note: "여기서 선언된 예외 — 이 예외를 가리키는 cover 가 없습니다. 이름만 같고 import 되지 않은 cover 는 이 예외를 가리키지 않습니다."
                        .to_string(),
                }],
                fixes: vec![
                    Fix {
                        kind: FixKind::Edit,
                        // 어느 문서의 어느 topic 이 이 예외의 정책인지는 사람이 정한다.
                        doc: None,
                        action: format!(
                            "이 예외를 다루는 정책 topic 을 정해 그 본문 끝에 다음 줄을 추가하세요: cover `{이름}`. 그 topic 이 다른 문서에 있다면 그 문서의 import 블록에 다음 줄을 먼저 추가하세요: import {}",
                            주소()
                        ),
                    },
                    Fix {
                        kind: FixKind::Edit,
                        doc: Some((*doc).clone()),
                        action: "정책이 아직 결정되지 않은 것이라면 이 exception 선언 줄 끝에 pending 을 붙이세요. 그러면 이 예외는 error 가 아니라 `K031` 알림이 되고 빌드는 통과합니다.".to_string(),
                    },
                ],
            },
            // (일반, 커버 둘 이상) — 예외 하나에 정책 하나를 강제한다.
            //
            // **세는 것은 cover 선언이지 topic 이 아니다.** 같은 topic 이 같은 예외를 두 번
            // 커버해도 중복이고, 그때 "둘 이상의 topic" 이라고 말하면 진단이 거짓이 된다.
            (false, _) => Diagnostic {
                severity: Severity::Error,
                code: "K033",
                message: format!(
                    "한 예외를 커버하는 cover 선언이 {}개입니다 — `{이름}`. 예외 하나에 정책 하나를 강제합니다 (스펙 5.2).",
                    짝.len()
                ),
                locations: {
                    let mut locations = vec![Location {
                        doc: (*doc).clone(),
                        line: exception.line,
                        note: "여기서 선언된 예외 — 아래 자리들이 모두 이 예외를 커버합니다."
                            .to_string(),
                    }];
                    locations.extend(커버_위치());
                    locations
                },
                fixes: vec![Fix {
                    kind: FixKind::Edit,
                    // 어느 것을 남길지는 사람이 정한다. 문서를 하나 고를 근거가 없다.
                    doc: None,
                    action: format!(
                        "이 예외를 실제로 다루는 정책 하나만 남기고 나머지 cover 줄을 지우세요. 지금 커버하는 것은 {}입니다.",
                        커버_설명()
                    ),
                }],
            },
            // (pending, 커버 없음) — 통과이되 알린다. `Severity::Warn` 을 내는 유일한 칸이다.
            (true, 0) => Diagnostic {
                severity: Severity::Warn,
                code: "K031",
                message: format!(
                    "정책이 아직 결정되지 않은 예외입니다 — `{이름}`. 알림이며 빌드를 실패시키지 않습니다 (스펙 5.2)."
                ),
                locations: vec![Location {
                    doc: (*doc).clone(),
                    line: exception.line,
                    // `K030` 과 같은 이유로 해석 기준이다.
                    note: "여기서 pending 으로 선언된 예외 — 이 예외를 가리키는 cover 가 아직 없습니다. 이름만 같고 import 되지 않은 cover 는 이 예외를 가리키지 않습니다."
                        .to_string(),
                }],
                fixes: vec![Fix {
                    kind: FixKind::Edit,
                    doc: Some((*doc).clone()),
                    // cover 줄만 넣으면 그 이름이 그 문서의 스코프에 없어 `K034` 와
                    // `K030` 이 난다. warn 을 없애려는 수정이 error 둘을 만들면 안 된다.
                    // 형제 갈래인 `K030` 의 첫 수정과 같은 문장을 쓴다.
                    action: format!(
                        "정책이 정해지면 그것을 다루는 topic 에 다음 줄을 추가하고, 이 선언에서 pending 을 지우세요: cover `{이름}`. 그 topic 이 다른 문서에 있다면 그 문서의 import 블록에 다음 줄을 먼저 추가하세요: import {}",
                        주소()
                    ),
                }],
            },
            // (pending, 커버 있음) — 선언과 문서가 서로 어긋난다.
            (true, _) => Diagnostic {
                severity: Severity::Error,
                code: "K032",
                message: format!(
                    "pending 으로 선언된 예외를 커버하는 topic 이 있습니다 — `{이름}`. pending 은 이 예외를 다루는 정책이 아직 결정되지 않았다는 뜻인데 커버하는 topic 이 실제로 있어, 선언과 문서가 서로 어긋납니다 (스펙 5.2)."
                ),
                locations: {
                    let mut locations = vec![Location {
                        doc: (*doc).clone(),
                        line: exception.line,
                        note: "여기서 pending 으로 선언된 예외 — 정책이 미결정이라고 말합니다."
                            .to_string(),
                    }];
                    locations.extend(커버_위치());
                    locations
                },
                // 둘 다 유효한 길이므로 각 수정이 자기 조건을 안고 있다.
                fixes: vec![
                    Fix {
                        kind: FixKind::Edit,
                        doc: Some((*doc).clone()),
                        // 커버가 둘 이상이면 pending 만 지운 결과는 `(일반, 커버 둘)` 이라
                        // `K033` 이 새로 난다. 이 함수의 rustdoc 이 인정한 사실을 fix 본문이
                        // 부정하면 안 된다 — 스펙 V0001:417 은 에이전트가 fix 를 **그대로
                        // 적용**한다고 정한다. `K005` 가 같은 이유로 갈래를 가른다.
                        action: if 짝.len() == 1 {
                            "정책이 이미 결정된 것이라면(커버하는 topic 이 그 정책입니다) 이 exception 선언에서 pending 을 지우세요. 그러면 이 예외는 커버된 일반 예외가 되어 통과합니다.".to_string()
                        } else {
                            format!(
                                "정책이 이미 결정된 것이라면 이 exception 선언에서 pending 을 지우고, 커버하는 {} 가운데 실제 정책 하나만 남기세요. pending 만 지우면 커버가 둘 이상이라 `K033` 이 남습니다.",
                                커버_설명()
                            )
                        },
                    },
                    Fix {
                        kind: FixKind::Edit,
                        // 커버가 여러 문서에 흩어질 수 있어 문서를 하나로 고를 수 없다.
                        doc: None,
                        action: format!(
                            "정책이 아직 결정되지 않은 것이라면 {}을 지우세요. 그러면 이 예외는 `K031` 알림이 되고 빌드는 통과합니다.",
                            커버_설명()
                        ),
                    },
                ],
            },
        };
        diagnostics.push(diagnostic);
    }

    // 어떤 예외도 가리키지 않는 cover. `커버_전부` 가 이미 문서 경로 순이다.
    //
    // ponytail: 위와 같은 이유로 O(cover × 예외) 다. 같은 색인을 만들면 함께 내려간다.
    for c in &커버_전부 {
        // 스코프에서 해석되고 그것이 예외이면 짝이 맞은 것이다.
        if c.대상
            .is_some_and(|id| 예외_전부.iter().any(|(예외_id, _, _)| *예외_id == id))
        {
            continue;
        }
        let 이름 = c.이름;
        // 스코프에 이름이 아예 없는 것과, 있으나 예외가 아닌 것은 다른 사실이다.
        // 뭉뚱그리면 둘 중 하나에 대해 진단이 거짓을 말한다.
        let 이유 = if c.대상.is_some() {
            "이 문서에서 그 이름은 exception 이 아닙니다. cover 는 exception 만 가리킬 수 있습니다 (스펙 4.6)."
        } else {
            "이 문서는 그 이름을 선언하지도 import 하지도 않았습니다."
        };

        let mut fixes: Vec<Fix> = Vec::new();
        // 그 이름의 예외가 다른 문서에 있고 이 문서가 그것을 들여오지 않았다면,
        // 고칠 것은 빠진 import 줄이므로 그것을 그대로 준다 (스펙 5.1.1).
        if c.대상.is_none()
            && let Some(후보) = 예외_소유자.get(이름)
        {
            // 후보가 둘 이상이면 어느 것을 고를지는 뜻이 정한다. 조건을 붙이지 않으면
            // 진단이 "이것이 답이다" 라고 단정하게 된다 — `미해결_심볼` 과 같은 규칙이다.
            for owner in 후보 {
                let 조건 = if 후보.len() > 1 {
                    format!("{owner} 가 같은 개념이라면, ")
                } else {
                    String::new()
                };
                fixes.push(Fix {
                    kind: FixKind::Edit,
                    doc: Some(c.doc.clone()),
                    action: format!(
                        "{조건}import 블록에 다음 줄을 추가하세요: import {}",
                        심볼_주소(owner, &SymbolKind::Exception, 이름, true)
                    ),
                });
            }
        }
        fixes.push(Fix {
            kind: FixKind::Edit,
            doc: Some(c.doc.clone()),
            action: "cover 대상을 이 문서에서 쓸 수 있는 exception 이름으로 고치세요. 가리킬 예외가 없다면 이 cover 줄을 지우세요.".to_string(),
        });

        diagnostics.push(Diagnostic {
            severity: Severity::Error,
            code: "K034",
            message: format!("cover 대상 예외를 찾지 못했습니다 — `{이름}`. {이유}"),
            locations: vec![Location {
                doc: c.doc.clone(),
                line: c.line,
                note: format!(
                    "topic `{}` — 여기서 커버하려는 대상을 찾지 못했습니다.",
                    c.topic
                ),
            }],
            fixes,
        });
    }

    diagnostics
}

/// 스펙 4.8 의 rev 핀을 검사한다.
///
/// | 상황 | 결과 |
/// |---|---|
/// | 핀 없음 | `K020` error — `bless` 가 삽입한다 |
/// | 핀이 대상의 현재 해시와 다름 | `K021` error — 대상을 다시 읽고 갱신한다 |
///
/// **두 규칙에 두 코드를 준다.** 처방은 둘 다 `kang bless` 지만 원인이 다르고, 스펙 5.1.1 은
/// "에이전트가 코드로 분기할 수 있어야 한다" 고 못박는다. 하나로 묶으면 "대상이 바뀌었으니
/// 다시 읽어라" 와 "아직 한 번도 핀을 박지 않았다" 가 같은 코드가 되어, 진단이 둘 중 하나에
/// 대해 거짓을 말하게 된다. `K021` 은 스펙 5.1.1 이 번호까지 못박은 코드다.
///
/// **해시는 [`SymbolTable::hash_source`] 에서 온다.** keyword 는 한 줄 정의, topic 은 본문,
/// exception 은 **그것을 선언한 topic 의 본문**이다 (스펙 4.8). 한 topic 이 예외를 여럿
/// 선언하면 전부 같은 해시를 갖는다 — 맥락이 바뀌었으니 커버 문서가 전부 다시 봐야 한다는
/// 것이 그 규칙의 의도다.
///
/// **대상이 해석되지 않는 import 는 보지 않는다.** 그것은 미해결 import 이고 `K002` 의
/// 몫이다. 여기서 함께 진단하면 같은 사실이 두 번 보고된다.
///
/// **줄마다 진단 하나다.** 핀은 import 줄마다 따로 붙으므로, 한 문서가 같은 대상을 두 번
/// import 하면 한 줄은 핀이 없고 다른 줄은 핀이 틀릴 수 있다. 묶으면 코드를 하나로 골라야
/// 하고 그러면 한쪽에 대해 거짓이 된다. 같은 대상을 여러 번 import 하는 것 자체는
/// `K004`·`K052` 가 따로 잡는다.
///
/// 진단 순서는 **문서 경로 순**이고 한 문서 안에서는 import 줄 순서다.
///
/// # 매개변수
/// - `project`: 파싱을 마친 프로젝트
/// - `table`: [`SymbolTable::build`] 가 만든 전역 심볼 테이블. 대상의 해시 입력을 여기서 얻는다
///
/// # 반환값
/// 핀이 없거나 어긋난 import 줄마다 만들어진 진단들. 어긴 것이 없으면 빈 벡터
pub fn check_revs(project: &Project, table: &SymbolTable) -> Vec<Diagnostic> {
    // HashMap 의 나열 순서는 보장되지 않는다. 정렬해야 진단 순서가 실행마다 같다.
    let mut 순서: Vec<&DocPath> = project.docs.keys().collect();
    순서.sort_by(|a, b| a.0.cmp(&b.0));

    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    for doc in 순서 {
        // 한 문서의 import 를 파일 순서대로 본다.
        for import in &project.docs[doc].imports {
            // 대상이 없으면 물을 해시가 없다. 미해결 import 는 `K002` 의 몫이다.
            // **핀이 없는 줄도 여기서 걸러진다** — 대상이 없는 import 에 "핀을 붙여라" 고
            // 하면 bless 가 할 수 없는 일을 시키게 되고, `K002` 와 같은 사실을 두 번 말한다.
            let Some(id) = table.resolve(&import.target) else {
                continue;
            };

            let 대상 = &import.target;
            let 이름 = 대상.name.join(".");
            let 문서_문법 = 심볼_주소(&대상.doc, &대상.kind, &이름, true);
            // `bless` 는 CLI 이므로 백틱 없는 주소에 인용을 붙인다 (스펙 5.1.1·6.0).
            // 심볼 이름에는 공백이 들어가므로 인용을 빠뜨리면 셸이 인자를 쪼갠다.
            // **alias 가 아니라 대상의 정본 이름**을 넘긴다 — alias 로는 bless 가 그 import
            // 줄을 찾지 못한다.
            let 명령 = || {
                format!(
                    "kang bless {} --import {}",
                    셸_인용(&doc.to_string()),
                    셸_인용(&심볼_주소(&대상.doc, &대상.kind, &이름, false))
                )
            };

            let diagnostic = match &import.rev {
                // 핀이 없다. 손으로 계산할 수 없으므로 `bless` 가 삽입한다 (스펙 4.8).
                // 이것이 스펙 4.8 정본 3단계 레시피의 2단계다 — `K001`·`K030` 의 처방이
                // 만든 핀 없는 import 줄을 여기서 이어받는다.
                None => Diagnostic {
                    severity: Severity::Error,
                    code: "K020",
                    message: format!(
                        "import 에 rev 핀이 없습니다 — {문서_문법}. keyword·topic·exception 세 종류 모두 핀이 필수입니다 (스펙 4.7). 핀은 손으로 계산할 수 없으므로 kang bless 가 삽입합니다 (스펙 4.8)."
                    ),
                    locations: vec![Location {
                        doc: doc.clone(),
                        line: import.line,
                        note: "여기서 import 했습니다 — 이 줄에 rev 핀이 없습니다.".to_string(),
                    }],
                    fixes: vec![Fix {
                        kind: FixKind::Shell,
                        doc: None,
                        action: format!("이 import 에 rev 핀을 붙이세요: {}", 명령()),
                    }],
                },
                Some(핀) => {
                    // ponytail: 같은 대상을 여러 줄이 import 하면 해시를 줄마다 다시
                    // 계산한다. 입력이 한 줄 정의나 topic 본문 하나라 비용이 작아 캐시를
                    // 두지 않는다. 문서가 아주 커지거나 import 가 아주 많아지면
                    // `SymbolId` 별로 한 번만 계산해 재사용하는 형태로 올린다.
                    let 현재 = crate::hash::rev(table.hash_source(id));
                    // 핀이 현재 해시와 같으면 이 import 는 통과다.
                    if *핀 == 현재 {
                        continue;
                    }
                    Diagnostic {
                        severity: Severity::Error,
                        code: "K021",
                        // **"대상이 바뀌었다" 고 단정하지 않는다.** kang 은 이전 본문을
                        // 저장하지 않아 그것을 확인할 수 없고, 핀을 손으로 잘못 적은
                        // 문서에 대해서는 거짓이 된다. 검증되는 사실은 두 해시가 다르다는
                        // 것뿐이므로 그것만 말하고, 나머지는 지시로 적는다.
                        //
                        // diff 를 출력하지 않는다 — 이전 본문이 없으므로 만들 수 없다.
                        // 무엇이 바뀌었는지는 git 이 안다.
                        message: format!(
                            "rev 핀이 대상의 현재 내용과 다릅니다 — {문서_문법}. 핀은 참조 시점의 내용을 가리킵니다 (스펙 4.8). 무엇이 달라졌는지는 프로젝트 루트에서 git diff {} 로 확인하고, 이 문서가 지금의 대상에도 여전히 맞는지 확인한 뒤 핀을 갱신하세요.",
                            셸_인용(&format!("{}.kang", 대상.doc))
                        ),
                        locations: vec![Location {
                            doc: doc.clone(),
                            line: import.line,
                            note: format!("여기서 import 했습니다 — 핀 {핀}, 현재 {현재}."),
                        }],
                        fixes: vec![Fix {
                            kind: FixKind::Shell,
                            doc: None,
                            action: format!(
                                "대상을 다시 읽고 이 문서가 여전히 맞는다면 핀을 갱신하세요: {}",
                                명령()
                            ),
                        }],
                    }
                }
            };
            diagnostics.push(diagnostic);
        }
    }

    diagnostics
}

/// 진단 목록을 사람과 LLM 이 읽을 형태로 만든다 (스펙 5.1.1).
///
/// 세 요소를 항상 찍는다 — 관련 위치 전부, 왜 문제인지 한 문장, 그대로 적용 가능한 `fix`.
///
/// **순서를 다시 정하지 않는다.** 진단 하나가 여러 문서를 가리킬 수 있어(`K012`·`K040`)
/// "이 진단의 문서" 라는 것이 없고, 호출자는 파싱·로드·규칙 진단을 이미 뜻이 있는
/// 순서로 이어 붙인다. 결정성은 진단을 **만드는** 쪽이 문서 경로 순으로 도는 것으로 얻는다.
///
/// # 매개변수
/// - `diags`: 찍을 진단들
///
/// # 반환값
/// 진단마다 한 블록씩 담은 여러 줄 문자열. 진단이 없으면 빈 문자열
pub fn report(diags: &[Diagnostic]) -> String {
    let mut 출력 = String::new();

    // 진단 하나가 한 블록이다. 블록 사이는 빈 줄로 가른다.
    for diagnostic in diags {
        if !출력.is_empty() {
            출력.push('\n');
        }
        let 심각도 = match diagnostic.severity {
            Severity::Error => "error",
            Severity::Warn => "warning",
        };
        출력.push_str(&format!(
            "{심각도}[{}]: {}\n\n",
            diagnostic.code, diagnostic.message
        ));

        // `Location::line` 은 1-based 이므로 `0` 은 "가리킬 줄이 없음" 이다. 그때 `doc` 은
        // 문서 주소가 아니라 표시용 경로이므로 `.kang` 을 붙이면 없는 파일 이름을 지어낸다.
        let 자리들: Vec<String> = diagnostic
            .locations
            .iter()
            .map(|location| {
                if location.line == 0 {
                    location.doc.to_string()
                } else {
                    format!("{}.kang:{}", location.doc, location.line)
                }
            })
            .collect();
        let 폭 = 자리들
            .iter()
            .map(|자리| 자리.chars().count())
            .max()
            .unwrap_or(0);

        // 관련 위치 전부를 찍고 note 를 같은 열에서 시작시킨다.
        for (자리, location) in 자리들.iter().zip(&diagnostic.locations) {
            let 채움 = " ".repeat(폭 - 자리.chars().count());
            출력.push_str(&format!("  {자리}{채움}   {}\n", location.note));
        }

        // fix 가 없는 진단은 만들지 않지만, 찍는 쪽이 그것을 전제로 빈 머리글을 남기면
        // 적용할 것이 있는 것처럼 보인다.
        if diagnostic.fixes.is_empty() {
            continue;
        }
        출력.push_str("\n  fix:\n");
        // `fixes` 는 순서 있는 목록이며 앞에서부터 적용한다 (스펙 5.1.1).
        for fix in &diagnostic.fixes {
            match (&fix.kind, &fix.doc) {
                // 문서 편집은 어느 파일을 여는지부터 보여야 한다.
                (FixKind::Edit, Some(doc)) => {
                    출력.push_str(&format!(
                        "    [edit]  {doc}.kang\n            {}\n",
                        fix.action
                    ));
                }
                // 대상 문서가 없는 편집은 파일 이름을 지어내지 않고 행동만 적는다.
                (FixKind::Edit, None) => 출력.push_str(&format!("    [edit]  {}\n", fix.action)),
                // 셸 명령은 그대로 복사해 실행할 수 있어야 하므로 한 줄로 찍는다.
                (FixKind::Shell, _) => 출력.push_str(&format!("    [shell] {}\n", fix.action)),
            }
        }
    }

    출력
}

/// 문서의 모든 심볼 선언을 훑는다.
///
/// keyword 선언·topic 헤딩·exception 선언 세 자리가 모두 `iknow` 를 받을 수 있고
/// (스펙 4.4) 세 종류 모두 파일 밖으로 노출되므로, 세 곳을 한 번에 도는 자리가 필요하다.
///
/// # 매개변수
/// - `document`: 훑을 문서
///
/// # 반환값
/// keyword 를 먼저, 그 다음 topic 과 그 topic 이 선언한 exception 을 담은 목록
fn 선언_훑기(document: &Document) -> Vec<선언<'_>> {
    let mut 목록: Vec<선언> = Vec::new();

    // keyword 선언. 계층 이름은 `.` 로 이어 전체 이름으로 만든다 (스펙 4.3).
    for keyword in &document.keywords {
        목록.push(선언 {
            doc: &document.path,
            kind: SymbolKind::Keyword,
            name: keyword.name.0.join("."),
            line: keyword.line,
            iknow: &keyword.iknow,
        });
    }

    // topic 과 그 topic 이 선언한 exception.
    for topic in &document.topics {
        목록.push(선언 {
            doc: &document.path,
            kind: SymbolKind::Topic,
            name: topic.name.clone(),
            line: topic.line,
            iknow: &topic.iknow,
        });
        for exception in &topic.exceptions {
            목록.push(선언 {
                doc: &document.path,
                kind: SymbolKind::Exception,
                name: exception.name.clone(),
                line: exception.line,
                iknow: &exception.iknow,
            });
        }
    }

    목록
}

/// 문서의 백틱 참조를 스코프 이름으로 합치고, 그 문서가 실제로 쓴 이름을 모은다.
///
/// **왜 합치는가.** [`SymbolTable::scope`] 의 키는 `"결제수단.카드"` 처럼 `.` 로 이은
/// 전체 이름인데, 파서는 본문의 `` `결제수단`.`카드` `` 를 `"결제수단"` 과 `"카드"`
/// **두 조각**으로 넣는다. 조각을 그대로 조회하면 합법 문서가 미해결 심볼 error 를 받는다.
///
/// **어떻게 합치는가.** 같은 줄의 조각들을 하나의 그룹으로 보고, **모든 조각이 스코프에서
/// 해석되는 분할**을 찾는다. 그런 분할이 있으면 그중 왼쪽부터 가장 길게 잡는 것을 쓴다.
///
/// 왼쪽부터 최장 접두를 확정하고 **조각을 소비하는** 탐욕은 틀린다. 어떤 이름이 최상위이면서
/// 동시에 다른 이름의 하위일 수 있기 때문이다 (스펙 4.3). `주문`·`주문`.`상태`·`상태`.`전이`
/// 가 모두 선언된 문서에서 `` `주문` 은 `상태`.`전이` 를 따른다 `` 는 `주문`.`상태` 를 먼저
/// 잡아 `전이` 를 고아로 만들고, 합법 문서에 미해결 심볼 error 를 낸다.
/// **합치기가 새 이름을 만들지 않는다는 것만으로는 안전이 보장되지 않는다** — 조각을
/// 소비하는 것 자체가 뒤의 조각을 깨뜨린다.
///
/// ponytail: 원문의 `.` 을 보지 않고 스코프만 본다. 전부 해석되는 분할이 **여럿**이면
/// 원문이 어느 쪽인지 알 수 없어 왼쪽 최장을 택한다. 그래서 두 갈래가 남는다 —
/// `` `A` 와 `B` `` 를 `A.B` 로 합쳐 그 자리의 진단을 **놓치고**, 그 조각을 단독으로
/// import 한 줄을 **미사용으로 오인한다.** 뒤쪽은 놓침이 아니라 **거부**다.
/// 둘 다 원문 인접성이 [`crate::ast::Topic::refs`] 에 없어서 생기며, 파서가 그것을 실어
/// 주면 후보를 원문으로 걸러 함께 없앤다. `ast.rs`·`parse.rs` 변경이 필요해 v1 범위 밖이다.
///
/// # 매개변수
/// - `document`: 참조를 모을 문서
/// - `scope`: 그 문서에서 쓸 수 있는 로컬 이름들
///
/// # 반환값
/// - 합친 참조들. 각 항목은 `(이름, 줄 번호, 참조된 자리)` 이며 자리는 진단의 note 에 쓴다
/// - 이 문서가 쓴 이름 집합. 합친 이름과 **이 문서가 선언한 계층 이름의 상위 전부**를 담는다.
///   스펙 4.3 은 하위 키워드를 선언하려면 상위를 "같은 파일에서 정의했거나 import" 하라고
///   **의무로** 못박으므로, 그 상위 import 를 미사용이라 하면 처방대로 고친 문서가 위법이 된다.
///   `cover` 대상과 keyword 상세 마커도 선언부의 백틱이므로 함께 센다 — 스펙 4.6·4.7 의
///   정본 예시가 exception 을 import 해 `cover` 로만 쓴다.
///
///   **합친 이름의 조각을 낱개로 넣지는 않는다.** `` `결제수단`.`카드` `` 는 심볼 하나를
///   가리키지 `카드` 를 따로 쓰지 않으므로, 낱개로 넣으면 무관한 `카드` import 가
///   미사용 검사를 빠져나간다.
///
/// keyword 상세 마커는 **참조 목록에도** 들어간다. `cover` 만 빠진다 — 이유는 아래 주석에.
fn 참조_해석(
    document: &Document,
    scope: &HashMap<String, SymbolId>,
) -> (Vec<(String, usize, &'static str)>, HashSet<String>) {
    let mut 참조: Vec<(String, usize, &'static str)> = Vec::new();
    let mut 사용: HashSet<String> = HashSet::new();
    let 이어붙이기 = |조각: &[(String, usize)]| {
        조각
            .iter()
            .map(|(이름, _)| 이름.as_str())
            .collect::<Vec<&str>>()
            .join(".")
    };

    // keyword 정의와 topic 본문 두 자리를 함께 돈다. 스펙 4.2 는 "본문과 **선언부**의 모든
    // 백틱은 심볼 참조" 이므로 정의 안의 참조도 같은 규칙을 받는다.
    let 자리들 = document
        .keywords
        .iter()
        .map(|keyword| (&keyword.refs, "keyword 정의"))
        .chain(document.topics.iter().map(|topic| (&topic.refs, "본문")));

    for (조각들, 자리) in 자리들 {
        let mut 첫째 = 0;
        // 같은 줄의 조각들을 한 그룹으로 끊어 가며 훑는다.
        while 첫째 < 조각들.len() {
            let 줄 = 조각들[첫째].1;
            // 합칠 수 있는 것은 **같은 줄에서 이어지는** 조각뿐이다. 줄이 바뀌면
            // 원문에서 `.` 로 이어져 있을 수 없다.
            let mut 끝 = 첫째;
            while 끝 + 1 < 조각들.len() && 조각들[끝 + 1].1 == 줄 {
                끝 += 1;
            }
            let 그룹 = &조각들[첫째..=끝];

            // 뒤에서부터 채우는 표. `이음[k]` 는 k 번째 조각부터 끝까지가 **전부** 해석되는
            // 분할이 있을 때 그 자리에서 잡을 길이다. 없으면 None.
            // 뒤를 먼저 알아야 앞에서 "이 길이를 잡으면 나머지가 살아남는가" 를 물을 수 있다.
            let mut 이음: Vec<Option<usize>> = vec![None; 그룹.len() + 1];
            이음[그룹.len()] = Some(0);
            for 자 in (0..그룹.len()).rev() {
                // 긴 이음부터 본다. `A`.`B`.`C` 가 한 이름이면 `A`.`B` 로 끊기지 않아야 한다.
                이음[자] = (1..=그룹.len() - 자).rev().find(|&길이| {
                    이음[자 + 길이].is_some()
                        && scope.contains_key(&이어붙이기(&그룹[자..자 + 길이]))
                });
            }

            // 전부 해석되는 분할이 없으면 탐욕으로 끊어 진단할 자리를 만든다.
            // 이때만 미해결 심볼이 나온다.
            let mut 자 = 0;
            while 자 < 그룹.len() {
                let 길이 = 이음[자].unwrap_or_else(|| {
                    (1..=그룹.len() - 자)
                        .rev()
                        .find(|&길이| scope.contains_key(&이어붙이기(&그룹[자..자 + 길이])))
                        .unwrap_or(1)
                });
                let 이름 = 이어붙이기(&그룹[자..자 + 길이]);
                사용.insert(이름.clone());
                참조.push((이름, 줄, 자리));
                자 += 길이;
            }

            첫째 = 끝 + 1;
        }
    }

    // 하위 키워드를 선언하면 그 **직접 상위**를 쓰는 것이다 (스펙 4.3:91). 본문이 그 이름을
    // 백틱으로 언급하지 않아도 마찬가지다.
    //
    // 조부모까지 세지 않는다. 의무가 직접 상위에만 걸리므로([`계층_상위_검사`]) 조부모를
    // 세면 그것만 import 하고 쓰지 않는 줄의 참인 `K003` 이 사라진다. 연쇄는 각 선언이
    // 자기 직접 상위를 요구하면서 스스로 닫힌다.
    for keyword in &document.keywords {
        // 계층이 아니면 쓰는 상위가 없다. 자기 자신은 선언이지 사용이 아니다.
        let Some(상위) = 직접_상위(&keyword.name.0) else {
            continue;
        };

        // 그 상위를 대주는 import 는 **대상의 정식 이름**으로 찾는다. alias 를 붙이면
        // 스코프에 묶이는 이름은 alias 이므로, 상위 이름만 넣으면 그 import 가
        // 미사용으로 오인된다. 스펙 4.3:91 은 "import **한 것**" 을 허용하고
        // 4.7 은 alias 가 정식 이름을 폐기한다고 하지 않는다.
        //
        // 종류도 [`계층_상위_검사`] 와 같이 본다. 이름만 같은 topic import 는 상위가 될 수
        // 없으므로(스펙 4.1:68) 하위 선언이 그것을 쓰는 것이 아니다.
        //
        // ponytail: 같은 이름을 keyword 로 import 한 줄이 여럿이면 전부 사용으로 센다.
        // 상위를 실제로 대준 것은 하나뿐이므로 나머지의 `K003` 을 놓친다. 어느 줄이
        // 대주었는지는 스코프가 알려 주지 않아, 없애려면 상위 해석을 import 줄 단위로
        // 되돌려야 한다. 그 형태가 실제로 나타나면 그때 올린다.
        for import in &document.imports {
            if import.target.kind == SymbolKind::Keyword && import.target.name[..] == *상위 {
                사용.insert(
                    import
                        .alias
                        .clone()
                        .unwrap_or_else(|| import.target.name.join(".")),
                );
            }
        }
        사용.insert(상위.join("."));
    }

    // `cover` 대상과 keyword 상세 마커는 `refs` 에 담기지 않지만 선언부의 백틱이므로
    // 심볼 참조다 (스펙 4.2). 둘 다 사용으로 세되, 미해결 판정은 상세 마커만 받는다.
    //
    // `cover` 를 미해결로 보지 않는 것은 그것이 가리키는 exception 의 짝 맞추기가
    // 상태 기계의 몫이기 때문이다. 여기서 함께 보면 같은 사실이 두 번 보고된다.
    for topic in &document.topics {
        // 한 topic 이 여러 예외를 커버할 수 있다.
        for (이름, _) in &topic.covers {
            사용.insert(이름.clone());
        }
    }

    // 상세 마커는 미해결 판정을 받는다. 스펙 4.3 은 상위 키워드가 "같은 파일에서
    // 정의했거나 import 한 것" 이어야 한다고 하고 그 둘이 정확히 스코프의 내용이므로,
    // 스코프로 검사하는 것은 합법 입력을 거부할 수 없다. 검사하지 않으면
    // `` #`없는 상세` `` 가 아무 진단 없이 통과한다.
    //
    // **종류는 보지 않는다.** `#` 는 topic 시글이지만 스펙은 "상세 대상이 반드시 topic"
    // 을 명시하지 않았다. 스펙이 정하지 않은 것을 강제하면 그것이 합법 입력 거부가 된다.
    // 스코프 키에는 종류가 없으므로 실재만 본다.
    for keyword in &document.keywords {
        // 상세 topic 은 선택이므로 없는 keyword 가 많다.
        let Some(detail) = &keyword.detail else {
            continue;
        };
        // 상세 마커는 백틱 이름 **하나**다 (파서가 강제한다). 합칠 조각이 없다.
        사용.insert(detail.clone());
        참조.push((detail.clone(), keyword.line, "keyword 상세 마커"));
    }

    (참조, 사용)
}

/// 스코프에 없는 참조를 `K001` 로 보고한다.
///
/// 같은 이름을 여러 자리에서 참조했으면 **진단 하나에 위치를 전부** 담는다.
/// 고칠 곳은 import 한 줄이므로 자리마다 진단을 내면 같은 처방이 여러 번 나온다.
///
/// # 매개변수
/// - `document`: 검사할 문서
/// - `참조`: [`참조_해석`] 이 합친 참조들
/// - `scope`: 그 문서에서 쓸 수 있는 로컬 이름들
/// - `선언들`: 이름별 선언 색인
/// - `diagnostics`: 진단을 모을 곳
fn 미해결_검사(
    document: &Document,
    참조: &[(String, usize, &'static str)],
    scope: &HashMap<String, SymbolId>,
    선언들: &BTreeMap<String, Vec<선언>>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // 등장 순서대로 담는다. HashMap 을 쓰면 진단 순서가 실행마다 뒤집히므로,
    // 순서를 따로 관리하는 대신 순서 있는 자료구조 하나만 쓴다.
    // 한 문서에서 해석되지 않는 이름은 많아야 몇 개라 선형 탐색으로 충분하다.
    let mut 자리들: Vec<(&str, Vec<(usize, &'static str)>)> = Vec::new();

    // 해석되지 않은 참조를 이름별로 모은다.
    for (이름, 줄, 자리) in 참조 {
        if scope.contains_key(이름) {
            continue;
        }
        match 자리들.iter_mut().find(|(본, _)| *본 == 이름) {
            Some((_, 모음)) => 모음.push((*줄, *자리)),
            None => 자리들.push((이름, vec![(*줄, *자리)])),
        }
    }

    // 이름마다 진단 하나를 만든다.
    for (이름, 모음) in 자리들 {
        diagnostics.push(미해결_심볼(
            &document.path,
            이름,
            &모음,
            선언들.get(이름).map_or(&[][..], Vec::as_slice),
        ));
    }
}

/// import 의 대상 실재(`K002`)·사용 여부(`K003`)·이름 개수(`K004`)를 검사한다.
///
/// 세 규칙을 한 순회에 둔 이유는 셋 다 **같은 import 줄**을 훑고, 앞 규칙의 결과가 뒤
/// 규칙의 전제이기 때문이다 — 대상이 없는 import 는 이름이 묶이지 않아 사용 여부를 물을
/// 수 없다.
///
/// # 매개변수
/// - `document`: 검사할 문서
/// - `project`: 파싱을 마친 프로젝트. 대상 문서가 실재하는지 여기서 본다
/// - `table`: 전역 심볼 테이블
/// - `사용`: [`참조_해석`] 이 모은, 이 문서가 실제로 쓴 이름들
/// - `diagnostics`: 진단을 모을 곳
fn import_검사(
    document: &Document,
    project: &Project,
    table: &SymbolTable,
    사용: &HashSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // 같은 대상에 묶인 로컬 이름들. 대상은 등장 순서를 지키려고 Vec 으로 든다.
    let mut 대상별: Vec<(&SymbolRef, Vec<&Import>)> = Vec::new();

    // import 를 파일 순서대로 훑는다.
    for import in &document.imports {
        // 대상이 실재하지 않으면 이름이 묶이지 않았다. 순환 검출도 이 간선을 건너뛰므로
        // (`check_cycles`) 여기서 진단하지 않으면 대상 없는 import 가 조용히 통과한다.
        if table.resolve(&import.target).is_none() {
            diagnostics.push(import_대상_없음(document, import, project));
            continue;
        }

        // alias 가 있으면 그 이름으로, 없으면 대상의 정본 이름으로 묶인다 (스펙 4.7).
        let 이름 = import
            .alias
            .clone()
            .unwrap_or_else(|| import.target.name.join("."));
        // 사용 여부는 문서 전체에서 본다. 한 topic 에서라도 쓰였으면 통과다.
        if !사용.contains(&이름) {
            diagnostics.push(미사용_import(document, import));
        }

        let 같은_대상 = 대상별.iter_mut().find(|(대상, _)| {
            대상.doc == import.target.doc
                && 대상.kind == import.target.kind
                && 대상.name == import.target.name
        });
        match 같은_대상 {
            Some((_, 줄들)) => 줄들.push(import),
            None => 대상별.push((&import.target, vec![import])),
        }
    }

    // 한 심볼에 서로 다른 이름이 둘 이상 붙었는지 본다 (스펙 4.7).
    for (대상, 줄들) in 대상별 {
        let 서로_다른: HashSet<String> = 줄들
            .iter()
            .map(|import| {
                import
                    .alias
                    .clone()
                    .unwrap_or_else(|| import.target.name.join("."))
            })
            .collect();
        if 서로_다른.len() >= 2 {
            diagnostics.push(이름_여럿(document, 대상, &줄들));
        }
    }
}

/// `iknow` 대상 문서와 심볼이 실재하는지 검사한다 (`K010`).
///
/// `iknow` 는 import 가 아니므로 미사용 검사 대상이 아니다. 경로 실재와 상호성만 본다
/// (스펙 4.4). 상호성은 [`이름_충돌_검사`] 가 본다.
///
/// # 매개변수
/// - `document`: 검사할 문서
/// - `project`: 파싱을 마친 프로젝트
/// - `table`: 전역 심볼 테이블
/// - `diagnostics`: 진단을 모을 곳
fn iknow_실재_검사(
    document: &Document,
    project: &Project,
    table: &SymbolTable,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // 세 종류 선언 모두가 iknow 를 받을 수 있다 (스펙 4.4).
    for 하나 in 선언_훑기(document) {
        // 한 선언이 여러 대상을 나열할 수 있다.
        for 대상 in 하나.iknow {
            // 문서도 심볼도 실재해야 한다. 이름이 바뀌면 여기서 잡힌다 (스펙 4.4).
            if table.resolve(대상).is_none() {
                diagnostics.push(iknow_대상_없음(&하나, 대상, project));
            }
        }
    }
}

/// 여러 문서가 같은 이름을 선언했는데 `iknow` 상호 명시가 완전하지 않은지 검사한다 (`K012`).
///
/// **진단 단위는 이름 하나다.** 스펙 5.1.1 의 `K012` 예시가 두 선언을 한 진단의 두 위치로
/// 묶고 수정도 양쪽에 하나씩 낸다. 문서마다 진단을 내면 같은 사실이 N번 나온다.
/// 그래서 3개 파일 중 한 곳만 빠뜨려도 진단은 **하나**이고, 수정만 그 한 곳을 가리킨다.
///
/// 판정은 **전체 경로 기준**이다 — `결제`.`상태` 와 `구독`.`상태` 는 다른 이름이므로
/// 충돌이 아니다 (스펙 4.3). 색인의 키가 이미 전체 경로라 이것이 원천에서 지켜진다.
///
/// # 매개변수
/// - `선언들`: 이름별 선언 색인
/// - `diagnostics`: 진단을 모을 곳
fn 이름_충돌_검사(
    선언들: &BTreeMap<String, Vec<선언>>, diagnostics: &mut Vec<Diagnostic>
) {
    // 이름마다 그 이름을 선언한 문서들을 본다. BTreeMap 이라 이름 순회가 결정적이다.
    for (이름, 선언목록) in 선언들 {
        // 문서마다 대표 선언 하나를 고른다. 한 문서가 같은 이름을 두 번 선언한
        // 경우(`K052`)도 문서 하나로 센다. 상대의 **종류**를 알아야 수정이 옳은 구분
        // 기호를 적으므로 문서 경로가 아니라 선언을 들고 다닌다.
        let mut 대표들: Vec<&선언> = Vec::new();
        for 하나 in 선언목록 {
            if !대표들.iter().any(|이미| 이미.doc == 하나.doc) {
                대표들.push(하나);
            }
        }
        // 한 문서만 선언했으면 충돌이 아니다.
        if 대표들.len() < 2 {
            continue;
        }

        // 문서마다 아직 명시하지 않은 상대를 모은다. N개 파일이면 각자 나머지 N-1개를
        // 전부 명시해야 한다 (스펙 4.4).
        let mut 누락: Vec<(&선언, Vec<&선언>)> = Vec::new();
        for &대표 in &대표들 {
            // 같은 문서의 선언이 여럿이면 iknow 는 합집합으로 본다.
            // 종류는 보지 않는다 — 종류를 잘못 쓴 대상은 `K010` 이 이미 잡으므로,
            // 여기서 또 "빠뜨렸다" 고 하면 명시한 사람에게 거짓을 말하게 된다.
            let 내_iknow: HashSet<&DocPath> = 선언목록
                .iter()
                .filter(|하나| 하나.doc == 대표.doc)
                .flat_map(|하나| 하나.iknow.iter())
                .filter(|대상| 대상.name.join(".") == *이름)
                .map(|대상| &대상.doc)
                .collect();
            let 빠진: Vec<&선언> = 대표들
                .iter()
                .copied()
                .filter(|상대| 상대.doc != 대표.doc && !내_iknow.contains(상대.doc))
                .collect();
            if 빠진.is_empty() {
                continue;
            }
            누락.push((대표, 빠진));
        }

        // 전부 상호 명시했으면 합법이다. 여기서 무조건 진단하면 iknow 를 제대로 쓴
        // 문서가 거부된다.
        if 누락.is_empty() {
            continue;
        }
        diagnostics.push(iknow_불완전(이름, 선언목록, &누락));
    }
}

/// 심볼 주소를 문자열로 만든다.
///
/// 문서와 이름을 잇는 기호가 종류마다 다르다 — keyword 는 `.`, topic 은 `#`,
/// exception 은 `!` 다 (스펙 4.1).
///
/// # 매개변수
/// - `doc`: 심볼이 선언된 문서
/// - `kind`: 심볼의 종류
/// - `name`: 전체 이름. keyword 계층은 `.` 로 이어진 형태다
/// - `백틱`: 참이면 문서 문법(조각마다 백틱), 거짓이면 CLI 문법(백틱 없음). 스펙 5.1.1 은
///   두 문법을 섞지 말라고 못박는다 — `[edit]` 는 문서 문법, `[shell]` 은 CLI 문법이다
///
/// # 반환값
/// 조립된 주소 문자열
fn 심볼_주소(doc: &DocPath, kind: &SymbolKind, name: &str, 백틱: bool) -> String {
    let 감싸기 = |조각: &str| {
        if 백틱 {
            format!("`{조각}`")
        } else {
            조각.to_string()
        }
    };
    let 문서 = doc
        .0
        .iter()
        .map(|조각| 감싸기(조각))
        .collect::<Vec<String>>()
        .join("/");

    match kind {
        // 계층 keyword 는 조각마다 백틱을 두르고 `.` 로 잇는다 (스펙 4.1).
        SymbolKind::Keyword => {
            let 이름 = name
                .split('.')
                .map(&감싸기)
                .collect::<Vec<String>>()
                .join(".");
            format!("{문서}.{이름}")
        }
        SymbolKind::Topic => format!("{문서}#{}", 감싸기(name)),
        SymbolKind::Exception => format!("{문서}!{}", 감싸기(name)),
    }
}

/// 참조 대상을 찾지 못한 이유를 한 문장으로 말한다.
///
/// **문서가 없는 것과 문서 안에 심볼이 없는 것은 다른 사실이다.** 뭉뚱그리면 둘 중
/// 하나에 대해 진단이 거짓을 말한다. 파싱에 실패해 [`Project::docs`] 에서 빠진 문서를
/// "없는 파일" 이라 단정하지 않도록 두 원인을 함께 적는다.
///
/// # 매개변수
/// - `대상`: 찾지 못한 심볼 참조
/// - `project`: 파싱을 마친 프로젝트. 대상 문서의 실재를 여기서 본다
///
/// # 반환값
/// 원인을 설명하는 한 문장
fn 대상_설명(대상: &SymbolRef, project: &Project) -> String {
    if project.docs.contains_key(&대상.doc) {
        // 조사를 이름 뒤에 붙이지 않는다. 이름의 끝소리에 따라 "이/가" 가 갈리는데
        // 컴파일러가 그것을 판정할 수 없어, 붙이면 어느 한쪽에서 반드시 어색해진다.
        format!(
            "문서 {} 에는 그 이름의 {} 선언이 없습니다.",
            대상.doc,
            종류_낱말(&대상.kind)
        )
    } else {
        format!(
            "이 프로젝트에서 문서 {} 를 찾지 못했습니다. 경로가 틀렸거나 그 문서가 컴파일되지 않았습니다.",
            대상.doc
        )
    }
}

/// 계층 keyword 의 상위가 이 문서에 있는지 검사한다 (`K005`).
///
/// 스펙 4.3:91 — "하위 키워드는 `.`로 계층을 표현한다. **상위 키워드는 같은 파일에서
/// 정의했거나 import한 것이어야 한다.**"
///
/// **[`SymbolTable::scope`] 로 판정하지 않는다.** alias 를 붙이면 스코프에 묶이는 이름은
/// alias 이므로, `` import `docs`/`a`.`결제수단` as `수단` `` 뒤의
/// `` keyword `결제수단`.`카드` `` 가 거부된다. 스펙은 "import **한 것**" 을 허용하고 4.7 은
/// alias 가 정식 이름을 폐기한다고 하지 않으므로 그 문서는 합법이다.
/// 그래서 [`Document::keywords`] 의 선언 이름과 [`Document::imports`] 의 **대상 이름**을
/// 직접 본다.
///
/// **요구하는 것은 직접 상위 하나뿐이다.** `A`.`B`.`C` 는 `A`.`B` 만 요구하고 `A` 는
/// 요구하지 않는다. `A`.`B` 가 import 된 것이면 `A` 는 그 파일의 사정이고, 이 파일이
/// 선언한 것이면 그 선언이 자기 차례에 `A` 를 요구받아 연쇄가 스스로 닫힌다.
///
/// **의무는 선언에만 걸린다.** 계층 심볼을 import 하는 것은 남의 파일 심볼을 가져오는
/// 것이므로 상위를 함께 import 할 것을 요구하지 않는다 — 스펙 4.7 에 그런 조항이 없다.
///
/// # 매개변수
/// - `document`: 검사할 문서
/// - `선언들`: 이름별 선언 색인. 상위를 어디서 가져올 수 있는지 알려 주는 데 쓴다
/// - `diagnostics`: 진단을 모을 곳
fn 계층_상위_검사(
    document: &Document,
    선언들: &BTreeMap<String, Vec<선언>>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // 이 문서가 선언한 keyword 를 파일 순서대로 본다.
    for keyword in &document.keywords {
        // 계층이 아니면 요구할 상위가 없다.
        let Some(상위) = 직접_상위(&keyword.name.0) else {
            continue;
        };

        // 같은 파일에서 선언했으면 통과다. 이 가지는 `keywords` 만 보므로 종류가 이미 맞다.
        if document.keywords.iter().any(|다른| 다른.name.0 == *상위) {
            continue;
        }

        // import 했으면 통과다. **`alias` 가 아니라 대상의 정식 이름**으로 본다 —
        // 스펙 4.3:91 은 "import **한 것**" 을 허용하고 4.7 은 alias 가 정식 이름을
        // 폐기한다고 하지 않는다.
        //
        // 종류도 본다. `.` 는 키워드 계층이므로(스펙 4.1:68) topic·exception 은 상위가
        // 될 수 없다. 같은 파일 가지가 이미 keyword 전용이라 이것이 대칭이다.
        // 통과 판정은 **모든** import 를 본다. 이름만 보고 첫 줄을 집어 종류를 물으면,
        // 같은 이름을 topic 과 keyword 로 각각 import 한 문서에서 topic 줄이 먼저 나올 때
        // keyword import 가 있는데도 진단이 난다.
        if document.imports.iter().any(|import| {
            import.target.kind == SymbolKind::Keyword && import.target.name[..] == *상위
        }) {
            continue;
        }

        // 여기까지 왔으면 상위 keyword 는 없다. 그래도 **같은 이름이 이 문서에 이미
        // 묶여 있을** 수 있다 — 다른 종류의 import 이거나 이 파일의 topic·exception 이다.
        // 그때 "찾을 수 없습니다" 는 거짓이고, 수정이 그 이름을 또 묶으면 `K052` 가 난다.
        // 위에서 keyword 가 없음이 확정되었으므로 어느 것을 집어도 참인 종류다.
        let 이름 = 상위.join(".");
        let 다른_종류 = document
            .imports
            .iter()
            .find(|import| import.target.name[..] == *상위)
            .map(|import| 종류_낱말(&import.target.kind))
            .or_else(|| {
                선언_훑기(document)
                    .into_iter()
                    .find(|하나| 하나.name == 이름)
                    .map(|하나| 종류_낱말(&하나.kind))
            });
        diagnostics.push(계층_상위_없음(
            document,
            keyword,
            상위,
            다른_종류,
            선언들,
        ));
    }
}

/// import 가 이 문서에 묶은 이름을 문서 문법으로 적는다.
///
/// alias 는 백틱 **한 쌍**이고, alias 가 없으면 대상의 계층 이름이므로 조각마다 두른다.
/// 둘을 뭉뚱그리면 한쪽이 문서에 없는 문자열을 가리키게 된다.
///
/// # 매개변수
/// - `import`: 그 이름을 묶은 import
///
/// # 반환값
/// 본문에서 그 심볼을 부를 때 실제로 쓰는 표기
fn 로컬_표기(import: &Import) -> String {
    match &import.alias {
        Some(alias) => format!("`{alias}`"),
        None => 백틱_이음(&import.target.name.join(".")),
    }
}

/// 계층 이름을 문서 문법으로 적는다 — 조각마다 백틱을 두르고 `.` 로 잇는다 (스펙 4.1).
///
/// 계층 이름을 백틱 **한 쌍**으로 감싸면 `` `결제수단.카드` `` 라는, 문서 어디에도 없는
/// 문자열을 찾으라고 시키게 된다. `[edit]` 는 문서 문법이므로(스펙 5.1.1) 에이전트가
/// 파일에서 찾을 실제 텍스트와 같아야 한다.
///
/// **alias 에는 쓰지 마라.** alias 는 실제로 백틱 한 쌍이므로 쪼개면 그쪽이 거짓이 된다.
///
/// # 매개변수
/// - `이름`: `.` 로 이어진 전체 이름
///
/// # 반환값
/// 조각마다 백틱을 두르고 `.` 로 이은 문자열
fn 백틱_이음(이름: &str) -> String {
    이름
        .split('.')
        .map(|조각| format!("`{조각}`"))
        .collect::<Vec<String>>()
        .join(".")
}

/// 계층 이름의 **직접 상위**를 돌려준다.
///
/// 스펙 4.3:91 의 의무는 직접 상위 하나에만 걸린다 — `A`.`B`.`C` 는 `A`.`B` 만 요구하고
/// `A` 는 요구하지 않는다. `A`.`B` 가 import 된 것이면 `A` 는 그 파일의 사정이고, 이 파일이
/// 선언한 것이면 그 선언이 자기 차례에 `A` 를 요구받아 연쇄가 스스로 닫힌다.
///
/// 사용 집계와 [`계층_상위_검사`] 가 이것을 함께 쓴다. 둘이 다른 범위를 보면 한쪽이
/// 의무라고 하지 않은 것을 다른 쪽이 사용으로 세어 참인 `K003` 이 사라진다.
///
/// # 매개변수
/// - `조각들`: keyword 이름의 조각들
///
/// # 반환값
/// 계층이면 마지막 조각을 뗀 앞부분. 계층이 아니면 `None`
fn 직접_상위(조각들: &[String]) -> Option<&[String]> {
    조각들
        .split_last()
        .map(|(_, 앞)| 앞)
        .filter(|앞| !앞.is_empty())
}

/// 심볼 종류를 문서에 쓰는 낱말로 바꾼다.
///
/// # 매개변수
/// - `kind`: 심볼의 종류
///
/// # 반환값
/// 선언 문법에 쓰이는 낱말
fn 종류_낱말(kind: &SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Keyword => "keyword",
        SymbolKind::Topic => "topic",
        SymbolKind::Exception => "exception",
    }
}

/// 스코프에 없는 심볼을 참조했다는 진단을 만든다.
///
/// # 매개변수
/// - `doc`: 참조한 문서
/// - `이름`: 해석되지 않은 이름
/// - `자리들`: 그 이름을 참조한 `(줄 번호, 참조된 자리)` 목록. 최소 1개
/// - `선언한_곳`: 프로젝트에서 그 이름을 선언한 자리들. 없으면 빈 슬라이스
///
/// # 반환값
/// `K001` 진단
fn 미해결_심볼(
    doc: &DocPath,
    이름: &str,
    자리들: &[(usize, &'static str)],
    선언한_곳: &[선언],
) -> Diagnostic {
    // 같은 문서가 같은 이름을 두 번 선언했을 수 있으므로 문서 단위로 줄인다.
    let mut 후보: Vec<&선언> = Vec::new();
    for 하나 in 선언한_곳 {
        if !후보.iter().any(|이미| 이미.doc == 하나.doc) {
            후보.push(하나);
        }
    }

    // 개수를 세어 말한다. "하나 있습니다" 라고 해 놓고 둘이면 진단이 거짓을 말한다.
    let 안내 = if 후보.is_empty() {
        "프로젝트 어디에도 이 이름을 선언한 문서가 없습니다. 이름을 잘못 썼거나, 아직 선언하지 않았습니다.".to_string()
    } else {
        let 목록: Vec<String> = 후보.iter().map(|하나| 하나.doc.to_string()).collect();
        format!(
            "같은 이름을 선언한 문서가 {} 개 있습니다: {}. 같은 개념이라면 import 하고, 다른 개념이라면 이 문서에서 선언한 뒤 양쪽에 iknow 를 붙이세요.",
            후보.len(),
            목록.join(", ")
        )
    };

    // 선언한 문서가 없으면 import 할 대상도 없다. 그때는 선언하거나 오타를 고치는 것이
    // 유일한 처방이다.
    let fixes: Vec<Fix> = if 후보.is_empty() {
        vec![Fix {
            kind: FixKind::Edit,
            doc: Some(doc.clone()),
            action: format!(
                "이 문서에서 keyword 로 선언하거나(예: keyword `{이름}`: <한 줄 정의>) 참조의 오타를 고치세요."
            ),
        }]
    } else {
        // 후보마다 "import 를 쓰고 → 핀을 붙인다" 두 수정이 짝을 이룬다. 순서가 뒤집히면
        // bless 가 핀을 붙일 줄이 아직 없다 (스펙 4.8).
        후보
            .iter()
            .flat_map(|하나| {
                let 문서_문법 = 심볼_주소(하나.doc, &하나.kind, &하나.name, true);
                let cli_문법 = 심볼_주소(하나.doc, &하나.kind, &하나.name, false);
                // 후보가 둘 이상이면 어느 것을 고를지는 뜻이 정한다. 조건을 붙이지 않으면
                // 진단이 "이것이 답이다" 라고 단정하게 된다.
                let 조건 = if 후보.len() > 1 {
                    format!("{} 가 같은 개념이라면, ", 하나.doc)
                } else {
                    String::new()
                };
                [
                    Fix {
                        kind: FixKind::Edit,
                        doc: Some(doc.clone()),
                        action: format!(
                            "{조건}import 블록에 다음 줄을 추가하세요: import {문서_문법}"
                        ),
                    },
                    Fix {
                        kind: FixKind::Shell,
                        doc: None,
                        action: format!(
                            "그 import 에 rev 핀을 붙이세요: kang bless {} --import {}",
                            셸_인용(&doc.to_string()),
                            셸_인용(&cli_문법)
                        ),
                    },
                ]
            })
            .collect()
    };

    Diagnostic {
        severity: Severity::Error,
        code: "K001",
        message: format!(
            "선언되지 않은 심볼 — `{이름}`. 이 문서는 이 이름을 선언하지도 import 하지도 않았습니다. {안내}"
        ),
        locations: 자리들
            .iter()
            .map(|(줄, 자리)| Location {
                doc: doc.clone(),
                line: *줄,
                note: format!("{자리}에서 참조했습니다."),
            })
            .collect(),
        fixes,
    }
}

/// 계층 keyword 의 상위가 이 문서에 없다는 진단을 만든다.
///
/// # 매개변수
/// - `document`: 그 선언이 있는 문서
/// - `keyword`: 상위를 갖추지 못한 계층 keyword 선언
/// - `상위`: 요구되는 직접 상위의 이름 조각들. 최소 1개
/// - `다른_종류`: 그 이름이 이 문서에 이미 묶여 있으나 keyword 가 아닐 때 그 종류의 낱말.
///   아예 없으면 `None` — "이 문서에 없다" 와 "종류가 다르다" 는 다른 사실이고
///   뭉뚱그리면 진단이 둘 중 하나에 대해 거짓을 말한다. 이 값이 `Some` 이면 수정이
///   그 이름을 **또 묶지 않도록** 문구가 갈린다 (그러면 `K052` 가 난다)
/// - `선언들`: 이름별 선언 색인. 그 이름을 keyword 로 선언한 문서를 찾는 데 쓴다
///
/// # 반환값
/// `K005` 진단
fn 계층_상위_없음(
    document: &Document,
    keyword: &Keyword,
    상위: &[String],
    다른_종류: Option<&'static str>,
    선언들: &BTreeMap<String, Vec<선언>>,
) -> Diagnostic {
    let 상위_표기 = 백틱_이음(&상위.join("."));

    // 그 이름을 keyword 로 선언한 문서를 찾는다. topic·exception 은 상위가 될 수 없으므로
    // (스펙 4.3 은 "상위 **키워드**") import 를 권할 대상이 아니다.
    let mut 후보: Vec<&선언> = Vec::new();
    for 하나 in 선언들.get(&상위.join(".")).map_or(&[][..], Vec::as_slice) {
        if 하나.kind == SymbolKind::Keyword && !후보.iter().any(|이미| 이미.doc == 하나.doc)
        {
            후보.push(하나);
        }
    }

    // 개수를 세어 말한다. 없는데 "있습니다" 라고 하면 진단이 거짓을 말한다.
    let 안내 = match 후보.first() {
        Some(_) => format!(
            "그 이름을 keyword 로 선언한 문서가 {} 개 있습니다: {}.",
            후보.len(),
            후보
                .iter()
                .map(|하나| 하나.doc.to_string())
                .collect::<Vec<String>>()
                .join(", ")
        ),
        None => "프로젝트 어디에도 그 이름의 keyword 선언이 없습니다.".to_string(),
    };

    Diagnostic {
        severity: Severity::Error,
        code: "K005",
        message: match 다른_종류 {
            // 이름은 있다. 없다고 하면 거짓이다.
            Some(낱말) => format!(
                "계층 keyword 의 상위가 keyword 가 아닙니다 — {상위_표기}. 이 이름은 이 문서에 이미 묶여 있지만 종류가 {낱말} 입니다. `.` 는 키워드 계층이므로 상위는 keyword 여야 합니다 (스펙 4.1·4.3). {안내}"
            ),
            None => format!(
                "계층 keyword 의 상위를 이 문서에서 찾을 수 없습니다 — {상위_표기}. 스펙 4.3 은 상위 키워드가 같은 파일에서 정의했거나 import 한 것이어야 한다고 정합니다. {안내}"
            ),
        },
        locations: vec![Location {
            doc: document.path.clone(),
            line: keyword.line,
            note: format!(
                "여기서 하위 keyword 를 선언했습니다 — {}",
                백틱_이음(&keyword.name.0.join("."))
            ),
        }],
        // 두 갈래는 **배타적**이다. 어느 쪽인지는 뜻이 정하므로 조건을 action 에 담는다.
        // 순서는 이 문서 안에서 끝나는 쪽이 먼저다.
        fixes: vec![
            Fix {
                kind: FixKind::Edit,
                doc: Some(document.path.clone()),
                // 같은 이름이 이미 묶여 있으면 선언만 더하는 것은 `K052` 를 만든다.
                // 스펙 V0001:417 은 에이전트가 fix 를 **그대로 적용**한다고 정한다.
                action: match 다른_종류 {
                    Some(낱말) => format!(
                        "이 개념이 이 문서의 것이라면, 같은 이름을 묶고 있는 그 {낱말} 줄을 먼저 지운 뒤 상위를 여기서 선언하세요: keyword {상위_표기}: <한 줄 정의>"
                    ),
                    None => format!(
                        "이 개념이 이 문서의 것이라면, 상위를 여기서 선언하세요: keyword {상위_표기}: <한 줄 정의>"
                    ),
                },
            },
            Fix {
                kind: FixKind::Edit,
                doc: Some(document.path.clone()),
                action: match 후보.first() {
                    // 이름을 이미 다른 종류로 import 했다면 그 줄을 바꾸라고 말해야 한다.
                    // "추가하세요" 라고만 하면 같은 이름이 두 번 묶여 `K052` 가 난다.
                    Some(하나) if 다른_종류.is_some() => format!(
                        "상위 keyword 가 {} 의 것이라면, 지금의 import 줄을 그 keyword 로 바꾸세요: import {}",
                        하나.doc,
                        심볼_주소(하나.doc, &하나.kind, &상위.join("."), true)
                    ),
                    Some(하나) => format!(
                        "상위가 {} 의 것이라면, import 블록에 다음 줄을 추가하세요: import {}",
                        하나.doc,
                        심볼_주소(하나.doc, &하나.kind, &상위.join("."), true)
                    ),
                    None if 다른_종류.is_some() => "상위가 다른 문서의 것이라면, 그 문서에서 그것을 keyword 로 선언한 뒤, 같은 이름을 묶고 있는 이 문서의 줄을 그 import 로 바꾸세요."
                        .to_string(),
                    None => "상위가 다른 문서의 것이라면, 그 문서에서 그것을 keyword 로 선언한 뒤 이 문서의 import 블록에서 가져오세요."
                        .to_string(),
                },
            },
        ],
    }
}

/// import 대상이 실재하지 않는다는 진단을 만든다.
///
/// 문서가 없는 것과 문서 안에 심볼이 없는 것은 다른 사실이고 고치는 법도 다르다.
/// 둘을 뭉뚱그리면 진단이 검증되지 않는 것을 말하게 된다.
///
/// # 매개변수
/// - `document`: import 를 쓴 문서
/// - `import`: 대상을 찾지 못한 import
/// - `project`: 파싱을 마친 프로젝트. 대상 문서의 실재를 여기서 본다
///
/// # 반환값
/// `K002` 진단
fn import_대상_없음(document: &Document, import: &Import, project: &Project) -> Diagnostic {
    let 대상 = &import.target;
    let 문서_문법 = 심볼_주소(&대상.doc, &대상.kind, &대상.name.join("."), true);

    // 문서 자체가 없으면 경로를, 심볼이 없으면 이름을 고쳐야 한다. 처방이 갈린다.
    let 처방 = if project.docs.contains_key(&대상.doc) {
        format!("이 import 줄의 대상 이름을 고치거나, 대상 문서에 다음을 선언하세요: {문서_문법}")
    } else {
        "이 import 줄의 대상 경로를 실재하는 문서로 고치세요. 그 문서가 컴파일에 실패했다면 먼저 그쪽 진단을 고치세요.".to_string()
    };

    Diagnostic {
        severity: Severity::Error,
        code: "K002",
        message: format!(
            "import 대상을 찾지 못했습니다 — {문서_문법}. {}",
            대상_설명(대상, project)
        ),
        locations: vec![Location {
            doc: document.path.clone(),
            line: import.line,
            note: "여기서 import 했습니다.".to_string(),
        }],
        fixes: vec![Fix {
            kind: FixKind::Edit,
            doc: Some(document.path.clone()),
            action: 처방,
        }],
    }
}

/// import 했으나 쓰지 않았다는 진단을 만든다.
///
/// # 매개변수
/// - `document`: import 를 쓴 문서
/// - `import`: 쓰이지 않은 import
///
/// # 반환값
/// `K003` 진단
fn 미사용_import(document: &Document, import: &Import) -> Diagnostic {
    // 이 문서에서 그 심볼을 부를 때 **실제로 쓰는 표기**다. alias 는 백틱 한 쌍이고,
    // alias 가 없으면 대상의 계층 이름이므로 조각마다 백틱을 두른다 (스펙 4.1).
    // 계층 이름을 한 쌍으로 감싸면 문서에 없는 문자열을 찾으라고 시키게 된다.
    let 표기 = 로컬_표기(import);

    Diagnostic {
        severity: Severity::Error,
        code: "K003",
        message: format!(
            "import 했으나 이 문서의 어느 곳에서도 쓰지 않았습니다 — {표기}. 쓰지 않는 import 는 문서가 무엇을 전제하는지 흐립니다."
        ),
        locations: vec![Location {
            doc: document.path.clone(),
            line: import.line,
            note: format!("여기서 import 했고, 이 문서에서 부르는 이름은 {표기} 입니다."),
        }],
        fixes: vec![Fix {
            kind: FixKind::Edit,
            doc: Some(document.path.clone()),
            action: format!(
                "이 import 줄을 지우세요. 이 문서가 실제로 그 개념을 쓴다면, 쓰는 자리에서 다음 표기로 참조하세요: {표기}"
            ),
        }],
    }
}

/// 한 심볼에 이름이 둘 이상 붙었다는 진단을 만든다.
///
/// # 매개변수
/// - `document`: import 를 쓴 문서
/// - `대상`: 여러 이름이 붙은 심볼
/// - `줄들`: 그 심볼을 가리키는 import 줄들. 최소 2개
///
/// # 반환값
/// `K004` 진단
fn 이름_여럿(document: &Document, 대상: &SymbolRef, 줄들: &[&Import]) -> Diagnostic {
    let 문서_문법 = 심볼_주소(&대상.doc, &대상.kind, &대상.name.join("."), true);

    Diagnostic {
        severity: Severity::Error,
        code: "K004",
        message: format!(
            "한 심볼이 이 문서에서 서로 다른 이름 여럿으로 묶였습니다 — {문서_문법}. 하나의 개념이 여러 이름을 갖는 것을 막습니다 (스펙 4.7)."
        ),
        // import 줄 전부가 관련 위치다. 하나만 보여 주면 나머지를 찾아 헤맨다.
        locations: 줄들
            .iter()
            .map(|import| Location {
                doc: document.path.clone(),
                line: import.line,
                // 로컬 이름도 문서 문법으로 적는다. alias 면 한 쌍, 아니면 조각마다.
                note: format!("여기서 묶은 이름: {}", 로컬_표기(import)),
            })
            .collect(),
        fixes: vec![Fix {
            kind: FixKind::Edit,
            doc: Some(document.path.clone()),
            action: "이 심볼을 가리킬 이름을 하나로 정하고, 나머지 import 줄을 지운 뒤 그 이름을 쓰던 참조도 함께 고치세요.".to_string(),
        }],
    }
}

/// `iknow` 대상이 실재하지 않는다는 진단을 만든다.
///
/// # 매개변수
/// - `하나`: iknow 를 붙인 선언
/// - `대상`: 찾지 못한 iknow 대상
/// - `project`: 파싱을 마친 프로젝트. 대상 문서의 실재를 여기서 본다
///
/// # 반환값
/// `K010` 진단
fn iknow_대상_없음(하나: &선언, 대상: &SymbolRef, project: &Project) -> Diagnostic {
    let 문서_문법 = 심볼_주소(&대상.doc, &대상.kind, &대상.name.join("."), true);

    Diagnostic {
        severity: Severity::Error,
        code: "K010",
        message: format!(
            "iknow 대상을 찾지 못했습니다 — {문서_문법}. {}",
            대상_설명(대상, project)
        ),
        locations: vec![Location {
            doc: 하나.doc.clone(),
            line: 하나.line,
            note: format!(
                "여기 {} 선언의 iknow 가 그 대상을 가리킵니다 — `{}`",
                종류_낱말(&하나.kind),
                하나.name
            ),
        }],
        fixes: vec![Fix {
            kind: FixKind::Edit,
            doc: Some(하나.doc.clone()),
            action: format!(
                "이 선언의 iknow 대상 주소를 실재하는 심볼로 고치세요. 그 대상이 사라졌다면 iknow 목록에서 다음을 지우세요: {문서_문법}"
            ),
        }],
    }
}

/// `iknow` 상호 명시가 완전하지 않다는 진단을 만든다.
///
/// # 매개변수
/// - `이름`: 여러 문서가 선언한 이름
/// - `선언목록`: 그 이름을 선언한 자리 전부. 최소 2개
/// - `누락`: 빠뜨린 문서마다 `(그 문서의 대표 선언, 아직 명시하지 않은 상대의 선언들)`.
///   상대의 **선언**이어야 한다 — 주소의 구분 기호가 상대 심볼의 종류로 갈리므로
///   자기 종류로 적으면 그대로 적용한 문서가 `K010` 을 맞는다. 최소 1개
///
/// # 반환값
/// `K012` 진단
fn iknow_불완전(
    이름: &str, 선언목록: &[선언], 누락: &[(&선언, Vec<&선언>)]
) -> Diagnostic {
    let 표기 = 백틱_이음(이름);

    Diagnostic {
        severity: Severity::Error,
        code: "K012",
        message: format!(
            "같은 이름의 심볼이 여러 파일에서 선언됨 — {표기}. iknow 상호 명시가 완전하지 않습니다. 다른 뜻이라면 각 선언에 iknow 를 붙여 상호 명시하고, 같은 뜻이라면 한쪽을 지우고 다른 쪽을 import 하세요."
        ),
        // 선언한 자리 전부가 관련 위치다 (스펙 5.1.1).
        locations: 선언목록
            .iter()
            .enumerate()
            .map(|(자리, 하나)| Location {
                doc: 하나.doc.clone(),
                line: 하나.line,
                // 첫 자리에만 "여기서", 나머지는 "여기서도" 라고 말한다. 어느 쪽이 원인인지는
                // 컴파일러가 알 수 없으므로 순서를 주장하지 않는다.
                note: if 자리 == 0 {
                    format!("여기서 선언했습니다 — {} {표기}", 종류_낱말(&하나.kind))
                } else {
                    format!("여기서도 선언했습니다 — {} {표기}", 종류_낱말(&하나.kind))
                },
            })
            .collect(),
        // 고칠 곳은 **빠뜨린 문서뿐**이다. 이미 상호 명시한 문서에까지 수정을 내면
        // 진단이 하지 않은 잘못을 탓하게 된다.
        fixes: 누락
            .iter()
            .map(|(대표, 빠진)| {
                let 목록: Vec<String> = 빠진
                    .iter()
                    // 주소는 **상대의** 문서와 종류로 짓는다. 자기 종류를 쓰면 구분 기호가
                    // 뒤바뀌어(`.` ↔ `#` ↔ `!`) 그대로 적용한 문서가 `K010` 을 맞는다.
                    .map(|상대| 심볼_주소(상대.doc, &상대.kind, 이름, true))
                    .collect();
                let 낱말 = 종류_낱말(&대표.kind);
                // 이미 iknow 를 쓴 문서에게 "추가하세요" 라고 하면 줄을 하나 더 만들게 된다.
                let 이미_있음 = 대표.iknow.iter().any(|대상| 대상.name.join(".") == 이름);
                Fix {
                    kind: FixKind::Edit,
                    doc: Some(대표.doc.clone()),
                    action: if 이미_있음 {
                        format!(
                            "{낱말} {표기} 선언의 iknow 목록에 다음을 덧붙이세요: {}",
                            목록.join(", ")
                        )
                    } else {
                        format!(
                            "{낱말} {표기} 선언 줄 끝에 다음을 추가하세요: // iknow {}",
                            목록.join(", ")
                        )
                    },
                }
            })
            .collect(),
    }
}
