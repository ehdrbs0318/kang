//! 소스 한 파일을 [`Document`] 로 바꾸는 파서.
//!
//! 파서는 **파일 하나만 안다.** 다른 파일을 읽거나 심볼을 해석하지 않는다.
//! 프로젝트 전체를 보는 것은 `resolve` 의 몫이다.
//!
//! 이 모듈이 내는 진단 코드는 문법·파싱 대역인 `K100`-`K199` 를 쓴다.
//!
//! | 코드 | 규칙 |
//! |---|---|
//! | `K101` | frontmatter 블록이 없거나 닫히지 않음 |
//! | `K102` | frontmatter 에 `description` 이 없음 |
//! | `K103` | keyword 선언 문법이 올바르지 않음 |
//! | `K104` | 백틱 짝이 맞지 않음 |
//! | `K105` | topic 헤딩에 백틱이 있음 |

use crate::ast::{
    Diagnostic, DocPath, Document, Fix, FixKind, Keyword, KeywordName, Location, Severity, Topic,
};

/// 소스 한 파일을 Document 로 파싱한다.
///
/// `imports` / `exceptions` / `covers` / `iknow` / `uncoded` 는 Task 3 이 채운다.
///
/// # 매개변수
/// - `path`: 프로젝트 루트 기준 문서 경로
/// - `source`: 문서 원문
///
/// # 반환값
/// 파싱에 성공하면 [`Document`], 실패하면 발견한 진단 전부
pub fn parse_document(path: DocPath, source: &str) -> Result<Document, Vec<Diagnostic>> {
    let lines: Vec<&str> = source.lines().collect();
    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    // frontmatter 가 없으면 본문 시작 위치를 알 수 없으므로 즉시 중단한다.
    let (description, frontmatter_line, body_start) = match parse_frontmatter(&path, &lines) {
        Ok(parsed) => parsed,
        Err(diagnostic) => return Err(vec![diagnostic]),
    };

    // description 은 없어도 나머지 파싱을 계속해서 진단을 한 번에 모아 준다.
    let description = match description {
        Some(value) => value,
        None => {
            diagnostics.push(description_없음(&path, frontmatter_line));
            String::new()
        }
    };

    let mut keywords: Vec<Keyword> = Vec::new();
    let mut topics: Vec<Topic> = Vec::new();
    let mut in_fence = false;

    // frontmatter 다음 줄부터 끝까지 순회하며 keyword 선언과 topic 을 모은다.
    for (index, raw) in lines.iter().enumerate().skip(body_start) {
        let line_no = index + 1;
        let trimmed = raw.trim_start();

        // 코드 펜스 경계와 그 안쪽은 심볼 해석도 topic 분할도 하지 않는다 (스펙 4.2).
        let fence_marker = trimmed.starts_with("```");
        if fence_marker {
            in_fence = !in_fence;
        }
        if fence_marker || in_fence {
            if let Some(topic) = topics.last_mut() {
                push_body_line(topic, raw);
            }
            continue;
        }

        // `## ` 는 새 topic 의 시작이다. `###` 이하는 topic 본문의 마크다운 헤딩이다.
        if let Some(heading) = trimmed.strip_prefix("## ") {
            // 헤딩에 백틱이 있으면 그 topic 은 CLI 인자로 주소를 댈 수 없다 (스펙 6.0).
            // 짝 검사(K104)보다 먼저 판정해야 "주소 불가능" 이라는 정확한 진단이 나온다.
            if heading.contains('`') {
                diagnostics.push(헤딩_백틱(&path, heading.trim(), line_no));
            }
            topics.push(Topic {
                name: heading.trim().to_string(),
                body: raw.to_string(),
                uncoded: false,
                iknow: Vec::new(),
                refs: Vec::new(),
                exceptions: Vec::new(),
                covers: Vec::new(),
                line: line_no,
            });
            continue;
        }

        // keyword 선언은 서술이 아니라 선언이므로 topic 본문에 담지 않는다 (스펙 4.8).
        if let Some(rest) = trimmed.strip_prefix("keyword ") {
            match parse_keyword_line(&path, rest, line_no) {
                Ok(keyword) => keywords.push(keyword),
                Err(diagnostic) => diagnostics.push(diagnostic),
            }
            continue;
        }

        // 나머지 줄은 현재 topic 의 본문이다. topic 밖의 줄은 이 태스크에서 다루지 않는다.
        let Some(topic) = topics.last_mut() else {
            continue;
        };
        push_body_line(topic, raw);

        // 본문의 백틱 쌍은 전부 심볼 참조다.
        match scan_symbols(raw) {
            Some(symbols) => topic
                .refs
                .extend(symbols.into_iter().map(|symbol| (symbol, line_no))),
            None => diagnostics.push(백틱_짝_없음(&path, line_no)),
        }
    }

    // 진단이 하나라도 있으면 Document 를 만들지 않는다. 통과하지 못한 문서는 출력되지 않는다.
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }

    Ok(Document {
        path,
        description,
        imports: Vec::new(),
        keywords,
        topics,
    })
}

