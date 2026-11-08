// 그래프 인덱스(`kang::graph`)와 문서 해시(`kang::hash::doc_hash`)를 검증하는 통합 테스트.
use kang::ast::DocPath;
use kang::graph;
use kang::hash;
use kang::resolve::{self, SymbolTable};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// 문서 원문들로 임시 프로젝트를 만들어 그래프 인덱스 출력을 돌려준다.
///
/// # 매개변수
/// - `name`: 임시 디렉토리를 구분하는 이름
/// - `docs`: (루트 기준 경로 `A.kang` 꼴, 원문) 쌍들
///
/// # 반환값
/// `(출력 문자열, 경로별 원문)`
fn render(name: &str, docs: &[(&str, &str)]) -> (String, HashMap<DocPath, String>) {
    let root: PathBuf =
        std::env::temp_dir().join(format!("kang-graph-{}-{}", std::process::id(), name));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let mut sources = HashMap::new();
    for (file, text) in docs {
        fs::write(root.join(file), text).unwrap();
        let path = DocPath(vec![file.trim_end_matches(".kang").to_string()]);
        sources.insert(path, text.to_string());
    }
    let (project, diagnostics) = resolve::load(&root);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let (table, _) = SymbolTable::build(&project);
    let mut out = Vec::new();
    graph::write_graph(&project, &table, &sources, &mut out).unwrap();
    let _ = fs::remove_dir_all(&root);
    (String::from_utf8(out).unwrap(), sources)
}

/// 문서 둘의 출력 전체를 기대 문자열과 비교한다. 문서·keyword·edge 모두 정렬되어 있다.
#[test]
fn writes_sorted_docs_keywords_edges() {
    let a = "---\ndescription: 문서 A\n---\n\nkeyword `결제`: 대금을 낸다\nkeyword `결제수단`: 방법\nkeyword `결제수단`.`카드`: 카드로 낸다\n";
    let b = "---\ndescription: 문서 B\n---\n\nimport `A`.`결제`\nimport `A`.`결제수단`.`카드`\nimport `A`.`결제`\n";
    // 파일 순서를 거꾸로 줘도 출력은 경로 순서다.
    let (out, _) = render("sorted", &[("B.kang", b), ("A.kang", a)]);
    let expected = format!(
        "{{\"kang\":\"graph/1\",\n\"docs\":[\n\
{{\"path\":\"A\",\"description\":\"문서 A\",\"hash\":\"{}\"}},\n\
{{\"path\":\"B\",\"description\":\"문서 B\",\"hash\":\"{}\"}}\n\
],\n\"keywords\":[\n\
{{\"path\":\"A\",\"name\":\"결제\",\"definition\":\"대금을 낸다\"}},\n\
{{\"path\":\"A\",\"name\":\"결제수단\",\"definition\":\"방법\"}},\n\
{{\"path\":\"A\",\"name\":\"결제수단.카드\",\"definition\":\"카드로 낸다\"}}\n\
],\n\"edges\":[\n\
{{\"from\":\"B\",\"to\":\"A\",\"symbols\":[\"A.결제\",\"A.결제수단.카드\"]}}\n\
]}}\n",
        hash::doc_hash(a),
        hash::doc_hash(b)
    );
    assert_eq!(out, expected);
}

/// `"`, `\`, 탭이 JSON 규칙대로 나온다.
#[test]
fn escapes_quotes_backslash_and_control() {
    let a = "---\ndescription: 따옴표 \" 역슬래시 \\ 탭\t끝\n---\n";
    let (out, _) = render("escape", &[("A.kang", a)]);
    assert!(
        out.contains("\"description\":\"따옴표 \\\" 역슬래시 \\\\ 탭\\u0009끝\""),
        "{out}"
    );
}

/// 문서가 없으면 빈 목록도 한 줄 규칙을 지킨다.
#[test]
fn empty_lists_keep_line_layout() {
    let (out, _) = render("empty", &[]);
    assert_eq!(
        out,
        "{\"kang\":\"graph/1\",\n\"docs\":[\n],\n\"keywords\":[\n],\n\"edges\":[\n]}\n"
    );
}

/// 첫 줄로 자기 산출물을 알아본다.
#[test]
fn graph_format_is_detected_by_first_line() {
    assert!(graph::is_graph_format(
        "{\"kang\":\"graph/1\",\n\"docs\":[\n]}\n"
    ));
    assert!(!graph::is_graph_format("{\"name\":\"x\"}\n"));
    assert!(!graph::is_graph_format(""));
}

/// 문서 해시는 정규화 뒤 SHA-256 전체 hex 이고 앞 6자리가 `rev` 와 같다.
#[test]
fn doc_hash_matches_normalize_sha256() {
    let text = "a  \r\n\n\n b\n";
    let mut hasher = Sha256::new();
    hasher.update(hash::normalize(text).as_bytes());
    let expected: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let actual = hash::doc_hash(text);
    assert_eq!(actual, expected);
    assert_eq!(actual.len(), 64);
    assert_eq!(&actual[..6], hash::rev(text));
}
