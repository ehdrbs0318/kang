//! `kang show` 의 뷰 조립층 (스펙 6.4).
//!
//! kang 의 창립 원칙은 "LLM 은 원본을 보지 않는다" 이고, 그 원칙은 `show` 가 `cat` 보다
//! 쓸모 있을 때만 성립한다. `cat` 으로 읽은 `.kang` 은 import 간접 참조 때문에 마크다운보다
//! 읽기 나쁘므로, 이 모듈이 하는 일은 **간접 참조를 전부 펼쳐 완결된 뷰 하나로 만드는 것**이다.
//!
//! 세 가지를 펼친다.
//!
//! - 정의한 키워드와 **그것을 참조하는 topic 전부** (`referencedBy`)
//! - 정의한 예외의 **커버 본문**과, 커버하는 예외의 **선언 맥락**
//! - 각 topic 이 참조하는 키워드와 topic 을 **재귀적으로** (스펙 6.4)
//!
//! **중복 제거가 이 펼침을 유한하게 만든다.** 같은 심볼이 여러 경로로 도달되면 최초 1회만
//! 전개하고 이후에는 경로 문자열로 대체한다. 다이아몬드에서 본문이 지수로 불어나는 것과,
//! 자기 자신을 참조하는 topic 에서 무한 재귀가 되는 것을 같은 장치가 막는다.
//!
//! **깊이 제한은 두지 않는다** (스펙 6.4). 읽기 시점 옵션은 LLM 에게 선택권을 주는 것이고,
//! 손댈 때는 "이 문서가 참조하는 정책이 너무 많다" 는 **빌드 시점 구조 린트**여야 한다.
//! 임계값은 도그푸딩에서 나온다.

use crate::ast::{DocPath, Exception, Keyword, SymbolKind, SymbolRef, Topic};
use crate::check;
use crate::resolve::{Project, SymbolId, SymbolTable};
use crate::yaml::Emitter;
use kang_macros as kang;
use std::collections::HashSet;

/// 조회 대상. 문서 전체이거나 그 안의 topic 하나다.
pub enum ShowTarget {
    /// 문서 전체.
    Document(DocPath),
    /// 문서 안의 topic 하나. 두 번째 값이 topic 이름이다.
    Topic(DocPath, String),
}

