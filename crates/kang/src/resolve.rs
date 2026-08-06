//! 프로젝트 전체를 읽어 전역 심볼 테이블을 만드는 층.
//!
//! [`crate::parse`] 는 파일 하나만 안다. 여러 파일을 함께 보는 것은 여기서 시작한다 —
//! 루트를 찾고, `.kang` 파일을 전부 읽어 파싱하고, 심볼에 프로젝트 전역 식별자를 준다.
//!
//! 이 모듈이 내는 진단 코드는 환경·IO 대역인 `K050`-`K059` 를 쓴다.
//!
//! | 코드 | 규칙 |
//! |---|---|
//! | `K050` | git 저장소가 아니라 프로젝트 루트를 정할 수 없음 |
//! | `K051` | 파일이나 디렉토리를 읽지 못함 (IO 오류·UTF-8 아님) |
//! | `K052` | 한 문서 안에서 같은 로컬 이름이 두 번 묶임 |
//!
//! 여기에 더해 **`K113`**(문서 파일 이름에 `.`·`#`·`!`)을 낸다. 스펙 6.0 `:419` 가 문법
//! 대역의 번호를 배당했으나 그 이름을 보는 층은 문서를 로드하는 이곳뿐이다.
//!
//! **줄 번호가 없는 진단.** [`crate::ast::Location::line`] 은 1-based 이므로 `0` 은
//! "가리킬 줄이 없음" 을 뜻한다. 파일을 읽지도 못했거나 대상이 디렉토리인 진단이
//! 그 경우다. 없는 줄을 1 이라고 적으면 진단이 검증되지 않는 사실을 말하게 된다.
//!
//! **이 층이 판정하지 않는 것.** 서로 다른 파일이 같은 이름을 선언하는 것은 `iknow`
//! 로 합법이 된다 (스펙 4.4). 상호성을 보는 층이 판정해야 하므로 여기서는
//! [`SymbolTable::by_name`] 으로 사실만 노출한다. 미해결 심볼과 순환도 마찬가지다.

use crate::ast::{
    Diagnostic, DocPath, Document, Fix, FixKind, Location, Severity, SymbolKind, SymbolRef,
};
use crate::parse::parse_document;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 파싱된 문서 전체를 담는다.
pub struct Project {
    /// 루트 기준 문서 경로로 찾는 문서들. 파싱에 실패한 문서는 들어 있지 않다.
    pub docs: HashMap<DocPath, Document>,
}

/// 프로젝트 루트를 찾는다. git 저장소 루트가 곧 kang 프로젝트 루트다.
/// git 저장소가 아니면 그 사실을 진단으로 돌려준다.
///
/// # 매개변수
/// - `cwd`: 탐색을 시작할 디렉토리. **절대 경로여야 한다** —
///   호출자는 `std::env::current_dir()` 의 결과를 넘긴다.
///   경로를 정규화하지 않으므로 심볼릭 링크는 풀리지 않은 채 그대로 이어진다
///   (루트와 문서 경로가 서로 어긋나지 않으므로 일관성에는 영향이 없다)
///
/// # 반환값
/// `.git` 을 가진 가장 가까운 상위 디렉토리
///
/// # 오류
/// 위로 끝까지 올라가도 `.git` 이 없으면 `K050` 진단
pub fn find_root(cwd: &Path) -> Result<PathBuf, Diagnostic> {
    cwd.ancestors()
        // 상대 경로의 마지막 조상은 **빈 경로**이고 `Path::new("").join(".git")` 은
        // 프로세스 cwd 기준으로 해석된다. 걸러내지 않으면 cwd 가 저장소일 때
        // 빈 루트를 성공으로 돌려주고, 그 뒤 load 가 경로 없는 진단을 낸다.
        // 절대 경로의 마지막 조상은 `/` 이므로 영향이 없다.
        .filter(|dir| !dir.as_os_str().is_empty())
        // `.git` 은 디렉토리일 수도 파일일 수도 있다 — worktree 와 submodule 은 파일이다.
        // 그래서 종류를 묻지 않고 존재만 본다. `is_dir()` 로 좁히면 그 저장소들이
        // 통째로 K050 을 맞는다.
        .find(|dir| dir.join(".git").exists())
        .map(Path::to_path_buf)
        .ok_or_else(|| git_저장소_아님(cwd))
}

