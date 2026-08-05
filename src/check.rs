//! import 그래프의 순환을 검출하는 층.
//!
//! [`crate::resolve`] 가 문서를 다 읽어 온 뒤, 문서들이 서로를 전제로 삼아 고리를
//! 이루는지 본다. 순환이 없어야 어느 문서가 상위 정책인지 정할 수 있다 (스펙 5.3).
//!
//! 이 모듈이 내는 진단 코드는 순환 대역인 `K040`-`K049` 를 쓴다.
//!
//! | 코드 | 규칙 |
//! |---|---|
//! | `K040` | import 그래프에 순환이 있음 |
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

use crate::ast::{Diagnostic, DocPath, Fix, FixKind, Location, Severity};
use crate::resolve::Project;
use std::collections::HashSet;

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
