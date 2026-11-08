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
    rev = "80f751"
)]
pub fn show(project: &Project, table: &SymbolTable, target: &ShowTarget) -> String {
    let (path, topic_name) = match target {
        ShowTarget::Document(path) => (path, None),
        ShowTarget::Topic(path, name) => (path, Some(name.as_str())),
    };

    // 없는 문서는 호출자가 사용법 오류로 거절한다. 여기서 패닉하면 문서 컴파일러가
    // 사용자 문서를 앞에 두고 죽는다.
    let Some(document) = project.docs.get(path) else {
        return String::new();
    };

    let view = View::new(project, table);
    let mut visit: HashSet<String> = HashSet::new();
    let mut out = Emitter::new();

    // topic 뷰는 그 topic 하나로 좁힌다. 문서 뷰는 전부다.
    let target_topics: Vec<&Topic> = document
        .topics
        .iter()
        .filter(|topic| topic_name.is_none_or(|name| topic.name == name))
        .collect();

    out.pair(
        "path",
        &topic_name.map_or_else(|| path.to_string(), |name| format!("{path}#{name}")),
    );

    // **최상위에서 펼칠 것을 먼저 방문으로 선점한다.** 그러지 않으면 본문 참조가 형제
    // topic 을 먼저 전개해 버리고, 최상위 자리가 경로 참조로 줄어 문서 자신의 본문이
    // 출력에서 사라진다.
    for topic in &target_topics {
        visit.insert(format!("{path}#{}", topic.name));
    }

    // 키워드는 topic 이 아니라 파일이 소유하므로 문서 뷰에만 나온다 (스펙 4.3).
    if topic_name.is_none() {
        for keyword in &document.keywords {
            visit.insert(format!("{path}.{}", keyword.name.0.join(".")));
        }

        if !document.keywords.is_empty() {
            let items: Vec<Emitter> = document
                .keywords
                .iter()
                .map(|keyword| view.keyword_entry(path, keyword))
                .collect();
            out.seq("keywords", items);
        }

        // 이 파일이 참조하는 키워드. 재귀 임베드와 달리 import 선언을 그대로 훑는다 —
        // 어떤 개념을 남에게서 빌려 왔는지가 곧 import 목록이다 (스펙 4.7).
        let mut imported: Vec<Emitter> = Vec::new();
        for import in &document.imports {
            // keyword 만 이 절의 대상이다. topic 과 exception 은 다른 절들이 담는다.
            if import.target.kind != SymbolKind::Keyword {
                continue;
            }
            // 대상이 없는 import 는 미해결 심볼이며 compile() 이 이미 거절했다.
            let Some(id) = table.resolve(&import.target) else {
                continue;
            };
            if let Some(position) = view.find(id)
                && let Declaration::Keyword(keyword) = position.declaration
            {
                let mut e = Emitter::new();
                e.pair("name", &keyword.name.0.join("."));
                e.pair("path", &position.doc.to_string());
                e.pair("description", &keyword.definition);
                imported.push(e);
            }
        }
        if !imported.is_empty() {
            out.seq("referencingKeywords", imported);
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
    let mut exceptions: Vec<Emitter> = Vec::new();
    for topic in &target_topics {
        for exception in &topic.exceptions {
            exceptions.push(view.exception_entry(path, exception));
        }
    }
    if !exceptions.is_empty() {
        out.seq("exceptions", exceptions);
    }

    // 이 파일(또는 topic)이 커버하는 예외와 그 선언 맥락.
    let mut covers: Vec<Emitter> = Vec::new();
    for topic in &target_topics {
        for (name, _) in &topic.covers {
            if let Some(item) = view.cover_entry(path, name) {
                covers.push(item);
            }
        }
    }
    if !covers.is_empty() {
        out.seq("covers", covers);
    }

    // 최상위 topic 은 이름을 그대로 쓴다. 소속 문서는 위의 `path` 가 이미 말했다.
    let mut topics: Vec<Emitter> = Vec::new();
    for topic in &target_topics {
        topics.push(view.topic_entry(&topic.name, path, topic, &mut visit));
    }
    if !topics.is_empty() {
        out.seq("topics", topics);
    }

    out.finish()
}

/// 심볼 식별자 하나가 가리키는 선언.
///
/// exception 은 본문이 없고 의미가 **선언한 topic 의 맥락**에서 나오므로(스펙 4.8)
/// 그 topic 을 함께 든다.
#[derive(Clone, Copy)]
enum Declaration<'a> {
    /// 키워드 선언.
    Keyword(&'a Keyword),
    /// topic 선언.
    Topic(&'a Topic),
    /// 예외 선언과 그것을 선언한 topic.
    Exception(&'a Exception, &'a Topic),
}

/// 심볼 식별자와 그것이 가리키는 선언, 그리고 선언이 있는 문서.
struct DeclarationSite<'a> {
    /// 전역 심볼 식별자.
    id: SymbolId,
    /// 이 선언이 있는 문서.
    doc: &'a DocPath,
    /// 선언 그 자체.
    declaration: Declaration<'a>,
}

/// 뷰를 조립하는 동안 쓰는 색인.
///
/// [`SymbolTable`] 은 식별자로부터 **종류와 이름을 돌려주지 않고** [`SymbolId`] 는
/// 해시할 수 없으므로, 되찾아야 하는 것을 선형 목록으로 들고 다닌다.
///
/// ponytail: 조회가 선형 탐색이라 전체가 O(심볼 × 참조) 다. 여기에 [`View::covering_topic_of`]
/// 의 O(예외 × 문서) 가 더 붙는다 — 예외마다 프로젝트의 topic 을 전부 훑으며 스코프를 뜬다.
/// 1600문서·예외 60개에서 0.146s 라 v1 에서는 무해하다. 실측이 이 둘 중 하나를 지목하면
/// `resolve` 에 식별자 기반 조회를 열어 색인 자체를 없애고, 커버는 한 번의 순회로
/// `(exception → Cover topic)` 표를 만들어 색인에 함께 싣는다.
///
/// **이 천장이 실제로 지배항이다** — 문서 400·pending 예외 100·문서당 keyword 80 에서
/// `kang show` 한 번이 58ms 이고, 그 대부분이 여기다. 리뷰가 지목했던
/// [`SymbolTable::scope`] 의 딥 클론을 없애 봤더니 같은 코퍼스에서 5% 만 줄었다(611 →
/// 582ms/10회). 다음에 이 파일을 최적화하는 사람은 스코프가 아니라 **이 순회**부터 봐라.
///
/// ponytail: [`check::split_names`] 을 topic 마다 두 번 돈다 — [`View::new`] 가 색인을 만들며
/// 한 번, [`View::ref_group`] 이 같은 `refs` 로 또 한 번. 첫 순회가 해석한 `SymbolId` 를
/// 버리는 것이 원인이고, 들고 있으면 한 번으로 줄어든다. 위 천장을 걷어낼 때 함께 한다 —
/// 지금 따로 고치면 색인 구조를 두 번 바꾸게 된다.
struct View<'a> {
    /// 컴파일을 통과한 프로젝트.
    project: &'a Project,
    /// 전역 심볼 테이블.
    table: &'a SymbolTable,
    /// 프로젝트의 모든 선언. 문서 경로 순, 문서 안에서는 선언 순이다.
    declarations: Vec<DeclarationSite<'a>>,
    /// `(참조된 심볼, 참조한 topic 의 전체 주소)`. 같은 짝은 한 번만 들어 있다.
    refs: Vec<(SymbolId, String)>,
}

impl<'a> View<'a> {
    /// 프로젝트를 훑어 색인을 만든다.
    ///
    /// # 매개변수
    /// - `project`: 컴파일을 통과한 프로젝트
    /// - `table`: 전역 심볼 테이블
    ///
    /// # 반환값
    /// 선언 색인과 참조 색인을 갖춘 뷰
    fn new(project: &'a Project, table: &'a SymbolTable) -> Self {
        // HashMap 의 나열 순서는 보장되지 않는다. 정렬해야 출력이 실행마다 같다.
        let order = project.paths();

        let mut declarations: Vec<DeclarationSite<'a>> = Vec::new();
        let mut refs: Vec<(SymbolId, String)> = Vec::new();

        // 문서를 경로 순으로 훑으며 선언과 참조를 함께 모은다.
        for doc in order {
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
                    declarations.push(DeclarationSite {
                        id,
                        doc,
                        declaration: Declaration::Keyword(keyword),
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
                    declarations.push(DeclarationSite {
                        id,
                        doc,
                        declaration: Declaration::Topic(topic),
                    });
                }

                for exception in &topic.exceptions {
                    if let Some(id) = table.resolve(&SymbolRef {
                        doc: doc.clone(),
                        kind: SymbolKind::Exception,
                        name: vec![exception.name.clone()],
                    }) {
                        declarations.push(DeclarationSite {
                            id,
                            doc,
                            declaration: Declaration::Exception(exception, topic),
                        });
                    }
                }

                // **분할은 진단을 내는 층과 같은 함수를 쓴다.** 두 층이 같은 문장을 다르게
                // 읽으면 빌드가 통과하는 문서에서 조회가 조용히 틀린 답을 낸다.
                let address = format!("{doc}#{}", topic.name);
                for (name, _) in check::split_names(&topic.refs, scope) {
                    // 스코프에 없는 이름은 미해결 심볼이며 compile() 이 이미 거절했다.
                    let Some(&id) = scope.get(&name) else {
                        continue;
                    };
                    // 한 topic 이 같은 심볼을 여러 번 가리켜도 참조처는 한 번만 센다.
                    if !refs
                        .iter()
                        .any(|(existing, position)| *existing == id && position == &address)
                    {
                        refs.push((id, address.clone()));
                    }
                }
            }
        }

        View {
            project,
            table,
            declarations,
            refs,
        }
    }

    /// 식별자가 가리키는 선언 자리를 찾는다.
    ///
    /// # 매개변수
    /// - `id`: 찾을 심볼 식별자
    ///
    /// # 반환값
    /// 그 식별자의 선언 자리. 색인에 없으면 `None`
    fn find(&self, id: SymbolId) -> Option<&DeclarationSite<'a>> {
        self.declarations.iter().find(|position| position.id == id)
    }

    /// 심볼의 전체 주소를 만든다 (스펙 6.0).
    ///
    /// # 매개변수
    /// - `id`: 주소를 물을 심볼
    ///
    /// # 반환값
    /// `docs/A.결제`·`docs/A#결제의 방법`·`docs/A!무료 결제` 꼴의 주소.
    /// 색인에 없으면 `None`
    fn address(&self, id: SymbolId) -> Option<String> {
        let position = self.find(id)?;
        let doc = position.doc;
        Some(match position.declaration {
            Declaration::Keyword(keyword) => format!("{doc}.{}", keyword.name.0.join(".")),
            Declaration::Topic(topic) => format!("{doc}#{}", topic.name),
            Declaration::Exception(exception, _) => format!("{doc}!{}", exception.name),
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
    fn keyword_entry(&self, owner: &DocPath, keyword: &Keyword) -> Emitter {
        let name = keyword.name.0.join(".");
        let mut e = Emitter::new();
        e.pair("name", &name);
        e.pair("path", &owner.to_string());
        e.pair("description", &keyword.definition);

        // 상세 topic 은 선택이다 (스펙 4.3). 있으면 전체 경로로 담는다 — 파싱만 하고
        // 버리면 조회한 쪽이 상세 설명에 닿을 길이 없다.
        //
        // **해석되지 않으면 키를 내지 않는다.** `{owner}#{detail}` 로 지어내면 없는 주소를
        // 사실처럼 말하게 된다. 미해결 상세 마커는 `K001` 이 이미 잡으므로(`check.rs` 의
        // `참조.push(.. "keyword 상세 마커")`) 컴파일을 통과한 문서에서는 늘 해석된다.
        if let Some(detail) = &keyword.detail
            && let Some(address) = self
                .table
                .scope(owner)
                .get(detail)
                .and_then(|&id| self.address(id))
        {
            e.pair("detail", &address);
        }

        // 이 키워드를 참조하는 topic 전부. `kang refs` 가 답하는 것과 같은 질문이며
        // 같은 색인에서 나온다.
        let referenced_by: Vec<Emitter> = match self.table.resolve(&SymbolRef {
            doc: owner.clone(),
            kind: SymbolKind::Keyword,
            name: keyword.name.0.clone(),
        }) {
            Some(id) => self
                .refs
                .iter()
                .filter(|(existing, _)| *existing == id)
                .map(|(_, address)| Emitter::value(address))
                .collect(),
            None => Vec::new(),
        };
        // 아무도 참조하지 않는 키워드가 빈 목록을 달고 다닐 이유가 없다.
        if !referenced_by.is_empty() {
            e.seq("referencedBy", referenced_by);
        }

        e
    }

    /// topic 항목 하나를 만든다. 참조한 topic 을 재귀적으로 임베드한다.
    ///
    /// # 매개변수
    /// - `이름`: 항목에 적을 이름. 최상위는 topic 이름, 참조 자리는 전체 주소다
    /// - `doc`: 이 topic 이 있는 문서
    /// - `topic`: 담을 topic
    /// - `visit`: 이미 전개한 심볼의 전체 주소 집합
    ///
    /// # 반환값
    /// 이름·`uncoded`·본문과, 있다면 참조를 담은 이미터
    fn topic_entry(
        &self,
        name: &str,
        doc: &DocPath,
        topic: &Topic,
        visit: &mut HashSet<String>,
    ) -> Emitter {
        let mut e = Emitter::new();
        e.pair("name", name);
        e.flag("uncoded", topic.uncoded);
        e.block("topic", &topic.body);
        // 아무것도 참조하지 않는 topic 에 빈 `references` 를 달지 않는다.
        if let Some(reference) = self.ref_group(doc, topic, visit) {
            e.map("references", reference);
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
    /// - `visit`: 이미 전개한 심볼의 전체 주소 집합
    ///
    /// # 반환값
    /// `keywords`·`topics` 를 담은 이미터. 참조가 하나도 없으면 `None`
    fn ref_group(
        &self,
        doc: &DocPath,
        topic: &Topic,
        visit: &mut HashSet<String>,
    ) -> Option<Emitter> {
        let scope = self.table.scope(doc);
        let mut targets: Vec<SymbolId> = Vec::new();
        // 분할은 진단을 내는 층과 같은 함수를 쓴다.
        for (name, _) in check::split_names(&topic.refs, scope) {
            // 스코프에 없는 이름은 미해결 심볼이며 compile() 이 이미 거절했다.
            let Some(&id) = scope.get(&name) else {
                continue;
            };
            // 한 본문이 같은 심볼을 여러 번 가리켜도 항목은 하나다.
            if !targets.contains(&id) {
                targets.push(id);
            }
        }

        let mut keywords: Vec<Emitter> = Vec::new();
        let mut topics: Vec<Emitter> = Vec::new();
        // 본문에 나온 순서대로 훑는다.
        for id in targets {
            // 색인에 없는 식별자는 없다 — 스코프의 값은 전부 선언에서 왔다.
            let Some(position) = self.find(id) else {
                continue;
            };
            match position.declaration {
                Declaration::Keyword(keyword) => {
                    let address = format!("{}.{}", position.doc, keyword.name.0.join("."));
                    // 처음 도달했으면 전개하고, 아니면 경로 하나로 줄인다.
                    keywords.push(if visit.insert(address.clone()) {
                        self.keyword_entry(position.doc, keyword)
                    } else {
                        Emitter::value(&address)
                    });
                }
                Declaration::Topic(referenced) => {
                    let address = format!("{}#{}", position.doc, referenced.name);
                    // 전개 **전에** 방문에 넣는다. 자기 자신을 참조하는 topic 이 여기서
                    // 무한 재귀가 되는 것을 막는 것이 이 순서다 — 같은 파일 안의 참조는
                    // import 간선을 만들지 않아 순환 검사가 보지 않는다.
                    topics.push(if visit.insert(address.clone()) {
                        self.topic_entry(&address, position.doc, referenced, visit)
                    } else {
                        Emitter::value(&address)
                    });
                }
                // 예외는 스펙 6.4 의 `references` 스키마에 없다. 이 문서가 커버하는
                // 예외라면 `covers` 절이 이미 선언 맥락째로 담았다.
                Declaration::Exception(..) => {}
            }
        }

        // 참조가 없으면 절 자체를 만들지 않는다.
        if keywords.is_empty() && topics.is_empty() {
            return None;
        }
        let mut e = Emitter::new();
        if !keywords.is_empty() {
            e.seq("keywords", keywords);
        }
        if !topics.is_empty() {
            e.seq("topics", topics);
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
    fn exception_entry(&self, doc: &DocPath, exception: &Exception) -> Emitter {
        let mut e = Emitter::new();
        e.pair("name", &exception.name);
        e.flag("pending", exception.pending);

        let target = self.table.resolve(&SymbolRef {
            doc: doc.clone(),
            kind: SymbolKind::Exception,
            name: vec![exception.name.clone()],
        });
        // 커버가 둘 이상인 것은 error 이므로(스펙 5.2) 컴파일을 통과한 프로젝트에는
        // 하나뿐이다. 없으면 아직 커버되지 않은 예외이며 `coveredBy` 를 달지 않는다.
        if let Some(id) = target
            && let Some((cover_doc, covering_topic)) = self.covering_topic_of(id)
        {
            e.pair("coveredBy", &format!("{cover_doc}#{}", covering_topic.name));
            e.block("topic", &covering_topic.body);
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
    fn covering_topic_of(&self, target: SymbolId) -> Option<(&'a DocPath, &'a Topic)> {
        // HashMap 의 나열 순서는 보장되지 않는다. 정렬해야 결과가 실행마다 같다.
        let order = self.project.paths();

        // 문서를 경로 순으로, 그 안의 topic 은 선언 순으로 훑는다.
        for doc in order {
            let scope = self.table.scope(doc);
            for topic in &self.project.docs[doc].topics {
                // cover 에 적힌 이름은 alias 일 수 있으므로 이름이 아니라 심볼로 맞춘다.
                if topic
                    .covers
                    .iter()
                    .any(|(name, _)| scope.get(name) == Some(&target))
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
    fn cover_entry(&self, doc: &DocPath, name: &str) -> Option<Emitter> {
        let &id = self.table.scope(doc).get(name)?;
        let position = self.find(id)?;
        let Declaration::Exception(exception, declaring_topic) = position.declaration else {
            return None;
        };

        let mut e = Emitter::new();
        e.pair("name", &exception.name);
        e.pair("path", &position.doc.to_string());
        // 예외는 본문이 없다. 의미가 나오는 자리는 그것을 선언한 topic 이다 (스펙 4.8).
        e.block("topic", &declaring_topic.body);
        Some(e)
    }
}