/// 루트를 재귀 순회하며 .kang 파일을 전부 읽어 파싱한다.
/// 모든 DocPath 는 루트 기준 상대 경로이므로 어느 하위 디렉토리에서 실행해도 동일하다.
///
/// 한 파일이 읽히지 않거나 파싱에 실패해도 나머지 파일 처리를 계속한다 —
/// 문서 하나 때문에 프로젝트 전체가 보이지 않으면 고칠 길이 막힌다.
///
/// # 매개변수
/// - `root`: 프로젝트 루트
///
/// # 반환값
/// 파싱에 성공한 문서들과, 순회·읽기·파싱에서 나온 진단 전부
pub fn load(root: &Path) -> (Project, Vec<Diagnostic>) {
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let mut files: Vec<PathBuf> = Vec::new();
    수집(root, &mut files, &mut diagnostics);

    // 파일 시스템의 나열 순서는 보장되지 않는다. 정렬해야 진단 순서가 실행마다 같다.
    files.sort();

    let mut docs: HashMap<DocPath, Document> = HashMap::new();
    // 찾아낸 `.kang` 파일을 차례로 읽어 문서로 바꾼다.
    for file in files {
        let path = 문서경로(root, &file);

        // **문서 파일 이름에 주소 구분자가 있으면 읽지 않는다** (스펙 6.0 `:414`).
        // 마지막 조각만 본다 — 디렉토리 이름의 구분자는 주소를 마지막 `/` 뒤에서 가르는
        // 규칙 덕에 문제가 되지 않으므로 합법이다.
        if let Some(구분자) = path
            .0
            .last()
            .and_then(|이름| 이름.chars().find(|글자| ".#!".contains(*글자)))
        {
            diagnostics.push(문서_이름_구분자(path, 구분자));
            continue;
        }

        // 텍스트로 디코딩하기 전이므로 바이트로 읽는다. UTF-8 이 아닌 것도 진단 대상이다.
        let bytes = match std::fs::read(&file) {
            Ok(bytes) => bytes,
            // 권한이 없거나 읽는 도중 사라진 파일. 삼키면 문서가 조용히 없어진다.
            Err(error) => {
                diagnostics.push(파일_읽기_실패(&file, path, &error));
                continue;
            }
        };

        let source = match String::from_utf8(bytes) {
            Ok(source) => source,
            // kang 문서는 UTF-8 텍스트다. 아니면 파서가 볼 수 있는 것이 없다.
            Err(error) => {
                diagnostics.push(utf8_아님(&file, path, error.utf8_error().valid_up_to()));
                continue;
            }
        };

        // BOM 은 디코딩 아티팩트지 문법 요소가 아니다. 여기서 벗겨야 파서·해시·캐시가
        // 모두 같은 텍스트를 본다. 파서에 넣으면 다른 경로가 여전히 BOM 을 달고 다닌다.
        let source = source.strip_prefix('\u{feff}').unwrap_or(&source);

        match parse_document(path.clone(), source) {
            Ok(document) => {
                docs.insert(path, document);
            }
            // 통과하지 못한 문서는 어떤 CLI 명령으로도 출력되지 않는다 (스펙 5절).
            Err(파싱_진단) => diagnostics.extend(파싱_진단),
        }
    }

    (Project { docs }, diagnostics)
}

/// 심볼 하나를 가리키는 프로젝트 전역 식별자.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SymbolId(usize);

/// 심볼 테이블이 심볼 하나에 대해 들고 있는 것.
struct Symbol {
    /// 이 심볼을 선언한 문서.
    doc: DocPath,
    /// 심볼의 종류.
    kind: SymbolKind,
    /// 전체 이름. keyword 계층은 `.` 로 이어 붙인다 —
    /// 이름 충돌 판정이 전체 경로 기준이기 때문이다 (스펙 4.3).
    name: String,
    /// rev 핀이 해시할 텍스트 (스펙 4.8).
    hash_source: String,
    /// 선언이 등장한 줄 번호. 1-based 다.
    line: usize,
}

/// 프로젝트의 모든 심볼과 그것을 찾는 색인.
pub struct SymbolTable {
    /// 모든 심볼. [`SymbolId`] 가 이 벡터의 첨자다.
    symbols: Vec<Symbol>,
    /// 이름으로 찾는 색인. 같은 이름이 여러 문서에 있으면 전부 담는다.
    by_name: HashMap<String, Vec<SymbolId>>,
    /// 문서별 로컬 이름 → 심볼. 자기 선언과 import 를 합친 것이다.
    scopes: HashMap<DocPath, HashMap<String, SymbolId>>,
}