/// 스펙 6.4 의 YAML 을 만든다.
///
/// **두 뷰의 키가 다르다.** 문서 뷰는 `path`·`keywords`·`referencingKeywords`·
/// `exceptions`·`covers`·`topics` 를 모두 가질 수 있고, topic 뷰는 그중
/// `keywords` 와 `referencingKeywords` 를 갖지 않는다. 그 둘은 문서 단위 사실이며
/// (키워드는 topic 이 아니라 파일이 소유한다, 스펙 4.3), topic 이 실제로 쓰는 키워드는
/// `topics[0].references.keywords` 에 이미 전개되어 있다. `exceptions` 와 `covers` 는
/// topic 이 선언하는 것이므로(스펙 4.6) 그 topic 의 것으로 좁혀 함께 낸다.
///
/// 값이 빈 절은 생략한다 — 스펙 6.4 가 `covers` 에 대해 그렇게 정했고, 빈 절을 남기면
/// 조회 결과의 절반이 빈 목록으로 채워진다.
///
/// # 매개변수
/// - `project`: 컴파일을 통과한 프로젝트
/// - `table`: 전역 심볼 테이블
/// - `target`: 조회 대상
///
/// # 반환값
/// 스펙 6.4 의 YAML 텍스트. **마지막 개행이 없다** — 호출자의 `writeln!` 이 붙인다.
/// 대상 문서가 프로젝트에 없으면 빈 문자열 (호출자가 이미 걸러야 한다)
#[kang::topic(
    "docs/adr/0002-flatten-on-read-import-on-write#읽을 때는 평탄화, 쓸 때는 import",
    rev = "622fe2"
)]
pub fn show(project: &Project, table: &SymbolTable, target: &ShowTarget) -> String {
    let (path, 토픽_이름) = match target {
        ShowTarget::Document(path) => (path, None),
        ShowTarget::Topic(path, 이름) => (path, Some(이름.as_str())),
    };

    // 없는 문서는 호출자가 사용법 오류로 거절한다. 여기서 패닉하면 문서 컴파일러가
    // 사용자 문서를 앞에 두고 죽는다.
    let Some(document) = project.docs.get(path) else {
        return String::new();
    };

    let 뷰 = 뷰::새로(project, table);
    let mut 방문: HashSet<String> = HashSet::new();
    let mut out = Emitter::new();

    // topic 뷰는 그 topic 하나로 좁힌다. 문서 뷰는 전부다.
    let 대상_토픽: Vec<&Topic> = document
        .topics
        .iter()
        .filter(|topic| 토픽_이름.is_none_or(|이름| topic.name == 이름))
        .collect();

    out.pair(
        "path",
        &토픽_이름.map_or_else(|| path.to_string(), |이름| format!("{path}#{이름}")),
    );

    // **최상위에서 펼칠 것을 먼저 방문으로 선점한다.** 그러지 않으면 본문 참조가 형제
    // topic 을 먼저 전개해 버리고, 최상위 자리가 경로 참조로 줄어 문서 자신의 본문이
    // 출력에서 사라진다.
    for topic in &대상_토픽 {
        방문.insert(format!("{path}#{}", topic.name));
    }

    // 키워드는 topic 이 아니라 파일이 소유하므로 문서 뷰에만 나온다 (스펙 4.3).
    if 토픽_이름.is_none() {
        for keyword in &document.keywords {
            방문.insert(format!("{path}.{}", keyword.name.0.join(".")));
        }

        if !document.keywords.is_empty() {
            let 항목들: Vec<Emitter> = document
                .keywords
                .iter()
                .map(|keyword| 뷰.키워드_항목(path, keyword))
                .collect();
            out.seq("keywords", 항목들);
        }

        // 이 파일이 참조하는 키워드. 재귀 임베드와 달리 import 선언을 그대로 훑는다 —
        // 어떤 개념을 남에게서 빌려 왔는지가 곧 import 목록이다 (스펙 4.7).
        let mut 가져온: Vec<Emitter> = Vec::new();
        for import in &document.imports {
            // keyword 만 이 절의 대상이다. topic 과 exception 은 다른 절들이 담는다.
            if import.target.kind != SymbolKind::Keyword {
                continue;
            }
            // 대상이 없는 import 는 미해결 심볼이며 compile() 이 이미 거절했다.
            let Some(id) = table.resolve(&import.target) else {
                continue;
            };
            if let Some(자리) = 뷰.찾기(id)
                && let 선언::키워드(keyword) = 자리.선언
            {
                let mut e = Emitter::new();
                e.pair("name", &keyword.name.0.join("."));
                e.pair("path", &자리.doc.to_string());
                e.pair("description", &keyword.definition);
                가져온.push(e);
            }
        }
        if !가져온.is_empty() {
            out.seq("referencingKeywords", 가져온);
        }
    }

    // 이 파일(또는 topic)이 정의한 예외와 그 커버 내용.
    //
    // **이 두 절은 방문 집합에 참여하지 않는다.** 여기서 붙는 본문은 항목마다 정확히
    // 하나이고 재귀하지 않으므로, 방문 집합이 막는 폭발(다이아몬드·순환)이 일어나지 않는다.
    // 반대로 참여시키면 **얕고 재귀하지 않는 이 절이 주소를 선점해**, 아래 `topics` 의
    // 재귀 임베드 트리가 통째로 포인터 한 줄로 사라진다.
    //
    // 덧붙여 `covers` 항목은 **예외**로 주소가 붙으므로(`name` 과 `path`) topic 주소로
    // 가리켜도 닿을 곳이 없다. `exceptions` 는 `coveredBy` 로 topic 주소를 이미 찍으므로
    // 그 항목에는 해당하지 않는다.
    let mut 예외들: Vec<Emitter> = Vec::new();
    for topic in &대상_토픽 {
        for exception in &topic.exceptions {
            예외들.push(뷰.예외_항목(path, exception));
        }
    }
    if !예외들.is_empty() {
        out.seq("exceptions", 예외들);
    }

    // 이 파일(또는 topic)이 커버하는 예외와 그 선언 맥락.
    let mut 커버들: Vec<Emitter> = Vec::new();
    for topic in &대상_토픽 {
        for (이름, _) in &topic.covers {
            if let Some(항목) = 뷰.커버_항목(path, 이름) {
                커버들.push(항목);
            }
        }
    }
    if !커버들.is_empty() {
        out.seq("covers", 커버들);
    }

    // 최상위 topic 은 이름을 그대로 쓴다. 소속 문서는 위의 `path` 가 이미 말했다.
    let mut 토픽들: Vec<Emitter> = Vec::new();
    for topic in &대상_토픽 {
        토픽들.push(뷰.토픽_항목(&topic.name, path, topic, &mut 방문));
    }
    if !토픽들.is_empty() {
        out.seq("topics", 토픽들);
    }

    out.finish()
}