/// frontmatter 블록을 읽어 `description` 과 본문 시작 위치를 찾는다.
///
/// # 매개변수
/// - `path`: 진단에 담을 문서 경로
/// - `lines`: 문서 원문의 줄 목록
///
/// # 반환값
/// `(description 값, frontmatter 여는 줄 번호, 본문 시작 줄 인덱스)`.
/// `description` 키가 없거나 값이 비면 첫 항목이 `None` 이다.
///
/// # 오류
/// 블록이 없거나 닫히지 않으면 `K101` 진단을 돌려준다.
fn parse_frontmatter(
    path: &DocPath,
    lines: &[&str],
) -> Result<(Option<String>, usize, usize), Diagnostic> {
    // 파일 맨 앞의 빈 줄은 건너뛴다. 그 다음 줄이 `---` 여야 한다.
    let open = lines.iter().position(|line| !line.trim().is_empty());

    let Some(open) = open.filter(|&index| lines[index].trim() == "---") else {
        return Err(frontmatter_없음(
            path,
            "frontmatter 블록이 없습니다.".to_string(),
        ));
    };

    // 여는 `---` 다음에서 닫는 `---` 를 찾는다.
    let close = lines
        .iter()
        .skip(open + 1)
        .position(|line| line.trim() == "---")
        .map(|offset| open + 1 + offset);

    let Some(close) = close else {
        return Err(frontmatter_없음(
            path,
            "frontmatter 블록이 닫히지 않았습니다.".to_string(),
        ));
    };

    // ponytail: frontmatter 는 `키: 값` 한 줄 형식만 해석한다. 스펙 3절이 정의한 키가
    // `description` 하나뿐이라 YAML 파서가 필요 없다. 중첩 키가 생기면 그때 올린다.
    let description = lines[open + 1..close]
        .iter()
        .find_map(|line| line.trim().strip_prefix("description:"))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    Ok((description, open + 1, close + 1))
}

/// `keyword ` 접두사를 뗀 나머지를 [`Keyword`] 로 파싱한다.
///
/// # 매개변수
/// - `path`: 진단에 담을 문서 경로
/// - `rest`: `keyword ` 뒤의 나머지 텍스트
/// - `line_no`: 선언이 등장한 줄 번호 (1-based)
///
/// # 반환값
/// 파싱한 [`Keyword`]
///
/// # 오류
/// 백틱 짝이 맞지 않으면 `K104`, 이름이나 한 줄 정의가 없으면 `K103` 진단
fn parse_keyword_line(path: &DocPath, rest: &str, line_no: usize) -> Result<Keyword, Diagnostic> {
    // 이름과 정의 양쪽의 백틱이 모두 닫혀 있어야 구분자를 신뢰할 수 있다.
    if scan_symbols(rest).is_none() {
        return Err(백틱_짝_없음(path, line_no));
    }

    // 이름 뒤의 `:` 가 이름과 정의를 가른다. 정의 안의 `:` 에 걸리면 안 되므로 첫 번째만 본다.
    let Some(colon) = find_definition_colon(rest) else {
        return Err(keyword_문법_오류(
            path,
            line_no,
            "keyword 선언에 한 줄 정의가 없습니다.".to_string(),
        ));
    };

    let name_part = &rest[..colon];
    let definition_part = rest[colon + 1..].trim();

    // 이름은 백틱으로 감싼 조각들이며 `.` 로 계층을 이룬다.
    let names = scan_symbols(name_part).unwrap_or_default();
    if names.is_empty() {
        return Err(keyword_문법_오류(
            path,
            line_no,
            "keyword 이름이 백틱으로 감싸여 있지 않습니다.".to_string(),
        ));
    }

    // 줄 끝의 `#`상세 topic`` 은 선택이다. 없으면 전부가 한 줄 정의다.
    let (definition, detail) = match definition_part.rfind("#`") {
        Some(marker) => {
            let detail = scan_symbols(&definition_part[marker + 1..])
                .unwrap_or_default()
                .into_iter()
                .next();
            (definition_part[..marker].trim(), detail)
        }
        None => (definition_part, None),
    };

    if definition.is_empty() {
        return Err(keyword_문법_오류(
            path,
            line_no,
            "keyword 선언에 한 줄 정의가 없습니다.".to_string(),
        ));
    }

    Ok(Keyword {
        name: KeywordName(names),
        definition: definition.to_string(),
        detail,
        iknow: Vec::new(),
        line: line_no,
    })
}