impl SymbolTable {
    /// 전역 심볼 테이블을 만든다. 이름 충돌은 진단으로 보고한다.
    ///
    /// 여기서 보는 충돌은 **한 문서 안에서 같은 로컬 이름이 두 번 묶이는 것**뿐이다.
    /// 그 경우 본문의 백틱 하나가 어느 심볼을 가리키는지 정할 수 없다.
    /// 서로 다른 문서가 같은 이름을 선언하는 것은 `iknow` 로 합법이므로
    /// 여기서 진단하지 않는다 (스펙 4.4).
    ///
    /// # 매개변수
    /// - `project`: 파싱을 마친 프로젝트
    ///
    /// # 반환값
    /// 만들어진 테이블과, 로컬 이름이 겹친 자리에 대한 `K052` 진단들
    pub fn build(project: &Project) -> (SymbolTable, Vec<Diagnostic>) {
        // HashMap 의 나열 순서는 보장되지 않는다. 정렬해야 SymbolId 와 진단 순서가
        // 실행마다 같다.
        let mut 순서: Vec<&DocPath> = project.docs.keys().collect();
        // DocPath 는 Vec<String> 래퍼이므로 조각을 그대로 비교한다.
        // to_string() 으로 비교하면 비교마다 String 을 새로 할당한다.
        순서.sort_by(|a, b| a.0.cmp(&b.0));

        let mut symbols: Vec<Symbol> = Vec::new();
        let mut by_name: HashMap<String, Vec<SymbolId>> = HashMap::new();
        let mut 문서별: Vec<Vec<SymbolId>> = Vec::new();

        // 1단계: 모든 문서의 심볼을 모은다. import 해석은 전부 모인 뒤에야 가능하다.
        for doc in &순서 {
            let document = &project.docs[*doc];
            let mut 내_심볼: Vec<SymbolId> = Vec::new();

            // keyword 의 해시 입력은 한 줄 정의 텍스트다 (스펙 4.8).
            for keyword in &document.keywords {
                내_심볼.push(심볼_추가(
                    &mut symbols,
                    &mut by_name,
                    Symbol {
                        doc: (*doc).clone(),
                        kind: SymbolKind::Keyword,
                        name: keyword.name.0.join("."),
                        hash_source: keyword.definition.clone(),
                        line: keyword.line,
                    },
                ));
            }

            // topic 의 해시 입력은 헤딩을 포함한 본문이다. 선언 줄은 파서가 이미 뺐다.
            for topic in &document.topics {
                내_심볼.push(심볼_추가(
                    &mut symbols,
                    &mut by_name,
                    Symbol {
                        doc: (*doc).clone(),
                        kind: SymbolKind::Topic,
                        name: topic.name.clone(),
                        hash_source: topic.body.clone(),
                        line: topic.line,
                    },
                ));

                // exception 은 본문이 없다. 의미가 선언 topic 의 맥락에서 나오므로
                // 그 topic 의 본문을 해시 입력으로 쓴다 (스펙 4.8).
                for exception in &topic.exceptions {
                    내_심볼.push(심볼_추가(
                        &mut symbols,
                        &mut by_name,
                        Symbol {
                            doc: (*doc).clone(),
                            kind: SymbolKind::Exception,
                            name: exception.name.clone(),
                            hash_source: topic.body.clone(),
                            line: exception.line,
                        },
                    ));
                }
            }

            문서별.push(내_심볼);
        }

        let mut table = SymbolTable {
            symbols,
            by_name,
            scopes: HashMap::new(),
        };
        let mut diagnostics: Vec<Diagnostic> = Vec::new();

        // 2단계: 문서마다 로컬 이름을 묶는다. 자기 선언이 먼저, import 가 나중이다.
        for (index, doc) in 순서.iter().enumerate() {
            // 값에 줄 번호를 함께 담는다. 진단이 남의 파일 줄을 가리키지 않게 하려면
            // 심볼의 선언 줄이 아니라 **이 문서에서 묶인 줄**을 알아야 한다.
            // import 로 묶인 이름의 선언 줄은 다른 파일에 있다.
            let mut scope: HashMap<String, (SymbolId, usize)> = HashMap::new();

            // 자기 파일이 선언한 심볼들.
            for &id in &문서별[index] {
                let name = table.symbols[id.0].name.clone();
                let line = table.symbols[id.0].line;
                이름_묶기(doc, &mut scope, &mut diagnostics, name, line, id);
            }

            // import 한 심볼들. alias 가 있으면 그 이름으로, 없으면 대상의 정본 이름으로
            // 묶인다 (스펙 4.7).
            for import in &project.docs[*doc].imports {
                // 대상이 없는 import 는 미해결 심볼이며 그 판정은 이 층의 몫이 아니다.
                let Some(id) = table.resolve(&import.target) else {
                    continue;
                };
                let name = import
                    .alias
                    .clone()
                    .unwrap_or_else(|| import.target.name.join("."));
                이름_묶기(doc, &mut scope, &mut diagnostics, name, import.line, id);
            }

            // 줄 번호는 진단을 만드는 동안에만 쓰였으므로 테이블에는 남기지 않는다.
            table.scopes.insert(
                (*doc).clone(),
                scope
                    .into_iter()
                    .map(|(name, (id, _))| (name, id))
                    .collect(),
            );
        }

        (table, diagnostics)
    }