/// 심볼 식별자 하나가 가리키는 선언.
///
/// exception 은 본문이 없고 의미가 **선언한 topic 의 맥락**에서 나오므로(스펙 4.8)
/// 그 topic 을 함께 든다.
#[derive(Clone, Copy)]
enum 선언<'a> {
    /// 키워드 선언.
    키워드(&'a Keyword),
    /// topic 선언.
    토픽(&'a Topic),
    /// 예외 선언과 그것을 선언한 topic.
    예외(&'a Exception, &'a Topic),
}

/// 심볼 식별자와 그것이 가리키는 선언, 그리고 선언이 있는 문서.
struct 선언자리<'a> {
    /// 전역 심볼 식별자.
    id: SymbolId,
    /// 이 선언이 있는 문서.
    doc: &'a DocPath,
    /// 선언 그 자체.
    선언: 선언<'a>,
}

/// 뷰를 조립하는 동안 쓰는 색인.
///
/// [`SymbolTable`] 은 식별자로부터 **종류와 이름을 돌려주지 않고** [`SymbolId`] 는
/// 해시할 수 없으므로, 되찾아야 하는 것을 선형 목록으로 들고 다닌다.
///
/// ponytail: 조회가 선형 탐색이라 전체가 O(심볼 × 참조) 다. 여기에 [`뷰::커버하는_topic`]
/// 의 O(예외 × 문서) 가 더 붙는다 — 예외마다 프로젝트의 topic 을 전부 훑으며 스코프를 뜬다.
/// 1600문서·예외 60개에서 0.146s 라 v1 에서는 무해하다. 실측이 이 둘 중 하나를 지목하면
/// `resolve` 에 식별자 기반 조회를 열어 색인 자체를 없애고, 커버는 한 번의 순회로
/// `(예외 → 커버 topic)` 표를 만들어 색인에 함께 싣는다.
struct 뷰<'a> {
    /// 컴파일을 통과한 프로젝트.
    project: &'a Project,
    /// 전역 심볼 테이블.
    table: &'a SymbolTable,
    /// 프로젝트의 모든 선언. 문서 경로 순, 문서 안에서는 선언 순이다.
    선언들: Vec<선언자리<'a>>,
    /// `(참조된 심볼, 참조한 topic 의 전체 주소)`. 같은 짝은 한 번만 들어 있다.
    참조들: Vec<(SymbolId, String)>,
}