/// 한 줄에서 백틱 쌍 안의 심볼 이름을 등장 순서대로 뽑는다.
///
/// `\`` 로 이스케이프한 백틱은 리터럴이므로 구분자로 세지 않는다 (스펙 4.2).
///
/// # 매개변수
/// - `line`: 검사할 한 줄
///
/// # 반환값
/// 백틱 쌍 안의 이름들. 짝이 맞지 않으면 `None`
fn scan_symbols(line: &str) -> Option<Vec<String>> {
    let bytes = line.as_bytes();
    let mut symbols = Vec::new();
    let mut open: Option<usize> = None;
    let mut index = 0;

    // 바이트 단위로 훑는다. 백틱과 역슬래시는 ASCII 라 UTF-8 이어 바이트와 겹치지 않는다.
    while index < bytes.len() {
        match bytes[index] {
            // 이스케이프된 백틱은 리터럴이므로 두 바이트를 통째로 건너뛴다.
            b'\\' if bytes.get(index + 1) == Some(&b'`') => index += 2,
            b'`' => {
                match open {
                    // 여는 백틱이 있었으면 여기서 닫히며 사이가 심볼 이름이다.
                    Some(start) => {
                        symbols.push(line[start..index].to_string());
                        open = None;
                    }
                    // 첫 백틱이면 이름의 시작 위치를 기억한다.
                    None => open = Some(index + 1),
                }
                index += 1;
            }
            _ => index += 1,
        }
    }

    // ponytail: 백틱 쌍은 한 줄 안에서 닫힌다고 본다. 심볼 이름에 줄바꿈이 들어갈 수
    // 없으므로 여는 백틱이 남은 줄은 오타다. 여러 줄 심볼 이름이 생기면 그때 올린다.
    if open.is_some() { None } else { Some(symbols) }
}

/// keyword 선언에서 이름과 정의를 가르는 `:` 의 바이트 위치를 찾는다.
///
/// 백틱 안의 `:` 는 이름의 일부이므로 건너뛴다.
///
/// # 매개변수
/// - `rest`: `keyword ` 뒤의 나머지 텍스트
///
/// # 반환값
/// 백틱 밖에 있는 첫 `:` 의 바이트 위치. 없으면 `None`
fn find_definition_colon(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    let mut in_backtick = false;
    let mut index = 0;

    // 백틱 안팎을 추적하면서 이름 뒤에 오는 첫 `:` 를 찾는다.
    while index < bytes.len() {
        match bytes[index] {
            // 이스케이프된 백틱은 구분자가 아니다.
            b'\\' if bytes.get(index + 1) == Some(&b'`') => index += 2,
            b'`' => {
                in_backtick = !in_backtick;
                index += 1;
            }
            // 백틱 밖의 `:` 만 이름과 정의의 경계다.
            b':' if !in_backtick => return Some(index),
            _ => index += 1,
        }
    }

    None
}

/// topic 본문에 원문 한 줄을 덧붙인다.
///
/// 본문은 헤딩 줄로 시작하므로 언제나 앞에 개행을 넣는다.
///
/// # 매개변수
/// - `topic`: 본문을 채울 topic
/// - `raw`: 덧붙일 원문 한 줄
fn push_body_line(topic: &mut Topic, raw: &str) {
    topic.body.push('\n');
    topic.body.push_str(raw);
}