    /// 심볼 참조를 전역 식별자로 해석한다.
    ///
    /// # 매개변수
    /// - `r`: 문서·종류·이름을 갖춘 심볼 참조
    ///
    /// # 반환값
    /// 해당하는 심볼의 식별자. 그런 심볼이 없으면 `None`
    pub fn resolve(&self, r: &SymbolRef) -> Option<SymbolId> {
        let name = r.name.join(".");
        // 같은 이름이 여러 문서에 있을 수 있으므로 문서와 종류까지 맞춰야 한다.
        self.by_name
            .get(&name)?
            .iter()
            .copied()
            .find(|&id| self.symbols[id.0].doc == r.doc && self.symbols[id.0].kind == r.kind)
    }

    /// 한 문서 안에서 쓸 수 있는 로컬 이름 → 심볼 매핑.
    /// 자기 파일이 선언한 심볼과 import 한 alias 를 합친 것이다.
    ///
    /// **계층 keyword 의 키는 `.` 로 이은 전체 이름이다** (`"결제.상태"`).
    /// 반면 [`crate::ast::Topic::refs`] 는 백틱 쌍 하나가 조각 하나이므로
    /// `` `결제`.`상태` `` 는 `"결제"` 와 `"상태"` 두 항목으로 들어온다.
    /// 조회하는 쪽이 인접 조각을 먼저 합쳐야 한다.
    ///
    /// # 매개변수
    /// - `doc`: 스코프를 볼 문서
    ///
    /// # 반환값
    /// 로컬 이름에서 심볼 식별자로 가는 매핑. 모르는 문서면 빈 매핑
    pub fn scope(&self, doc: &DocPath) -> HashMap<String, SymbolId> {
        self.scopes.get(doc).cloned().unwrap_or_default()
    }

    /// 같은 이름으로 선언된 심볼들을 모아 반환한다. iknow 검사에 쓴다.
    ///
    /// 이름은 전체 경로 기준이다 — `결제`.`상태` 는 `"결제.상태"` 이며
    /// `구독`.`상태` 와 다른 이름이다 (스펙 4.3).
    ///
    /// # 매개변수
    /// - `name`: 찾을 전체 이름
    ///
    /// # 반환값
    /// 그 이름으로 선언된 심볼들. 없으면 빈 슬라이스
    pub fn by_name(&self, name: &str) -> &[SymbolId] {
        self.by_name.get(name).map_or(&[], Vec::as_slice)
    }

    /// 심볼을 선언한 문서를 돌려준다.
    ///
    /// # 매개변수
    /// - `id`: 심볼 식별자
    ///
    /// # 반환값
    /// 그 심볼을 선언한 문서의 경로
    pub fn owner(&self, id: SymbolId) -> &DocPath {
        &self.symbols[id.0].doc
    }

    /// rev 핀이 해시할 텍스트를 돌려준다 (스펙 4.8).
    ///
    /// keyword 는 한 줄 정의, topic 은 본문, exception 은 그것을 선언한 topic 의 본문이다.
    ///
    /// # 매개변수
    /// - `id`: 심볼 식별자
    ///
    /// # 반환값
    /// 해시 입력 텍스트
    pub fn hash_source(&self, id: SymbolId) -> &str {
        &self.symbols[id.0].hash_source
    }
}

/// 심볼을 테이블에 넣고 이름 색인을 갱신한다.
///
/// # 매개변수
/// - `symbols`: 심볼 저장소
/// - `by_name`: 이름 색인
/// - `symbol`: 넣을 심볼
///
/// # 반환값
/// 새로 부여된 식별자
fn 심볼_추가(
    symbols: &mut Vec<Symbol>,
    by_name: &mut HashMap<String, Vec<SymbolId>>,
    symbol: Symbol,
) -> SymbolId {
    let id = SymbolId(symbols.len());
    by_name.entry(symbol.name.clone()).or_default().push(id);
    symbols.push(symbol);
    id
}