impl<'a> 뷰<'a> {
    /// 프로젝트를 훑어 색인을 만든다.
    ///
    /// # 매개변수
    /// - `project`: 컴파일을 통과한 프로젝트
    /// - `table`: 전역 심볼 테이블
    ///
    /// # 반환값
    /// 선언 색인과 참조 색인을 갖춘 뷰
    fn 새로(project: &'a Project, table: &'a SymbolTable) -> Self {
        // HashMap 의 나열 순서는 보장되지 않는다. 정렬해야 출력이 실행마다 같다.
        let 순서 = project.경로들();

        let mut 선언들: Vec<선언자리<'a>> = Vec::new();
        let mut 참조들: Vec<(SymbolId, String)> = Vec::new();

        // 문서를 경로 순으로 훑으며 선언과 참조를 함께 모은다.
        for doc in 순서 {
            let document = &project.docs[doc];
            // 스코프는 문서마다 한 번만 뜬다. `scope` 는 매 호출 매핑을 복제한다.
            let scope = table.scope(doc);

            // 키워드 선언.
            for keyword in &document.keywords {
                if let Some(id) = table.resolve(&SymbolRef {
                    doc: doc.clone(),
                    kind: SymbolKind::Keyword,
                    name: keyword.name.0.clone(),
                }) {
                    선언들.push(선언자리 {
                        id,
                        doc,
                        선언: 선언::키워드(keyword),
                    });
                }
            }

            // topic 선언과 그 안의 예외 선언, 그리고 그 topic 이 가리키는 참조.
            for topic in &document.topics {
                if let Some(id) = table.resolve(&SymbolRef {
                    doc: doc.clone(),
                    kind: SymbolKind::Topic,
                    name: vec![topic.name.clone()],
                }) {
                    선언들.push(선언자리 {
                        id,
                        doc,
                        선언: 선언::토픽(topic),
                    });
                }

                for exception in &topic.exceptions {
                    if let Some(id) = table.resolve(&SymbolRef {
                        doc: doc.clone(),
                        kind: SymbolKind::Exception,
                        name: vec![exception.name.clone()],
                    }) {
                        선언들.push(선언자리 {
                            id,
                            doc,
                            선언: 선언::예외(exception, topic),
                        });
                    }
                }

                // **분할은 진단을 내는 층과 같은 함수를 쓴다.** 두 층이 같은 문장을 다르게
                // 읽으면 빌드가 통과하는 문서에서 조회가 조용히 틀린 답을 낸다.
                let 주소 = format!("{doc}#{}", topic.name);
                for (이름, _) in check::이름_분할(&topic.refs, &scope) {
                    // 스코프에 없는 이름은 미해결 심볼이며 compile() 이 이미 거절했다.
                    let Some(&id) = scope.get(&이름) else {
                        continue;
                    };
                    // 한 topic 이 같은 심볼을 여러 번 가리켜도 참조처는 한 번만 센다.
                    if !참조들
                        .iter()
                        .any(|(기존, 자리)| *기존 == id && 자리 == &주소)
                    {
                        참조들.push((id, 주소.clone()));
                    }
                }
            }
        }

        뷰 {
            project,
            table,
            선언들,
            참조들,
        }
    }

