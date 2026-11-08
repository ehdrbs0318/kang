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
/// - `table`: 심볼 테이블. 문서마다 `scope` 를 만드는 데 쓴다
/// - `sources`: 문서 경로에서 파일 원문으로 가는 맵. 문서 해시의 입력이다
/// - `out`: 인덱스를 받을 곳
///
/// # 오류
/// 문서의 원문이 `sources` 에 없으면 `InvalidInput`. 쓰기가 실패하면 그 사정을 그대로 올린다.
pub fn write_graph(
    project: &Project,
    table: &SymbolTable,
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
            "{{\"path\":{},\"description\":{},\"hash\":\"{}\",\"scope\":[{}]}}",
            json_string(&path.to_string()),
            json_string(&project.docs[*path].description),
            hash::doc_hash(source),
            scope_json(project, table, path)
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

/// 문서 하나의 `scope` 목록을 JSON 항목들로 만든다.
///
/// 항목은 본문 참조 문자열(`local`)에서 정식 주소(`address`)와 그 심볼을 선언한 문서(`doc`)로
/// 가는 짝이다. 컴파일러가 받는 참조만 담는다. 하나는 [`SymbolTable::scope`] 의 로컬 이름이고,
/// 다른 하나는 `iknow` 대상의 정식 주소다. `iknow` 대상은 스코프가 아니라 전체 주소로 풀리므로
/// 주소 자체를 `local` 로 쓴다. 같은 `local` 이 둘이면 스코프의 것을 남긴다.
///
/// # 매개변수
/// - `project`: 로드된 프로젝트
/// - `table`: 심볼 테이블
/// - `path`: scope 를 만들 문서
///
/// # 반환값
/// `local` 순서로 정렬해 쉼표로 이은 항목들. 비면 ''
fn scope_json(project: &Project, table: &SymbolTable, path: &DocPath) -> String {
    let address = |id| symbol_address(table.owner(id), table.kind(id), table.name(id), false);
    let mut entries: BTreeMap<String, (String, String)> = BTreeMap::new();
    // 스코프의 로컬 이름마다 정식 주소와 선언 문서를 적는다.
    for (local, &id) in table.scope(path) {
        entries.insert(local.clone(), (address(id), table.owner(id).to_string()));
    }
    let document = &project.docs[path];
    let iknow = document
        .keywords
        .iter()
        .flat_map(|k| &k.iknow)
        .chain(document.topics.iter().flat_map(|t| &t.iknow))
        .chain(
            document
                .topics
                .iter()
                .flat_map(|t| t.exceptions.iter().flat_map(|e| &e.iknow)),
        );
    // iknow 대상은 전체 주소가 곧 참조 문자열이다. 풀리지 않는 대상은 컴파일이 이미 막았다.
    for target in iknow {
        if let Some(id) = table.resolve(target) {
            let local = address(id);
            entries
                .entry(local.clone())
                .or_insert((local, target.doc.to_string()));
        }
    }
    entries
        .iter()
        .map(|(local, (address, doc))| {
            format!(
                "{{\"local\":{},\"address\":{},\"doc\":{}}}",
                json_string(local),
                json_string(address),
                json_string(doc)
            )
        })
        .collect::<Vec<String>>()
        .join(",")
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