/// 로컬 이름 하나를 스코프에 묶는다. 이미 묶여 있으면 `K052` 진단을 낸다.
///
/// # 매개변수
/// - `doc`: 대상 문서
/// - `scope`: 채워 나가는 스코프. 값에 이 문서에서 묶인 줄을 함께 담는다
/// - `diagnostics`: 진단을 모을 곳
/// - `name`: 묶을 로컬 이름
/// - `line`: 묶는 줄 번호 (1-based)
/// - `id`: 묶을 심볼
fn 이름_묶기(
    doc: &DocPath,
    scope: &mut HashMap<String, (SymbolId, usize)>,
    diagnostics: &mut Vec<Diagnostic>,
    name: String,
    line: usize,
    id: SymbolId,
) {
    // 먼저 묶인 것을 남긴다. 나중 것으로 덮으면 앞의 선언이 조용히 사라진다.
    if let Some(&(_, 기존)) = scope.get(&name) {
        // 진단은 **파일에 나타난 순서**로 가리킨다. 자기 선언을 import 보다 먼저
        // 도는 것은 컴파일러의 내부 사정이고, 스펙 4.7 은 import 를 파일 최상단에
        // 두라고 하므로 정렬하지 않으면 흔한 배치에서 줄 번호가 거꾸로 나온다.
        let (앞, 뒤) = if 기존 <= line {
            (기존, line)
        } else {
            (line, 기존)
        };
        diagnostics.push(이름_중복(doc, &name, 앞, 뒤));
        return;
    }
    scope.insert(name, (id, line));
}

/// 루트 아래를 재귀 순회하며 `.kang` 파일 경로를 모은다.
///
/// # 매개변수
/// - `dir`: 순회할 디렉토리
/// - `files`: 찾은 파일 경로를 모을 곳
/// - `diagnostics`: 진단을 모을 곳
fn 수집(dir: &Path, files: &mut Vec<PathBuf>, diagnostics: &mut Vec<Diagnostic>) {
    let 읽기 = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        // 디렉토리를 열지 못하면 그 아래 문서가 통째로 보이지 않는다. 조용히 넘기면
        // 사용자는 문서가 없는 것과 구분할 수 없다.
        Err(error) => {
            diagnostics.push(디렉토리_읽기_실패(dir, &error));
            return;
        }
    };

    // 항목을 먼저 모아 정렬한다. `수집` 은 `files.sort()` 보다 앞서 돌면서 진단을
    // 직접 밀어 넣으므로, 여기서 정렬하지 않으면 읽을 수 없는 형제 디렉토리가 둘
    // 이상일 때 **진단 순서가 실행마다 뒤집힌다.**
    let mut entries: Vec<std::fs::DirEntry> = Vec::new();
    for entry in 읽기 {
        match entry {
            Ok(entry) => entries.push(entry),
            Err(error) => diagnostics.push(디렉토리_읽기_실패(dir, &error)),
        }
    }
    entries.sort_by_key(std::fs::DirEntry::path);

    // 디렉토리 항목을 순회하며 하위 디렉토리는 파고들고 `.kang` 파일은 모은다.
    for entry in entries {
        // 숨은 항목은 사용자의 문서가 아니다. `.git` 을 통째로 훑는 낭비도 여기서 막힌다.
        // ponytail: `.gitignore` 는 보지 않는다. 무시된 디렉토리에 문서를 두는 일이
        // 드물어서다. 순회가 실제로 느려지면 `git ls-files -z -- '*.kang'` 로 갈아탄다.
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }

        let kind = match entry.file_type() {
            Ok(kind) => kind,
            // 이 시점에는 그 항목이 파일인지 디렉토리인지 모른다. 디렉토리라고
            // 단정한 진단을 내면 사라진 평범한 파일에 대해 거짓을 말한다.
            Err(error) => {
                diagnostics.push(항목_종류_확인_실패(&entry.path(), &error));
                continue;
            }
        };

        // ponytail: DirEntry 의 종류는 링크를 따라가지 않으므로 심볼릭 링크는
        // 디렉토리로도 파일로도 세지 않고 건너뛴다. 링크 순환에 빠지지 않는 대신
        // 링크된 문서는 보이지 않는다. 실제로 링크로 문서를 두는 프로젝트가 나오면
        // 방문한 정규 경로 집합을 들고 다니는 순회로 올린다.
        if kind.is_dir() {
            수집(&entry.path(), files, diagnostics);
        } else if kind.is_file() && entry.path().extension().is_some_and(|ext| ext == "kang") {
            files.push(entry.path());
        }
    }
}