/// frontmatter 블록이 없거나 닫히지 않았다는 진단을 만든다.
///
/// # 매개변수
/// - `path`: 대상 문서 경로
/// - `message`: 어느 쪽인지 알리는 한 문장
///
/// # 반환값
/// `K101` 진단
fn frontmatter_없음(path: &DocPath, message: String) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: "K101",
        message,
        locations: vec![Location {
            doc: path.clone(),
            line: 1,
            note: "여기에 frontmatter 가 있어야 합니다".to_string(),
        }],
        fixes: vec![Fix {
            kind: FixKind::Edit,
            doc: Some(path.clone()),
            action: "파일 맨 앞에 `---` 줄, `description: <이 문서를 고르기 위한 한 줄 설명>` 줄, `---` 줄을 순서대로 추가하세요".to_string(),
        }],
    }
}

/// frontmatter 에 `description` 이 없다는 진단을 만든다.
///
/// # 매개변수
/// - `path`: 대상 문서 경로
/// - `line`: frontmatter 를 여는 `---` 의 줄 번호
///
/// # 반환값
/// `K102` 진단
fn description_없음(path: &DocPath, line: usize) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: "K102",
        message: "frontmatter 에 description 이 없습니다.".to_string(),
        locations: vec![Location {
            doc: path.clone(),
            line,
            note: "이 frontmatter 블록".to_string(),
        }],
        fixes: vec![Fix {
            kind: FixKind::Edit,
            doc: Some(path.clone()),
            action: "frontmatter 블록 안에 `description: <이 문서를 고르기 위한 한 줄 설명>` 줄을 추가하세요".to_string(),
        }],
    }
}

/// keyword 선언 문법이 올바르지 않다는 진단을 만든다.
///
/// # 매개변수
/// - `path`: 대상 문서 경로
/// - `line`: 선언이 있는 줄 번호
/// - `message`: 무엇이 빠졌는지 알리는 한 문장
///
/// # 반환값
/// `K103` 진단
fn keyword_문법_오류(path: &DocPath, line: usize, message: String) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: "K103",
        message,
        locations: vec![Location {
            doc: path.clone(),
            line,
            note: "이 keyword 선언".to_string(),
        }],
        fixes: vec![Fix {
            kind: FixKind::Edit,
            doc: Some(path.clone()),
            action: "이 줄을 `keyword `<이름>`: <한 줄 정의>` 형식으로 고치세요. 상세 설명이 필요하면 줄 끝에 `#`<topic 이름>`` 을 붙입니다".to_string(),
        }],
    }
}

/// topic 헤딩에 백틱이 있다는 진단을 만든다.
///
/// 백틱이 든 헤딩은 CLI 인자로 주소를 댈 수 없어(스펙 6.0) 조회 불가능한 심볼이 된다.
///
/// # 매개변수
/// - `path`: 대상 문서 경로
/// - `heading`: 문제가 된 헤딩 텍스트
/// - `line`: 헤딩이 있는 줄 번호
///
/// # 반환값
/// `K105` 진단
fn 헤딩_백틱(path: &DocPath, heading: &str, line: usize) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: "K105",
        message: "topic 헤딩에 백틱이 있습니다. 이 topic 은 CLI 로 주소를 댈 수 없습니다."
            .to_string(),
        locations: vec![Location {
            doc: path.clone(),
            line,
            note: "이 topic 헤딩".to_string(),
        }],
        fixes: vec![Fix {
            kind: FixKind::Edit,
            doc: Some(path.clone()),
            action: format!(
                "topic 헤딩 \"{heading}\" 에서 백틱을 지우고 평문 이름으로 고치세요. CLI 인자에는 백틱을 쓸 수 없어서(스펙 6.0) 백틱이 든 헤딩은 kang show 로 조회할 수 없습니다"
            ),
        }],
    }
}

/// 백틱 짝이 맞지 않는다는 진단을 만든다.
///
/// # 매개변수
/// - `path`: 대상 문서 경로
/// - `line`: 짝이 맞지 않는 줄 번호
///
/// # 반환값
/// `K104` 진단
fn 백틱_짝_없음(path: &DocPath, line: usize) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: "K104",
        message: "백틱 짝이 맞지 않습니다.".to_string(),
        locations: vec![Location {
            doc: path.clone(),
            line,
            note: "이 줄의 백틱 개수가 홀수입니다".to_string(),
        }],
        fixes: vec![Fix {
            kind: FixKind::Edit,
            doc: Some(path.clone()),
            action: "이 줄의 심볼 참조를 `<이름>` 처럼 백틱 쌍으로 닫으세요. 문자 그대로의 백틱이라면 `\\`` 로 이스케이프합니다".to_string(),
        }],
    }
}
