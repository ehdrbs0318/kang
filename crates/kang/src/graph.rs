//! 그래프 인덱스 — `kang build` 가 `.kang/graph.kang.json` 으로 쓰는 산출물.
//!
//! 편집기 플러그인이 읽는다. 문서 목록, keyword 정의, 문서 사이 import 관계(edge)를 담는다.
//! 같은 문서에서는 항상 같은 바이트가 나온다. 만든 시각은 넣지 않는다.
//!
//! **JSON 을 직접 쓴다.** `kang` 크레이트는 외부 의존성을 늘리지 않는다 (V0001 §10.1).
//! 한 항목이 한 줄이라 diff 가 항목 단위로 난다.
//!
//! **주소는 [`crate::check::symbol_address`] 가 만든다.** 사본을 만들면 `show`·`refs` 가
//! 받지 못하는 주소가 생긴다.

use crate::ast::DocPath;
use crate::check::symbol_address;
use crate::hash;
use crate::resolve::{Project, SymbolTable};
use std::collections::{BTreeMap, HashMap};
use std::io::{self, Write};

/// 그래프 인덱스의 첫 줄. 덮어쓰기 방어가 이 줄로 자기 산출물을 알아본다.
const FIRST_LINE: &str = "{\"kang\":\"graph/1\",";

/// 그래프 인덱스를 쓴다.
///
/// 순서는 `docs`·`keywords` 는 문서 경로 순서(keyword 는 그 안에서 선언 순서), `edges` 는
/// `(from, to)` 순서, `symbols` 는 import 순서다. 같은 (from, to) 의 import 는 edge 하나로
/// 합치고 같은 심볼은 한 번만 적는다. `import` 만 edge 를 만든다 (`iknow` 는 만들지 않는다).
///
/// # 매개변수
/// - `project`: 로드된 프로젝트
/// - `_table`: 심볼 테이블. 다른 산출물 함수와 시그니처를 맞추려고 받는다
/// - `sources`: 문서 경로에서 파일 원문으로 가는 맵. 문서 해시의 입력이다
/// - `out`: 인덱스를 받을 곳
///
/// # 오류
/// 문서의 원문이 `sources` 에 없으면 `InvalidInput`. 쓰기가 실패하면 그 사정을 그대로 올린다.
pub fn write_graph(
    project: &Project,
    _table: &SymbolTable,
    sources: &HashMap<DocPath, String>,
    out: &mut impl Write,
) -> io::Result<()> {
    let paths = project.paths();

    writeln!(out, "{FIRST_LINE}\n\"docs\":[")?;
    let mut lines = Vec::new();
    for path in &paths {
        // 원문 없이 해시를 내면 거짓 해시가 나간다. 조용히 넘기지 않는다.
        let Some(source) = sources.get(*path) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("문서 원문이 없습니다: {path}"),
            ));
        };
        lines.push(format!(
            "{{\"path\":{},\"description\":{},\"hash\":\"{}\"}}",
            json_string(&path.to_string()),
            json_string(&project.docs[*path].description),
            hash::doc_hash(source)
        ));
    }
    write_list(out, &lines)?;

    writeln!(out, ",\n\"keywords\":[")?;
    let mut lines = Vec::new();
    for path in &paths {
        for keyword in &project.docs[*path].keywords {
            lines.push(format!(
                "{{\"path\":{},\"name\":{},\"definition\":{}}}",
                json_string(&path.to_string()),
                json_string(&keyword.name.0.join(".")),
                json_string(&keyword.definition)
            ));
        }
    }
    write_list(out, &lines)?;

    writeln!(out, ",\n\"edges\":[")?;
    let mut lines = Vec::new();
    for path in &paths {
        // 대상 문서 조각 순서로 모으되, 심볼은 import 순서를 지킨다.
        let mut by_target: BTreeMap<&Vec<String>, Vec<String>> = BTreeMap::new();
        for import in &project.docs[*path].imports {
            let target = &import.target;
            let symbols = by_target.entry(&target.doc.0).or_default();
            let address = symbol_address(&target.doc, target.kind, &target.name.join("."), false);
            // 같은 심볼을 두 번 import 해도 한 번만 적는다.
            if !symbols.contains(&address) {
                symbols.push(address);
            }
        }
        for (target, symbols) in by_target {
            let symbols: Vec<String> = symbols.iter().map(|s| json_string(s)).collect();
            lines.push(format!(
                "{{\"from\":{},\"to\":{},\"symbols\":[{}]}}",
                json_string(&path.to_string()),
                json_string(&target.join("/")),
                symbols.join(",")
            ));
        }
    }
    write_list(out, &lines)?;
    writeln!(out, "}}")
}

/// 항목을 한 줄씩 쓰고 `]` 로 닫는다. 마지막 항목 뒤에는 쉼표를 두지 않는다.
///
/// # 매개변수
/// - `out`: 쓸 곳
/// - `lines`: 항목 한 줄씩. 비어 있으면 `]` 만 쓴다
///
/// # 오류
/// 쓰기가 실패하면 그대로 올린다.
fn write_list(out: &mut impl Write, lines: &[String]) -> io::Result<()> {
    write!(out, "{}", lines.join(",\n"))?;
    if !lines.is_empty() {
        writeln!(out)?;
    }
    write!(out, "]")
}

/// 주어진 내용이 [`write_graph`] 가 낸 것으로 보이는지 본다.
///
/// 덮어쓰기 방어에 쓴다. 첫 줄이 정해진 머리글이면 자기 산출물이다.
///
/// # 매개변수
/// - `content`: 이미 그 자리에 있는 파일의 내용
///
/// # 반환값
/// 첫 줄이 머리글이면 `true`
pub fn is_graph_format(content: &str) -> bool {
    content.lines().next() == Some(FIRST_LINE)
}

/// 문자열을 JSON 문자열 리터럴로 만든다. `"`, `\`, U+0000~U+001F 만 이스케이프한다.
///
/// # 매개변수
/// - `text`: 원문
///
/// # 반환값
/// 큰따옴표로 감싼 리터럴
fn json_string(text: &str) -> String {
    let mut result = String::with_capacity(text.len() + 2);
    result.push('"');
    for ch in text.chars() {
        match ch {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            // 제어 문자는 `\u00XX` 로 쓴다.
            c if (c as u32) < 0x20 => result.push_str(&format!("\\u{:04x}", c as u32)),
            c => result.push(c),
        }
    }
    result.push('"');
    result
}