/// 파일 시스템 경로를 루트 기준 [`DocPath`] 로 바꾼다.
///
/// # 매개변수
/// - `root`: 프로젝트 루트
/// - `file`: `.kang` 파일의 전체 경로
///
/// # 반환값
/// 확장자를 뗀 루트 기준 문서 경로
fn 문서경로(root: &Path, file: &Path) -> DocPath {
    // 폴백을 두지 않는다. 접두사가 없다면 `수집` 이 루트 밖을 훑었다는 뜻이고,
    // 그때 절대 경로를 문서 경로로 삼으면 틀린 주소가 조용히 프로젝트에 들어간다.
    let 상대 = file
        .strip_prefix(root)
        .expect("수집 이 root 아래에서만 경로를 모으므로 접두사가 항상 붙는다");
    // ponytail: UTF-8 이 아닌 파일 이름은 대체 문자로 바뀌어 원본과 어긋난다.
    // DocPath 가 String 인 한 그렇고, 그런 이름의 문서는 백틱 주소로 쓸 수도 없다.
    // 실제로 나타나면 DocPath 를 OsString 기반으로 올린다.
    let mut 조각: Vec<String> = 상대
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();
    // 마지막 조각의 `.kang` 확장자를 뗀다. 호출자가 확장자를 이미 확인했다.
    if let Some(last) = 조각.last_mut()
        && let Some(stem) = last.strip_suffix(".kang")
    {
        *last = stem.to_string();
    }
    DocPath(조각)
}

/// 문서가 아닌 대상을 가리키는 [`DocPath`] 를 만든다.
///
/// 디렉토리와 실행 위치는 문서가 아니므로 루트 기준 상대 경로로 줄일 수 없다.
/// 전체 경로를 조각 하나에 그대로 담아 표시가 실제 경로와 어긋나지 않게 한다.
///
/// **이 [`DocPath`] 는 문서 주소가 아니다.** 표시 외의 용도로 쓰면 안 된다 —
/// [`Project::docs`] 의 키로 찾을 수 없고, 백틱 심볼 주소로 쓸 수도 없다.
/// 조각이 언제나 하나이므로 `0.len() == 1` 로 구분할 수 있다.
///
/// # 매개변수
/// - `path`: 가리킬 경로
///
/// # 반환값
/// 전체 경로 문자열 하나를 담은 [`DocPath`]
fn 경로_그대로(path: &Path) -> DocPath {
    DocPath(vec![path.display().to_string()])
}

/// 셸 명령에 들어갈 값을 작은따옴표로 감싼다.
///
/// 스펙 5.1.1 은 "셸 명령이면 **인용까지 포함한다**", 6.1 은 "인용 여부를 스스로
/// 판단하게 두면 틀린다" 고 못박는다. 공백이 든 경로를 그대로 보간하면 셸이 인자를
/// 쪼개, 원인을 알리는 대신 **새로운 잘못된 사실**을 준다.
///
/// 작은따옴표 안에서는 모든 문자가 문자 그대로다. 값에 든 `'` 만 예외이므로
/// 그것을 `'\''`(따옴표 닫기 → 이스케이프된 따옴표 → 다시 열기)로 바꾼다.
///
/// # 매개변수
/// - `value`: 인용할 값. 경로일 수도 심볼 이름일 수도 있다
///
/// # 반환값
/// 셸에 그대로 붙여 넣을 수 있는 인용된 문자열
pub(crate) fn 셸_인용(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// git 저장소를 찾지 못했다는 진단을 만든다.
///
/// # 매개변수
/// - `cwd`: 탐색을 시작했던 디렉토리
///
/// # 반환값
/// `K050` 진단
fn git_저장소_아님(cwd: &Path) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: "K050",
        message: "kang 프로젝트 루트를 찾지 못했습니다. 프로젝트 루트는 git 저장소 루트입니다."
            .to_string(),
        locations: vec![Location {
            doc: 경로_그대로(cwd),
            line: 0,
            // 대안은 여기 적는다 — 그것은 지시이지 실행할 명령이 아니다.
            note: "이 디렉토리에서 위로 올라가며 .git 을 찾았지만 없었습니다. 이미 있는 git 저장소를 루트로 쓰려면 그 저장소 안의 디렉토리로 이동해 다시 실행하세요.".to_string(),
        }],
        // **`action` 은 명령만이다** (스펙 6.0). `kang` 을 이 디렉토리에서 불렀으므로
        // 여기를 루트로 삼겠다는 것이 기본 의도이고, `git init` 이 그 의도의 정확한
        // 표현이다. 인자를 붙이지 않는 것이 그래서 옳다 — 현재 디렉토리가 답이다.
        fixes: vec![Fix {
            kind: FixKind::Shell,
            doc: None,
            action: "git init".to_string(),
        }],
    }
}

/// 문서 파일을 읽지 못했다는 진단을 만든다.
///
/// # 매개변수
/// - `file`: 읽으려던 파일의 전체 경로
/// - `path`: 그 파일의 문서 경로
/// - `error`: 발생한 IO 오류
///
/// # 반환값
/// `K051` 진단
fn 파일_읽기_실패(file: &Path, path: DocPath, error: &std::io::Error) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: "K051",
        message: format!("문서 파일을 읽지 못했습니다 — {path}"),
        locations: vec![Location {
            doc: path,
            line: 0,
            note: format!("운영체제가 알린 원인: {error}"),
        }],
        fixes: vec![Fix {
            kind: FixKind::Shell,
            doc: None,
            // `action` 은 명령만이다. 무엇을 왜 확인하는지는 `message` 와 note 가 말한다.
            action: format!("ls -l {}", 셸_인용(&file.display().to_string())),
        }],
    }
}