    /// 식별자가 가리키는 선언 자리를 찾는다.
    ///
    /// # 매개변수
    /// - `id`: 찾을 심볼 식별자
    ///
    /// # 반환값
    /// 그 식별자의 선언 자리. 색인에 없으면 `None`
    fn 찾기(&self, id: SymbolId) -> Option<&선언자리<'a>> {
        self.선언들.iter().find(|자리| 자리.id == id)
    }

    /// 심볼의 전체 주소를 만든다 (스펙 6.0).
    ///
    /// # 매개변수
    /// - `id`: 주소를 물을 심볼
    ///
    /// # 반환값
    /// `docs/A.결제`·`docs/A#결제의 방법`·`docs/A!무료 결제` 꼴의 주소.
    /// 색인에 없으면 `None`
    fn 주소(&self, id: SymbolId) -> Option<String> {
        let 자리 = self.찾기(id)?;
        let doc = 자리.doc;
        Some(match 자리.선언 {
            선언::키워드(keyword) => format!("{doc}.{}", keyword.name.0.join(".")),
            선언::토픽(topic) => format!("{doc}#{}", topic.name),
            선언::예외(exception, _) => format!("{doc}!{}", exception.name),
        })
    }

    /// 키워드 항목 하나를 만든다.
    ///
    /// **`path` 를 함께 담는다.** 스펙 6.4 의 예시는 `references` 안에서 이름만 보이지만,
    /// 중복 제거된 두 번째 도달은 `docs/A.결제` 라는 **전체 경로**로 대체되므로 그것이
    /// 가리키는 자리를 찾을 수 있어야 한다. 이름만 있으면 같은 이름을 선언한 다른 문서와
    /// 구별되지 않는다 (`iknow` 로 합법이다, 스펙 4.4).
    ///
    /// # 매개변수
    /// - `owner`: 이 키워드를 선언한 문서
    /// - `keyword`: 담을 키워드
    ///
    /// # 반환값
    /// 이름·경로·정의와, 있다면 상세 topic 과 참조처를 담은 이미터
    fn 키워드_항목(&self, owner: &DocPath, keyword: &Keyword) -> Emitter {
        let 이름 = keyword.name.0.join(".");
        let mut e = Emitter::new();
        e.pair("name", &이름);
        e.pair("path", &owner.to_string());
        e.pair("description", &keyword.definition);

        // 상세 topic 은 선택이다 (스펙 4.3). 있으면 전체 경로로 담는다 — 파싱만 하고
        // 버리면 조회한 쪽이 상세 설명에 닿을 길이 없다.
        //
        // **해석되지 않으면 키를 내지 않는다.** `{owner}#{detail}` 로 지어내면 없는 주소를
        // 사실처럼 말하게 된다. 미해결 상세 마커는 `K001` 이 이미 잡으므로(`check.rs` 의
        // `참조.push(.. "keyword 상세 마커")`) 컴파일을 통과한 문서에서는 늘 해석된다.
        if let Some(detail) = &keyword.detail
            && let Some(주소) = self
                .table
                .scope(owner)
                .get(detail)
                .and_then(|&id| self.주소(id))
        {
            e.pair("detail", &주소);
        }

        // 이 키워드를 참조하는 topic 전부. `kang refs` 가 답하는 것과 같은 질문이며
        // 같은 색인에서 나온다.
        let 참조처: Vec<Emitter> = match self.table.resolve(&SymbolRef {
            doc: owner.clone(),
            kind: SymbolKind::Keyword,
            name: keyword.name.0.clone(),
        }) {
            Some(id) => self
                .참조들
                .iter()
                .filter(|(기존, _)| *기존 == id)
                .map(|(_, 주소)| Emitter::value(주소))
                .collect(),
            None => Vec::new(),
        };
        // 아무도 참조하지 않는 키워드가 빈 목록을 달고 다닐 이유가 없다.
        if !참조처.is_empty() {
            e.seq("referencedBy", 참조처);
        }

        e
    }

    /// topic 항목 하나를 만든다. 참조한 topic 을 재귀적으로 임베드한다.
    ///
    /// # 매개변수
    /// - `이름`: 항목에 적을 이름. 최상위는 topic 이름, 참조 자리는 전체 주소다
    /// - `doc`: 이 topic 이 있는 문서
    /// - `topic`: 담을 topic
    /// - `방문`: 이미 전개한 심볼의 전체 주소 집합
    ///
    /// # 반환값
    /// 이름·`uncoded`·본문과, 있다면 참조를 담은 이미터
    fn 토픽_항목(
        &self,
        이름: &str,
        doc: &DocPath,
        topic: &Topic,
        방문: &mut HashSet<String>,
    ) -> Emitter {
        let mut e = Emitter::new();
        e.pair("name", 이름);
        e.flag("uncoded", topic.uncoded);
        e.block("topic", &topic.body);
        // 아무것도 참조하지 않는 topic 에 빈 `references` 를 달지 않는다.
        if let Some(참조) = self.참조_묶음(doc, topic, 방문) {
            e.map("references", 참조);
        }
        e
    }

    /// topic 이 참조하는 키워드와 topic 을 모은다.
    ///
    /// **재귀와 중복 제거가 만나는 자리다.** 처음 도달한 심볼은 통째로 전개하고 방문에
    /// 넣는다. 두 번째부터는 전체 주소 하나로 대체하며, 그 주소가 곧 방문 집합의 키다.
    ///
    /// **그 주소로 먼저 전개된 자리를 찾는 방법은 종류마다 다르다.** 참조된 topic 의
    /// 전개 자리는 `name` 에 전체 주소를 그대로 담으므로 문자열이 같다. keyword 의 전개
    /// 자리는 `name`(계층 이름)과 `path`(owner 문서)로 나뉘어 있으므로 `{path}.{name}`
    /// 으로 되맞춰야 이 주소와 같아진다. 스펙 6.4 의 예시가 그 모양이므로 `name` 을
    /// 전체 주소로 올려 문자열을 맞추지 않는다.
    ///
    /// # 매개변수
    /// - `doc`: 이 topic 이 있는 문서
    /// - `topic`: 참조를 볼 topic
    /// - `방문`: 이미 전개한 심볼의 전체 주소 집합
    ///
    /// # 반환값
    /// `keywords`·`topics` 를 담은 이미터. 참조가 하나도 없으면 `None`
    fn 참조_묶음(
        &self,
        doc: &DocPath,
        topic: &Topic,
        방문: &mut HashSet<String>,
    ) -> Option<Emitter> {
        let scope = self.table.scope(doc);
        let mut 대상들: Vec<SymbolId> = Vec::new();
        // 분할은 진단을 내는 층과 같은 함수를 쓴다.
        for (이름, _) in check::이름_분할(&topic.refs, &scope) {
            // 스코프에 없는 이름은 미해결 심볼이며 compile() 이 이미 거절했다.
            let Some(&id) = scope.get(&이름) else {
                continue;
            };
            // 한 본문이 같은 심볼을 여러 번 가리켜도 항목은 하나다.
            if !대상들.contains(&id) {
                대상들.push(id);
            }
        }

        let mut 키워드들: Vec<Emitter> = Vec::new();
        let mut 토픽들: Vec<Emitter> = Vec::new();
        // 본문에 나온 순서대로 훑는다.
        for id in 대상들 {
            // 색인에 없는 식별자는 없다 — 스코프의 값은 전부 선언에서 왔다.
            let Some(자리) = self.찾기(id) else {
                continue;
            };
            match 자리.선언 {
                선언::키워드(keyword) => {
                    let 주소 = format!("{}.{}", 자리.doc, keyword.name.0.join("."));
                    // 처음 도달했으면 전개하고, 아니면 경로 하나로 줄인다.
                    키워드들.push(if 방문.insert(주소.clone()) {
                        self.키워드_항목(자리.doc, keyword)
                    } else {
                        Emitter::value(&주소)
                    });
                }
                선언::토픽(참조된) => {
                    let 주소 = format!("{}#{}", 자리.doc, 참조된.name);
                    // 전개 **전에** 방문에 넣는다. 자기 자신을 참조하는 topic 이 여기서
                    // 무한 재귀가 되는 것을 막는 것이 이 순서다 — 같은 파일 안의 참조는
                    // import 간선을 만들지 않아 순환 검사가 보지 않는다.
                    토픽들.push(if 방문.insert(주소.clone()) {
                        self.토픽_항목(&주소, 자리.doc, 참조된, 방문)
                    } else {
                        Emitter::value(&주소)
                    });
                }
                // 예외는 스펙 6.4 의 `references` 스키마에 없다. 이 문서가 커버하는
                // 예외라면 `covers` 절이 이미 선언 맥락째로 담았다.
                선언::예외(..) => {}
            }
        }

        // 참조가 없으면 절 자체를 만들지 않는다.
        if 키워드들.is_empty() && 토픽들.is_empty() {
            return None;
        }
        let mut e = Emitter::new();
        if !키워드들.is_empty() {
            e.seq("keywords", 키워드들);
        }
        if !토픽들.is_empty() {
            e.seq("topics", 토픽들);
        }
        Some(e)
    }

    /// 이 문서가 정의한 예외 항목 하나를 만든다.
    ///
    /// # 매개변수
    /// - `doc`: 예외를 선언한 문서
    /// - `exception`: 담을 예외
    ///
    /// # 반환값
    /// 이름·`pending` 과, 커버가 있으면 그 topic 의 주소와 본문을 담은 이미터
    fn 예외_항목(&self, doc: &DocPath, exception: &Exception) -> Emitter {
        let mut e = Emitter::new();
        e.pair("name", &exception.name);
        e.flag("pending", exception.pending);

        let 대상 = self.table.resolve(&SymbolRef {
            doc: doc.clone(),
            kind: SymbolKind::Exception,
            name: vec![exception.name.clone()],
        });
        // 커버가 둘 이상인 것은 error 이므로(스펙 5.2) 컴파일을 통과한 프로젝트에는
        // 하나뿐이다. 없으면 아직 커버되지 않은 예외이며 `coveredBy` 를 달지 않는다.
        if let Some(id) = 대상
            && let Some((커버_문서, 커버_토픽)) = self.커버하는_topic(id)
        {
            e.pair("coveredBy", &format!("{커버_문서}#{}", 커버_토픽.name));
            e.block("topic", &커버_토픽.body);
        }
        e
    }

    /// 예외를 커버하는 topic 을 찾는다.
    ///
    /// # 매개변수
    /// - `대상`: 커버될 예외의 식별자
    ///
    /// # 반환값
    /// 커버하는 문서와 topic. 커버가 없으면 `None`
    fn 커버하는_topic(&self, 대상: SymbolId) -> Option<(&'a DocPath, &'a Topic)> {
        // HashMap 의 나열 순서는 보장되지 않는다. 정렬해야 결과가 실행마다 같다.
        let 순서 = self.project.경로들();

        // 문서를 경로 순으로, 그 안의 topic 은 선언 순으로 훑는다.
        for doc in 순서 {
            let scope = self.table.scope(doc);
            for topic in &self.project.docs[doc].topics {
                // cover 에 적힌 이름은 alias 일 수 있으므로 이름이 아니라 심볼로 맞춘다.
                if topic
                    .covers
                    .iter()
                    .any(|(이름, _)| scope.get(이름) == Some(&대상))
                {
                    return Some((doc, topic));
                }
            }
        }
        None
    }

    /// 이 문서가 커버하는 예외 항목 하나를 만든다.
    ///
    /// # 매개변수
    /// - `doc`: `cover` 를 선언한 문서
    /// - `이름`: `cover` 줄에 적힌 이름. alias 일 수 있다
    ///
    /// # 반환값
    /// 예외의 정본 이름·선언 문서·선언 topic 의 본문을 담은 이미터.
    /// 이름이 해석되지 않거나 예외가 아니면 `None` — 그 판정은 `compile()` 이 이미 했다
    fn 커버_항목(&self, doc: &DocPath, 이름: &str) -> Option<Emitter> {
        let &id = self.table.scope(doc).get(이름)?;
        let 자리 = self.찾기(id)?;
        let 선언::예외(exception, 선언_토픽) = 자리.선언 else {
            return None;
        };

        let mut e = Emitter::new();
        e.pair("name", &exception.name);
        e.pair("path", &자리.doc.to_string());
        // 예외는 본문이 없다. 의미가 나오는 자리는 그것을 선언한 topic 이다 (스펙 4.8).
        e.block("topic", &선언_토픽.body);
        Some(e)
    }
}