/// 문서 파일 이름에 주소 구분자가 있다는 진단을 만든다 (스펙 6.0 `:414`).
///
/// **이 이름은 어떤 CLI 명령으로도 가리킬 수 없다.** 주소는 마지막 조각의 첫
/// `.`·`#`·`!` 에서 갈리므로 `docs/v1.2.승격` 은 "문서 `docs/v1` 의 keyword `2.승격`" 으로
/// 읽힌다. 그래서 이 문서의 심볼을 import 한 순간 `K020`(핀 없음)의 유일한 fix 인
/// `bless` 가 exit 2 로 죽고, 스펙 4.8 이 핀을 손으로 계산할 수 없다고 못박았으므로
/// **명령만으로는 처방이 아예 없다.** 파일 이름을 고치는 것이 유일한 해결이다.
///
/// 그래서 fix 는 `[shell]` 이 아니라 `[edit]` 이다 — 고칠 새 이름은 그 문서가 무엇에
/// 관한 것인지 아는 사람만 정할 수 있고, 컴파일러가 지어내면 그것은 처방이 아니다.
///
/// # 매개변수
/// - `path`: 그 파일의 문서 경로. 마지막 조각이 문제의 이름이다
/// - `구분자`: 이름에서 처음 나온 구분자
///
/// # 반환값
/// `K113` 진단
fn 문서_이름_구분자(path: DocPath, 구분자: char) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: "K113",
        message: format!(
            "문서 파일 이름에 주소 구분자 `{구분자}` 가 있습니다 — {path}. 이 문서는 CLI 주소로 가리킬 수 없어 kang bless·kang refs 가 받지 못하고, 이 문서의 심볼을 import 하면 핀을 붙일 방법이 없어 빌드가 error 에 머뭅니다 (스펙 6.0)."
        ),
        locations: vec![Location {
            doc: path.clone(),
            // 파일 이름 자체가 문제이므로 가리킬 줄이 없다.
            line: 0,
            note: "이 문서의 파일 이름. 주소는 마지막 조각의 첫 `.`·`#`·`!` 에서 갈리므로 이름의 일부와 구분자를 구별할 수단이 없습니다.".to_string(),
        }],
        fixes: vec![Fix {
            kind: FixKind::Edit,
            doc: Some(path),
            action: "이 파일 이름에서 `.`·`#`·`!` 를 빼고 이름을 바꾸세요. 이 문서를 가리키던 import 주소도 함께 고칩니다. 디렉토리 이름에는 이 세 문자를 그대로 쓸 수 있습니다".to_string(),
        }],
    }
}

/// 문서 파일이 UTF-8 이 아니라는 진단을 만든다.
///
/// fix 가 편집이 아니라 셸인 이유: 이 파일은 텍스트로 열리지 않으므로 에디터로
/// 고칠 수 없고, 형제 진단인 [`파일_읽기_실패`] 와 같은 `K051` 이므로 같은 모양이어야 한다.
///
/// # 매개변수
/// - `file`: 그 파일의 전체 경로
/// - `path`: 그 파일의 문서 경로
/// - `valid_up_to`: UTF-8 로 읽히다 멈춘 바이트 위치
///
/// # 반환값
/// `K051` 진단
fn utf8_아님(file: &Path, path: DocPath, valid_up_to: usize) -> Diagnostic {
    let 원본 = 셸_인용(&file.display().to_string());
    let 임시 = 셸_인용(&format!("{}.utf8", file.display()));
    Diagnostic {
        severity: Severity::Error,
        code: "K051",
        // 변환 방법은 여기 적는다. **명령으로 낼 수 없기 때문이다** — `-f` 에 넣을 원본
        // 인코딩은 첫 fix 를 실행한 뒤에야 알 수 있고, 자리를 비워 두면 셸이 `<인코딩>`
        // 을 입력 리다이렉션으로 읽어 터진다. 채워야 도는 템플릿은 fix 가 아니다.
        //
        // **`file -I` 가 추정이라는 것을 밝힌다.** 그 출력이 늘 맞지는 않으므로(EUC-KR 을
        // `iso-8859-1` 로 답하는 것을 실측했다) 그것을 확정으로 읽으면 틀린 변환을 한다.
        // 그렇다고 컴파일러가 후보를 단정하면 안 된다 — 한국어 밖 문서에서 자신 있게
        // 틀린 답을 내게 된다. 판단이 소비자에게 있다는 사실만 말한다.
        message: format!(
            "문서 파일이 UTF-8 이 아닙니다 — {path}. fix 의 file -I 는 원본 인코딩을 추정할 뿐이므로 그 값이 맞는지 확인한 뒤 -f 에 넣어 변환하세요: iconv -f <원본 인코딩> -t UTF-8 {원본} > {임시} && mv {임시} {원본}"
        ),
        locations: vec![Location {
            doc: path,
            line: 0,
            note: format!("바이트 {valid_up_to} 부터 UTF-8 로 디코딩되지 않습니다."),
        }],
        // 그래서 `[shell]` 로 낼 수 있는 것은 확인 명령 하나뿐이다.
        fixes: vec![Fix {
            kind: FixKind::Shell,
            doc: None,
            action: format!("file -I {원본}"),
        }],
    }
}

/// 디렉토리를 순회하지 못했다는 진단을 만든다.
///
/// # 매개변수
/// - `dir`: 읽으려던 디렉토리 경로
/// - `error`: 발생한 IO 오류
///
/// # 반환값
/// `K051` 진단
fn 디렉토리_읽기_실패(dir: &Path, error: &std::io::Error) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: "K051",
        message: format!(
            "디렉토리를 읽지 못해 그 아래 문서를 찾을 수 없습니다 — {}",
            dir.display()
        ),
        locations: vec![Location {
            doc: 경로_그대로(dir),
            line: 0,
            note: format!("운영체제가 알린 원인: {error}"),
        }],
        fixes: vec![Fix {
            kind: FixKind::Shell,
            doc: None,
            // `action` 은 명령만이다.
            action: format!("ls -ld {}", 셸_인용(&dir.display().to_string())),
        }],
    }
}

/// 디렉토리 항목의 종류를 확인하지 못했다는 진단을 만든다.
///
/// 이 시점에는 그 항목이 파일인지 디렉토리인지 **모른다.** 디렉토리라고 단정하면
/// 순회 중 사라진 평범한 `.kang` 파일에 대해 진단이 거짓을 말하게 된다.
///
/// # 매개변수
/// - `path`: 종류를 묻던 항목의 경로
/// - `error`: 발생한 IO 오류
///
/// # 반환값
/// `K051` 진단
fn 항목_종류_확인_실패(path: &Path, error: &std::io::Error) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: "K051",
        message: format!(
            "항목의 종류를 확인하지 못해 건너뛰었습니다 — {}",
            path.display()
        ),
        locations: vec![Location {
            doc: 경로_그대로(path),
            line: 0,
            note: format!("운영체제가 알린 원인: {error}"),
        }],
        fixes: vec![Fix {
            kind: FixKind::Shell,
            doc: None,
            // `action` 은 명령만이다.
            action: format!("ls -l {}", 셸_인용(&path.display().to_string())),
        }],
    }
}

/// 한 문서 안에서 같은 로컬 이름이 두 번 묶였다는 진단을 만든다.
///
/// # 매개변수
/// - `doc`: 대상 문서
/// - `name`: 겹친 로컬 이름
/// - `앞`: 파일에서 앞선 줄 번호 (1-based)
/// - `뒤`: 파일에서 뒤따르는 줄 번호 (1-based)
///
/// # 반환값
/// `K052` 진단
fn 이름_중복(doc: &DocPath, name: &str, 앞: usize, 뒤: usize) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: "K052",
        message: format!(
            "한 문서 안에서 이름 `{name}` 이 두 번 묶였습니다. 본문의 `{name}` 이 어느 쪽을 가리키는지 정할 수 없습니다."
        ),
        // 스펙 5.1.1 의 `K012` 예시와 같은 모양이다 — 순서를 주장하지 않고
        // "여기 / 여기도" 로만 가리킨다. 어느 쪽이 원인인지는 컴파일러가 알 수 없다.
        locations: vec![
            Location {
                doc: doc.clone(),
                line: 앞,
                note: format!("여기서 `{name}` 이 묶였습니다."),
            },
            Location {
                doc: doc.clone(),
                line: 뒤,
                note: format!("여기서도 `{name}` 이 묶였습니다."),
            },
        ],
        fixes: vec![Fix {
            kind: FixKind::Edit,
            doc: Some(doc.clone()),
            action: format!(
                "`{name}` 을 묶은 두 줄 중 하나를 고치세요. 선언이면 다른 이름을 주거나 그 줄을 지우고, import 면 `as` 로 다른 별칭을 주세요."
            ),
        }],
    }
}
