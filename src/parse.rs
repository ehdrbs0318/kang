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
//! | `K106` | 코드 펜스가 닫히지 않음 |
//! | `K107` | topic 헤딩에 이름이 없음 |
//! | `K108` | 백틱 쌍 안이 비어 있음 |
//! | `K109` | import 선언 문법이 올바르지 않음 |
//! | `K110` | modifier 문법이 올바르지 않음 |
//! | `K111` | exception / cover 선언 문법이 올바르지 않음 |
//! | `K112` | topic 밖에 내용이 있음 |
//! | `K114` | import 선언이 첫 topic 뒤에 있음 |
//!
//! `K113` 은 문서 파일 이름의 구분자에 배당되어 있고 그것을 보는 층은 [`crate::resolve`] 다
//! (스펙 6.0 `:419`). 이 모듈이 그 번호를 쓰지 않는 이유가 그것이다.

use crate::ast::{
    Diagnostic, DocPath, Document, Exception, Fix, FixKind, Import, Keyword, KeywordName, Location,
    Severity, SymbolKind, SymbolRef, Topic,
};

/// 소스 한 파일을 Document 로 파싱한다.
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

    let mut imports: Vec<Import> = Vec::new();
    let mut keywords: Vec<Keyword> = Vec::new();
    let mut topics: Vec<Topic> = Vec::new();
    // 열려 있는 코드 펜스의 (시작 줄, 백틱 개수). 닫히면 None 으로 돌아간다.
    let mut fence_open: Option<(usize, usize)> = None;
    // topic 밖 내용 진단이 `diagnostics` 안에서 차지한 자리. 위반 줄을 전부 그 하나의
    // `locations` 에 모은다 — 빈 줄 유무로 진단 개수가 달라지면 사용자가 보기에 같은
    // 서문인데 개수가 다르다.
    let mut topic_밖_진단: Option<usize> = None;

    // frontmatter 다음 줄부터 끝까지 순회하며 keyword 선언과 topic 을 모은다.
    for (index, raw) in lines.iter().enumerate().skip(body_start) {
        let line_no = index + 1;
        let trimmed = raw.trim_start();

        // 스펙 3절: topic 밖에는 frontmatter·import·keyword·빈 줄 외의 내용을 둘 수 없다.
        // 판정은 이 한 곳에서만 한다. 아래 분기들보다 앞에 두되 줄을 소비하지는 않으므로,
        // 펜스·헤딩·선언의 기존 처리는 그대로 이어진다.
        // 펜스 갱신 **전에** 보므로 여는 줄만 걸리고 안쪽 줄은 그 블록에 딸린 것으로 본다.
        if topics.is_empty() && fence_open.is_none() && !topic_밖_허용(trimmed) {
            match topic_밖_진단 {
                // 두 번째부터는 같은 진단에 위치만 보탠다. 서문 열 줄에 진단 열 개는 소음이고,
                // 첫 줄만 가리키면 어디서 끝나는지 알 수 없다.
                Some(index) => diagnostics[index]
                    .locations
                    .push(topic_밖_위치(&path, line_no, false)),
                None => {
                    topic_밖_진단 = Some(diagnostics.len());
                    diagnostics.push(topic_밖_내용(&path, line_no));
                }
            }
        }

        // 코드 펜스 경계와 그 안쪽은 심볼 해석도 topic 분할도 하지 않는다 (스펙 4.2).
        let fence_run = trimmed.bytes().take_while(|&byte| byte == b'`').count();
        let fence_marker = fence_run >= 3;
        if fence_marker {
            fence_open = match fence_open {
                // 여는 펜스보다 짧은 런은 닫지 못한다. 4-백틱 블록 안의 3-백틱 줄은
                // 마크다운을 설명하는 문서에서 흔하고, 닫는 것으로 세면 오탐이 난다.
                Some((_, open_run)) if fence_run >= open_run => None,
                Some(open) => Some(open),
                None => Some((line_no, fence_run)),
            };
        }
        if fence_marker || fence_open.is_some() {
            if let Some(topic) = topics.last_mut() {
                push_body_line(topic, raw);
            }
            continue;
        }

        // `##` 는 새 topic 의 시작이다. `###` 이하는 topic 본문의 마크다운 헤딩이다.
        // 뒤의 공백을 요구하면 `##` 와 `##<탭>제목` 이 이름 검사를 통째로 빠져나가
        // 선언 줄이 본문에 섞인다.
        if let Some(heading) = trimmed
            .strip_prefix("##")
            .filter(|rest| !rest.starts_with('#'))
        {
            // modifier 를 **먼저** 잘라낸다. 순서를 반대로 하면 `// iknow` 대상의 백틱이
            // K105 로 오인되어 스펙 4.4 가 보장하는 합법 문서가 거부된다.
            let (heading_text, modifier) = split_modifier(heading);
            let name = heading_text.trim();

            // 헤딩 줄은 바로 아래에서 continue 하므로 짝 검사(K104)를 아예 받지 않는다.
            // 폴백이 없으므로 여기의 두 판정이 유일한 방어선이다.
            if heading_text.contains('`') {
                // 백틱이 든 이름은 CLI 인자로 주소를 댈 수 없다 (스펙 6.0).
                diagnostics.push(헤딩_백틱(&path, name, line_no));
            } else if name.is_empty() {
                // 이름이 없는 topic 도 같은 이유로 주소를 댈 수 없다.
                diagnostics.push(헤딩_이름_없음(&path, line_no));
            }

            // modifier 는 선언이지 서술이 아니므로 본문에서도 빠진다 (스펙 4.8).
            // 다만 본문 계약은 "헤딩 포함 원문" 이라 헤딩 줄 자체는 남는다.
            // `heading` 은 `raw` 의 접미사이므로 길이 차이로 잘라낼 자리를 얻는다.
            let body = raw[..raw.len() - heading.len() + heading_text.len()].trim_end();

            // topic 헤딩은 `// uncoded` 를 받을 수 있는 유일한 자리다 (스펙 4.5).
            let (uncoded, iknow) = match modifier {
                Some(text) => match 백틱_검사(&path, text, line_no)
                    .and_then(|_| parse_modifier(&path, text, line_no, true))
                {
                    Ok(parsed) => parsed,
                    Err(diagnostic) => {
                        diagnostics.push(diagnostic);
                        (false, Vec::new())
                    }
                },
                None => (false, Vec::new()),
            };

            topics.push(Topic {
                name: name.to_string(),
                body: body.to_string(),
                uncoded,
                iknow,
                refs: Vec::new(),
                exceptions: Vec::new(),
                covers: Vec::new(),
                line: line_no,
            });
            continue;
        }

        // import 는 파일 최상단의 선언이다 (스펙 4.7).
        //
        // **첫 topic 뒤의 판정 기준은 "그 줄이 최상단에 있었다면 합법 선언인가" 하나다** —
        // 접두사만 보면 `import 를 쓸 때는 …` 같은 합법 산문을 거부하고(이 저장소의 스펙
        // 문서 자신이 그렇게 쓴다), 그렇다고 산문으로 흘리면 주소의 백틱이 전부 심볼
        // 참조로 세어져 문서 경로 조각까지 미해결 심볼이 되고 그 진단이 일반 명사를
        // keyword 로 선언하라고 안내한다 (스펙 4.3 이 금지한 것). 그래서 판정을 같은
        // 파서에 맡긴다 — 기준이 한 곳에 있으므로 두 자리가 갈라질 수 없다.
        //
        // ponytail: 천장은 **문법 오류가 있으면서 자리도 틀린** import 다. 그 줄은 선언으로
        // 읽히지 않으므로 산문으로 남아 옛 증상이 그대로 나온다. 기준을 "`import ` 뒤에
        // 백틱" 으로 넓히면 그것도 잡히지만 `import `폐포` 가 깊으면 …` 같은 합법 산문을
        // 함께 거부한다 — 합법 입력 거부가 더 나쁘다. 실수 둘이 겹친 경우를 위해 실수
        // 하나짜리 문서를 깨지 않는다. 문법 오류가 든 줄까지 잡아야 하면 `import ` 뒤에
        // 백틱이 있고 **그 문서에 최상단 import 블록이 아예 없을 때만** 으로 좁힌다.
        if let Some(rest) = trimmed.strip_prefix("import ") {
            let 선언 = parse_import_line(&path, rest, line_no);
            if topics.is_empty() {
                match 선언 {
                    Ok(import) => imports.push(import),
                    Err(diagnostic) => diagnostics.push(diagnostic),
                }
                continue;
            }
            // 첫 topic 뒤인데 선언으로 읽힌다. 자리가 틀린 선언이다.
            if 선언.is_ok() {
                diagnostics.push(import_자리_오류(&path, line_no));
                continue;
            }
            // 선언으로 읽히지 않으므로 산문이다. 아래 본문 처리로 흘려보낸다.
        }

        // keyword 선언은 서술이 아니라 선언이므로 topic 본문에 담지 않는다 (스펙 4.8).
        if let Some(rest) = trimmed.strip_prefix("keyword ") {
            match parse_keyword_line(&path, rest, line_no) {
                Ok(keyword) => keywords.push(keyword),
                Err(diagnostic) => diagnostics.push(diagnostic),
            }
            continue;
        }

        // exception 선언도 서술이 아니므로 본문에서 빠진다 (스펙 4.8).
        if let Some(rest) = trimmed.strip_prefix("exception ") {
            // exception 의 의미는 그것을 선언한 topic 의 맥락에서 나오고 rev 핀도
            // 그 topic 의 해시다 (스펙 4.8). topic 밖에서는 선언이 성립하지 않는다.
            let Some(topic) = topics.last_mut() else {
                diagnostics.push(선언_위치_오류(
                    &path,
                    line_no,
                    "exception 은 topic 안에서만 선언할 수 있습니다. 이 선언은 어떤 topic 의 맥락에도 속하지 않습니다.".to_string(),
                ));
                continue;
            };
            match parse_exception_line(&path, rest, line_no) {
                Ok(exception) => topic.exceptions.push(exception),
                Err(diagnostic) => diagnostics.push(diagnostic),
            }
            continue;
        }

        // cover 선언도 마찬가지다.
        if let Some(rest) = trimmed.strip_prefix("cover ") {
            let Some(topic) = topics.last_mut() else {
                diagnostics.push(선언_위치_오류(
                    &path,
                    line_no,
                    "cover 는 topic 안에서만 선언할 수 있습니다. 이 선언은 어떤 topic 의 맥락에도 속하지 않습니다.".to_string(),
                ));
                continue;
            };
            match parse_cover_line(&path, rest, line_no) {
                Ok(name) => topic.covers.push((name, line_no)),
                Err(diagnostic) => diagnostics.push(diagnostic),
            }
            continue;
        }

        // 나머지 줄은 현재 topic 의 본문이다. topic 밖의 줄은 위에서 이미 K112 로 걸렸으므로
        // 짝 검사를 덧붙이지 않는다 — 존재할 수 없는 줄에 진단을 하나 더 얹는 소음이다.
        // 백틱을 쓰면서 topic 밖에 놓일 수 있는 줄은 import 와 keyword 뿐이고,
        // 그 둘은 각자의 파서가 백틱_검사 를 부른다.
        let Some(topic) = topics.last_mut() else {
            continue;
        };
        push_body_line(topic, raw);

        // 본문의 백틱 쌍은 전부 심볼 참조다.
        match scan_symbols(raw) {
            // 빈 백틱 쌍은 가리키는 심볼이 없다.
            Some(symbols) if symbols.iter().any(String::is_empty) => {
                diagnostics.push(빈_심볼(&path, line_no));
            }
            Some(symbols) => topic
                .refs
                .extend(symbols.into_iter().map(|symbol| (symbol, line_no))),
            None => diagnostics.push(백틱_짝_없음(&path, line_no)),
        }
    }

    // 닫히지 않은 펜스는 나머지 줄을 통째로 삼켜 topic 분할과 참조 수집을 조용히 끈다.
    if let Some((line, _)) = fence_open {
        diagnostics.push(펜스_미종료(&path, line));
    }

    // 진단이 하나라도 있으면 Document 를 만들지 않는다. 통과하지 못한 문서는 출력되지 않는다.
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }

    Ok(Document {
        path,
        description,
        imports,
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
        return Err(frontmatter_없음(path));
    };

    // 여는 `---` 다음에서 닫는 `---` 를 찾는다.
    let close = lines
        .iter()
        .skip(open + 1)
        .position(|line| line.trim() == "---")
        .map(|offset| open + 1 + offset);

    let Some(close) = close else {
        // 블록이 없는 경우와 fix 가 달라야 한다. 여기서 새 블록을 만들라고 하면
        // 그대로 적용했을 때 열린 `---` 이 남아 문서가 더 망가진다.
        return Err(frontmatter_미종료(path, open + 1));
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
/// 백틱 짝이 맞지 않으면 `K104`, 이름이나 한 줄 정의가 없으면 `K103`,
/// modifier 가 올바르지 않으면 `K110` 진단
fn parse_keyword_line(path: &DocPath, rest: &str, line_no: usize) -> Result<Keyword, Diagnostic> {
    // modifier 를 먼저 가른다. `// iknow` 대상은 한 줄 정의가 아니므로 rev 해시의
    // 입력(스펙 4.8)에 섞이면 핀이 조용히 어긋난다.
    let (rest, modifier) = split_modifier(rest);

    // 이름과 정의 양쪽의 백틱이 모두 닫혀 있어야 구분자를 신뢰할 수 있다.
    // 빈 백틱 쌍은 가리키는 심볼이 없다. 이름·정의·상세 어디에 있든 마찬가지다.
    백틱_검사(path, rest, line_no)?;

    // 이름 뒤의 `:` 가 이름과 정의를 가른다. 정의 안의 `:` 에 걸리면 안 되므로 첫 번째만 본다.
    let Some(colon) = find_outside_backticks(rest, ":") else {
        return Err(keyword_문법_오류(
            path,
            line_no,
            "keyword 선언에 한 줄 정의가 없습니다.".to_string(),
        ));
    };

    let name_part = &rest[..colon];
    let definition_part = rest[colon + 1..].trim();

    // 이름은 백틱으로 감싼 조각들이며 `.` 로 계층을 이룬다.
    // `colon` 은 백틱 밖에서 찾았으므로 그 앞은 반드시 균형이 맞는다.
    let names = scan_symbols(name_part).expect("백틱 밖의 `:` 앞은 균형이 맞는다");
    if names.is_empty() {
        return Err(keyword_문법_오류(
            path,
            line_no,
            "keyword 이름이 백틱으로 감싸여 있지 않습니다.".to_string(),
        ));
    }

    // 줄 끝의 `#`상세 topic`` 은 선택이다. 없으면 전부가 한 줄 정의다.
    // 마커는 공백 뒤에만 온다 (스펙 4.3 예시). 이 조건이 없으면 `C#` 같은 심볼의
    // `#` 와 그 닫는 백틱이 마커로 오인되어 합법 선언이 거부된다.
    // ponytail: 심볼 이름이 ` #` 로 끝나고 그 뒤가 심볼 하나로 깔끔히 끝나는
    // 경우(`` `A #`B` ``)는 여전히 오인한다. 그 표기가 실제로 나타나면 마커 탐색을
    // find_definition_colon 처럼 백틱 안팎을 추적하는 스캔으로 올린다.
    let marker = definition_part
        .rfind("#`")
        .filter(|&marker| definition_part[..marker].ends_with(' '));

    let (definition, detail) = match marker.and_then(|marker| {
        // 뒤쪽이 심볼 하나로 파싱되지 않으면 그 `#` 는 애초에 마커가 아니었다.
        scan_symbols(&definition_part[marker + 1..]).map(|symbols| (marker, symbols))
    }) {
        Some((marker, mut tail_symbols)) => {
            let tail = &definition_part[marker + 1..];
            // 마커 뒤는 상세 심볼 하나로 끝나야 한다. 텍스트가 더 남아 있는데도
            // 마커로 인정하면 그 텍스트가 정의에서 조용히 사라진다.
            // 끝의 백틱이 이스케이프된 것이면 심볼을 닫은 것이 아니다 — 그것을 세면
            // 줄 끝에 `\`` 하나를 붙이는 것만으로 이 검사를 우회할 수 있다.
            if tail_symbols.len() != 1 || !tail.ends_with('`') || tail.ends_with("\\`") {
                return Err(keyword_문법_오류(
                    path,
                    line_no,
                    "상세 topic 마커 뒤에 텍스트가 남아 있습니다. 마커는 줄 끝에만 올 수 있습니다."
                        .to_string(),
                ));
            }
            (definition_part[..marker].trim(), tail_symbols.pop())
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

    // 정의 안의 백틱도 심볼 참조다 (스펙 4.2 "본문과 선언부의 모든 백틱").
    // 이름과 상세 topic 은 각각 name·detail 이 이미 들고 있으므로 refs 에 넣지 않는다.
    // 마커가 인정되면 tail 은 정확히 한 쌍이므로, 균형 잡힌 definition_part 에서
    // 잘라낸 앞부분도 반드시 균형이 맞는다.
    let refs = scan_symbols(definition)
        .expect("균형 잡힌 정의에서 한 쌍을 뗀 나머지도 균형이 맞는다")
        .into_iter()
        .map(|symbol| (symbol, line_no))
        .collect();

    // keyword 선언은 `// uncoded` 를 받지 않는다 — 대응 코드 유무는 topic 의 속성이다.
    let iknow = match modifier {
        Some(text) => {
            백틱_검사(path, text, line_no)?;
            parse_modifier(path, text, line_no, false)?.1
        }
        None => Vec::new(),
    };

    Ok(Keyword {
        name: KeywordName(names),
        definition: definition.to_string(),
        detail,
        iknow,
        refs,
        line: line_no,
    })
}

/// `import ` 접두사를 뗀 나머지를 [`Import`] 로 파싱한다.
///
/// `as` 와 `rev` 는 각각 선택이며 세 종류의 심볼 모두 핀을 가질 수 있다 (스펙 4.7).
///
/// # 매개변수
/// - `path`: 진단에 담을 문서 경로
/// - `rest`: `import ` 뒤의 나머지 텍스트
/// - `line_no`: 선언이 등장한 줄 번호 (1-based)
///
/// # 반환값
/// 파싱한 [`Import`]
///
/// # 오류
/// 백틱 짝이 맞지 않으면 `K104`, 빈 백틱 쌍이면 `K108`,
/// 대상·별칭·핀 문법이 맞지 않으면 `K109` 진단
fn parse_import_line(path: &DocPath, rest: &str, line_no: usize) -> Result<Import, Diagnostic> {
    // import 줄의 심볼 주소는 백틱으로 쓰므로 짝 검사를 반드시 받아야 한다.
    백틱_검사(path, rest, line_no)?;
    let rest = rest.trim();

    // `rev "<핀>"` 은 줄 끝의 선택 토큰이다. 백틱 밖에서만 찾아 별칭 안의 낱말과 구분한다.
    let (rest, rev) = match find_outside_backticks(rest, " rev ") {
        Some(at) => {
            let 원문 = rest[at + " rev ".len()..].trim();
            // 핀은 큰따옴표로 감싼 값 **하나**다. 벗긴 값에 따옴표가 남으면 뒤에 토큰이
            // 더 붙어 있다는 뜻이고, 그대로 담으면 그 텍스트가 통째로 해시에 섞인다.
            let Some(value) = 원문
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .filter(|value| !value.contains('"'))
            else {
                return Err(import_문법_오류(
                    path,
                    line_no,
                    format!(
                        "rev 핀 {원문} 을 큰따옴표로 감싼 값 하나로 읽을 수 없습니다. rev 는 줄 끝에 오고 `as` 는 그 앞에 옵니다."
                    ),
                ));
            };
            (&rest[..at], Some(value.to_string()))
        }
        None => (rest, None),
    };

    // `as `<별칭>`` 도 선택이다. 별칭은 백틱으로 감싼 이름 하나다.
    let (target, alias) = match find_outside_backticks(rest, " as ") {
        Some(at) => {
            let alias = rest[at + " as ".len()..].trim();
            let Some(name) = 백틱_이름_하나(alias) else {
                return Err(import_문법_오류(
                    path,
                    line_no,
                    format!("as 뒤의 별칭 \"{alias}\" 이 백틱으로 감싼 이름 하나가 아닙니다."),
                ));
            };
            (&rest[..at], Some(name))
        }
        None => (rest, None),
    };

    let target = parse_symbol_ref(target.trim())
        .map_err(|message| import_문법_오류(path, line_no, message))?;

    Ok(Import {
        target,
        alias,
        rev,
        line: line_no,
    })
}

/// `exception ` 접두사를 뗀 나머지를 [`Exception`] 으로 파싱한다.
///
/// # 매개변수
/// - `path`: 진단에 담을 문서 경로
/// - `rest`: `exception ` 뒤의 나머지 텍스트
/// - `line_no`: 선언이 등장한 줄 번호 (1-based)
///
/// # 반환값
/// 파싱한 [`Exception`]
///
/// # 오류
/// 백틱 짝이 맞지 않으면 `K104`, 빈 백틱 쌍이면 `K108`,
/// 선언 문법이 맞지 않으면 `K111`, modifier 가 올바르지 않으면 `K110` 진단
fn parse_exception_line(
    path: &DocPath,
    rest: &str,
    line_no: usize,
) -> Result<Exception, Diagnostic> {
    // modifier 를 먼저 가른다. iknow 대상의 백틱이 선언 문법 검사에 섞이면 안 된다.
    let (declaration, modifier) = split_modifier(rest);
    백틱_검사(path, declaration, line_no)?;
    let declaration = declaration.trim();

    // `pending` 은 선택 토큰이다. 떼고 나면 나머지는 백틱 이름 하나여야 한다 —
    // 그 밖의 텍스트가 남으면 조용히 사라지므로 error 다.
    let (이름_부분, pending) = match declaration.strip_suffix(" pending") {
        Some(head) => (head, true),
        None => (declaration, false),
    };
    let Some(name) = 백틱_이름_하나(이름_부분) else {
        return Err(선언_문법_오류(
            path,
            line_no,
            format!(
                "exception 선언 \"{declaration}\" 이 `이름` 또는 `이름` pending 형식이 아닙니다."
            ),
        ));
    };

    // exception 선언도 `// iknow` 를 받는 세 자리 중 하나다 (스펙 4.4).
    let iknow = match modifier {
        Some(text) => {
            백틱_검사(path, text, line_no)?;
            parse_modifier(path, text, line_no, false)?.1
        }
        None => Vec::new(),
    };

    Ok(Exception {
        name,
        pending,
        iknow,
        line: line_no,
    })
}

/// `cover ` 접두사를 뗀 나머지에서 커버 대상 이름을 읽는다.
///
/// `cover` 는 `// iknow` 가 붙는 자리가 아니다 — 스펙 4.4 는 keyword·topic 헤딩·exception
/// 셋만 든다.
///
/// # 매개변수
/// - `path`: 진단에 담을 문서 경로
/// - `rest`: `cover ` 뒤의 나머지 텍스트
/// - `line_no`: 선언이 등장한 줄 번호 (1-based)
///
/// # 반환값
/// 커버 대상 이름
///
/// # 오류
/// 백틱 짝이 맞지 않으면 `K104`, 빈 백틱 쌍이면 `K108`, 선언 문법이 맞지 않으면 `K111` 진단
fn parse_cover_line(path: &DocPath, rest: &str, line_no: usize) -> Result<String, Diagnostic> {
    백틱_검사(path, rest, line_no)?;

    // modifier 를 먼저 가려낸다. 그러지 않으면 iknow 대상의 백틱까지 선언에 섞여
    // "대상은 하나입니다" 라는 엉뚱한 원인을 대게 된다 — 사용자가 쓴 대상은 하나다.
    if split_modifier(rest).1.is_some() {
        return Err(선언_문법_오류(
            path,
            line_no,
            "cover 에는 modifier 를 붙일 수 없습니다. `// iknow` 는 keyword 선언·topic 헤딩·exception 선언 세 자리에만 붙습니다 (스펙 4.4)."
                .to_string(),
        ));
    }

    let declaration = rest.trim();

    // 대상은 백틱으로 감싼 이름 하나다. 뒤에 텍스트가 남으면 조용히 버리지 않는다.
    백틱_이름_하나(declaration).ok_or_else(|| {
        선언_문법_오류(
            path,
            line_no,
            format!("cover 선언 \"{declaration}\" 이 `이름` 형식이 아닙니다. 대상은 하나입니다."),
        )
    })
}

/// modifier 텍스트(`//` 뒤)를 해석한다.
///
/// # 매개변수
/// - `path`: 진단에 담을 문서 경로
/// - `text`: `//` 뒤의 텍스트. 앞뒤 공백이 제거되어 있어야 한다
/// - `line_no`: modifier 가 붙은 줄 번호 (1-based)
/// - `allow_uncoded`: `uncoded` 를 받을 수 있는 자리인지. topic 헤딩만 참이다
///
/// # 반환값
/// `(uncoded 여부, iknow 대상들)`
///
/// # 오류
/// 알 수 없는 modifier·대상 없는 `iknow`·자리에 맞지 않는 `uncoded` 면 `K110` 진단
fn parse_modifier(
    path: &DocPath,
    text: &str,
    line_no: usize,
    allow_uncoded: bool,
) -> Result<(bool, Vec<SymbolRef>), Diagnostic> {
    // `// uncoded` 는 대응 코드가 없는 것이 정상인 topic 을 표시한다 (스펙 4.5).
    if text == "uncoded" {
        if !allow_uncoded {
            return Err(modifier_문법_오류(
                path,
                line_no,
                "`// uncoded` 는 topic 헤딩에만 붙습니다.".to_string(),
            ));
        }
        return Ok((true, Vec::new()));
    }

    // ponytail: 한 줄에 modifier 하나만 받는다. 스펙 4.4·4.5 의 예시가 전부 하나뿐이라
    // `// uncoded // iknow …` 는 문법이 아니다. 둘을 함께 붙일 일이 생기면 그때 올린다.
    let targets = match text.strip_prefix("iknow") {
        // `iknow` 뒤에는 공백으로 구분한 대상 목록이 온다. `iknowX` 는 다른 낱말이다.
        Some(targets) if targets.is_empty() || targets.starts_with(char::is_whitespace) => {
            targets.trim()
        }
        _ => {
            return Err(modifier_문법_오류(
                path,
                line_no,
                format!("알 수 없는 modifier 입니다: \"{text}\""),
            ));
        }
    };

    // 대상이 없는 `iknow` 는 아무것도 인지하지 않는다.
    if targets.is_empty() {
        return Err(modifier_문법_오류(
            path,
            line_no,
            "`// iknow` 에 대상이 없습니다.".to_string(),
        ));
    }

    let mut iknow = Vec::new();
    let mut rest = targets;

    // 대상은 쉼표로 나열한다 (스펙 4.4). 백틱 안의 쉼표는 심볼 이름의 일부다.
    loop {
        let (piece, next) = match find_outside_backticks(rest, ",") {
            Some(at) => (&rest[..at], Some(&rest[at + 1..])),
            None => (rest, None),
        };
        let piece = piece.trim();
        // 빈 자리를 그대로 넘기면 원문에 없는 `""` 를 가리켜 고칠 자리를 못 찾게 한다.
        if piece.is_empty() {
            return Err(modifier_문법_오류(
                path,
                line_no,
                "`// iknow` 의 대상 목록에 빈 자리가 있습니다. 쉼표가 겹쳤거나 마지막 대상 뒤에 쉼표가 남았습니다."
                    .to_string(),
            ));
        }
        iknow.push(
            parse_symbol_ref(piece)
                .map_err(|message| modifier_문법_오류(path, line_no, message))?,
        );
        match next {
            Some(next) => rest = next,
            None => break,
        }
    }

    Ok((false, iknow))
}

/// 심볼 주소 하나를 [`SymbolRef`] 로 파싱한다. import 대상과 iknow 대상이 같은 문법이다.
///
/// 문서 경로와 심볼 이름은 첫 `.`·`#`·`!` 가 가르며 그 구분자가 종류를 정한다.
/// `.` 는 그 뒤로 키워드 계층에도 쓰이므로 첫 번째만 경계다 (스펙 4.1).
///
/// # 매개변수
/// - `text`: `` `docs`/`A`.`결제` `` 형태의 주소. 앞뒤 공백이 제거되어 있어야 한다
///
/// # 반환값
/// 파싱한 [`SymbolRef`]
///
/// # 오류
/// 문법이 맞지 않으면 무엇이 틀렸는지 알리는 한 문장. 호출자가 자기 진단 코드로 감싼다
fn parse_symbol_ref(text: &str) -> Result<SymbolRef, String> {
    let Some((at, separator, kind)) = [
        (".", SymbolKind::Keyword),
        ("#", SymbolKind::Topic),
        ("!", SymbolKind::Exception),
    ]
    .into_iter()
    .filter_map(|(separator, kind)| {
        find_outside_backticks(text, separator).map(|at| (at, separator, kind))
    })
    .min_by_key(|&(at, _, _)| at) else {
        return Err(format!(
            "심볼 주소 \"{text}\" 에 keyword `.` · topic `#` · exception `!` 구분자가 없습니다."
        ));
    };

    // 구분자를 백틱 밖에서 찾았으므로 그 앞뒤는 각각 백틱 균형이 맞는다.
    let (Some(doc), Some(name)) = (
        scan_symbols(&text[..at]),
        scan_symbols(&text[at + separator.len()..]),
    ) else {
        return Err(format!("심볼 주소 \"{text}\" 의 백틱 짝이 맞지 않습니다."));
    };

    if doc.is_empty() {
        return Err(format!("심볼 주소 \"{text}\" 에 문서 경로가 없습니다."));
    }
    if name.is_empty() {
        return Err(format!("심볼 주소 \"{text}\" 에 심볼 이름이 없습니다."));
    }
    // 계층을 갖는 것은 keyword 뿐이다 (스펙 4.3).
    if kind != SymbolKind::Keyword && name.len() > 1 {
        return Err(format!(
            "`{separator}` 뒤에는 이름 조각이 하나만 옵니다. 계층은 keyword 의 `.` 만 갖습니다."
        ));
    }

    // 읽어낸 결과로 정규 표기를 되만들어 원문과 대조한다. 어긋난 원문을 통과시키면
    // 그 차이가 어디에도 남지 않고 조용히 사라진다.
    // 무엇이 틀렸는지 단정하지 않고 "이렇게 읽혔다" 만 말한다 — 잉여 텍스트일 수도,
    // 자리가 틀린 구분자일 수도 있어서 원인을 특정하면 진단이 거짓말을 한다.
    let canonical = format!(
        "{}{separator}{}",
        백틱_결합(&doc, "/"),
        백틱_결합(&name, ".")
    );
    if canonical != text {
        return Err(format!(
            "심볼 주소 \"{text}\" 가 `{canonical}` 로 읽혔습니다. 경로는 `/`, keyword 진입과 계층은 `.`, topic 은 `#`, exception 은 `!` 로만 잇습니다."
        ));
    }

    Ok(SymbolRef {
        doc: DocPath(doc),
        kind,
        name,
    })
}

/// 이름 조각들을 각각 백틱으로 감싸고 `separator` 로 잇는다.
///
/// # 매개변수
/// - `names`: 이름 조각들
/// - `separator`: 조각 사이에 넣을 구분자
///
/// # 반환값
/// `` `a`/`b` `` 형태의 문자열
fn 백틱_결합(names: &[String], separator: &str) -> String {
    names
        .iter()
        .map(|name| format!("`{name}`"))
        .collect::<Vec<String>>()
        .join(separator)
}

/// 한 줄의 백틱 쌍을 검사하고 그 안의 이름들을 돌려준다.
///
/// # 매개변수
/// - `path`: 진단에 담을 문서 경로
/// - `text`: 검사할 텍스트
/// - `line_no`: 텍스트가 있는 줄 번호 (1-based)
///
/// # 반환값
/// 백틱 쌍 안의 이름들
///
/// # 오류
/// 짝이 맞지 않으면 `K104`, 빈 쌍이 있으면 `K108` 진단
fn 백틱_검사(path: &DocPath, text: &str, line_no: usize) -> Result<Vec<String>, Diagnostic> {
    let Some(symbols) = scan_symbols(text) else {
        return Err(백틱_짝_없음(path, line_no));
    };
    // 빈 백틱 쌍은 가리키는 심볼이 없다.
    if symbols.iter().any(String::is_empty) {
        return Err(빈_심볼(path, line_no));
    }
    Ok(symbols)
}

/// topic 밖에 놓여도 되는 줄인지 판정한다 (스펙 3절).
///
/// frontmatter 는 본문 순회에 들어오기 전에 처리되므로 여기서 볼 일이 없다.
/// `exception`·`cover` 는 topic 밖에서 `K111` 이 더 구체적으로 잡으므로 여기서 제외한다 —
/// 겹쳐 세면 한 줄에 진단이 둘이 된다.
///
/// # 매개변수
/// - `trimmed`: 앞 공백을 뗀 한 줄
///
/// # 반환값
/// topic 밖에 있어도 `K112` 를 내지 않는 줄이면 참
fn topic_밖_허용(trimmed: &str) -> bool {
    trimmed.is_empty()
        // `##` 는 여기서 topic 을 여니 밖이 아니다. `###` 이하는 topic 을 열지 못한다.
        || trimmed
            .strip_prefix("##")
            .is_some_and(|rest| !rest.starts_with('#'))
        || trimmed.starts_with("import ")
        || trimmed.starts_with("keyword ")
        || trimmed.starts_with("exception ")
        || trimmed.starts_with("cover ")
}

/// 선언 줄을 선언부와 modifier 부로 가른다.
///
/// modifier 의 `//` 는 **공백 뒤에** 오고 **알려진 modifier 낱말이 뒤따를 때만** 인정한다.
/// 두 조건 중 하나라도 빠지면 `https://`·`//cdn.example.com`·`50 // 100` 같은 본문
/// 표기를 modifier 로 가로채고, kang 에는 `//` 이스케이프가 없으므로 그런 정의를
/// 쓸 방법이 아예 없어진다.
///
/// # 매개변수
/// - `text`: 선언 줄에서 `keyword `·`##` 등의 접두사를 뗀 나머지
///
/// # 반환값
/// `(선언부, modifier 텍스트)`. modifier 가 없으면 두 번째가 `None`
fn split_modifier(text: &str) -> (&str, Option<&str>) {
    let mut base = 0;

    // 백틱 밖의 `//` 를 앞에서부터 훑는다. 백틱 밖에서 찾았다는 것은 그 앞의 백틱이
    // 균형을 이룬다는 뜻이므로, 후보를 지나 이어서 스캔해도 안팎 판정이 어긋나지 않는다.
    while let Some(offset) = find_outside_backticks(&text[base..], "//") {
        let at = base + offset;
        let candidate = text[at + 2..].trim();
        if text[..at].ends_with(char::is_whitespace) && modifier_낱말(candidate) {
            return (&text[..at], Some(candidate));
        }
        base = at + 2;
    }

    (text, None)
}

/// modifier 로 인정하는 낱말로 시작하는지 본다.
///
/// 낱말까지만 보고 그 **뒤가 올바른지는 보지 않는다.** `uncoded 뭐` 처럼 낱말은 맞고
/// 형태가 틀린 것은 modifier 자리로 넘겨 `K110` 을 받게 해야 한다 — 여기서 걸러 버리면
/// 그 오타가 조용히 topic 이름의 일부가 된다.
///
/// # 매개변수
/// - `candidate`: `//` 뒤의 텍스트 (앞뒤 공백이 제거되어 있어야 한다)
///
/// # 반환값
/// `uncoded` 나 `iknow` 낱말로 시작하면 참
fn modifier_낱말(candidate: &str) -> bool {
    ["uncoded", "iknow"].into_iter().any(|word| {
        candidate
            .strip_prefix(word)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
    })
}

/// 텍스트가 백틱으로 감싼 이름 **하나뿐**인지 보고 그 이름을 돌려준다.
///
/// import 별칭·exception·cover 가 같은 판정을 쓴다. 세 곳에 복제하면 서로 어긋난다.
///
/// # 매개변수
/// - `text`: 검사할 텍스트 (앞뒤 공백이 제거되어 있어야 한다)
///
/// # 반환값
/// 백틱 쌍 하나로 정확히 감싸여 있으면 그 안의 이름. 아니면 `None`
fn 백틱_이름_하나(text: &str) -> Option<String> {
    let names = scan_symbols(text)?;
    match names.as_slice() {
        [name] if text == format!("`{name}`") => Some(name.clone()),
        _ => None,
    }
}

/// 한 줄에서 백틱 쌍 안의 심볼 이름을 등장 순서대로 뽑는다.
///
/// `\`` 로 이스케이프한 백틱은 리터럴이므로 구분자로 세지 않는다 (스펙 4.2).
/// 스펙이 정의하는 이스케이프는 이것 하나뿐이다 — `\\` 는 이스케이프가 아니라
/// 역슬래시 문자 그대로이므로, 그 뒤의 `\`` 는 여전히 백틱 리터럴이다.
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

/// 백틱 **밖에서** `needle` 이 처음 나타나는 바이트 위치를 찾는다.
///
/// 백틱 안의 문자는 심볼 이름의 일부이므로 구분자로 세지 않는다.
/// 이스케이프 규칙은 [`scan_symbols`] 와 같다 — `` \` `` 는 리터럴이라 안팎을 뒤집지 않는다.
///
/// # 매개변수
/// - `text`: 검사할 텍스트
/// - `needle`: 찾을 구분자. ASCII 로 시작해야 결과가 문자 경계에 놓인다
///
/// # 반환값
/// 백틱 밖에 있는 첫 `needle` 의 바이트 위치. 없으면 `None`
fn find_outside_backticks(text: &str, needle: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut in_backtick = false;
    let mut index = 0;

    // 백틱 안팎을 추적하면서 백틱 밖의 needle 을 찾는다.
    while index < bytes.len() {
        match bytes[index] {
            // 이스케이프된 백틱은 구분자가 아니다.
            b'\\' if bytes.get(index + 1) == Some(&b'`') => index += 2,
            b'`' => {
                in_backtick = !in_backtick;
                index += 1;
            }
            // 백틱 밖의 것만 구분자다.
            _ if !in_backtick && bytes[index..].starts_with(needle.as_bytes()) => {
                return Some(index);
            }
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

/// frontmatter 블록이 아예 없다는 진단을 만든다.
///
/// # 매개변수
/// - `path`: 대상 문서 경로
///
/// # 반환값
/// `K101` 진단
fn frontmatter_없음(path: &DocPath) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: "K101",
        message: "frontmatter 블록이 없습니다.".to_string(),
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

/// frontmatter 블록이 닫히지 않았다는 진단을 만든다.
///
/// 블록을 새로 만들라고 하면 열린 `---` 이 남아 문서가 더 망가지므로,
/// 없는 경우와 별도의 fix 를 준다.
///
/// # 매개변수
/// - `path`: 대상 문서 경로
/// - `line`: 여는 `---` 의 줄 번호
///
/// # 반환값
/// `K101` 진단
fn frontmatter_미종료(path: &DocPath, line: usize) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: "K101",
        message: "frontmatter 블록이 닫히지 않았습니다.".to_string(),
        locations: vec![Location {
            doc: path.clone(),
            line,
            note: "여기서 연 frontmatter 가 닫히지 않았습니다".to_string(),
        }],
        fixes: vec![Fix {
            kind: FixKind::Edit,
            doc: Some(path.clone()),
            action:
                "이 frontmatter 블록의 마지막 키 줄 다음에 블록을 닫는 `---` 줄을 추가하세요. 여는 `---` 을 새로 만들지 마세요"
                    .to_string(),
        }],
    }
}

/// 코드 펜스가 닫히지 않았다는 진단을 만든다.
///
/// # 매개변수
/// - `path`: 대상 문서 경로
/// - `line`: 여는 펜스의 줄 번호
///
/// # 반환값
/// `K106` 진단
fn 펜스_미종료(path: &DocPath, line: usize) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: "K106",
        message: "코드 펜스가 닫히지 않았습니다. 뒤따르는 줄이 전부 코드로 취급되어 topic 과 심볼 참조가 인식되지 않습니다."
            .to_string(),
        locations: vec![Location {
            doc: path.clone(),
            line,
            note: "여기서 연 펜스가 끝나지 않았습니다".to_string(),
        }],
        fixes: vec![Fix {
            kind: FixKind::Edit,
            doc: Some(path.clone()),
            action: "코드 블록이 끝나는 자리에 펜스를 닫는 ``` 줄을 추가하세요".to_string(),
        }],
    }
}

/// topic 헤딩에 이름이 없다는 진단을 만든다.
///
/// # 매개변수
/// - `path`: 대상 문서 경로
/// - `line`: 헤딩이 있는 줄 번호
///
/// # 반환값
/// `K107` 진단
fn 헤딩_이름_없음(path: &DocPath, line: usize) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: "K107",
        message: "topic 헤딩에 이름이 없습니다. 이 topic 은 CLI 로 주소를 댈 수 없습니다."
            .to_string(),
        locations: vec![Location {
            doc: path.clone(),
            line,
            note: "이 topic 헤딩".to_string(),
        }],
        fixes: vec![Fix {
            kind: FixKind::Edit,
            doc: Some(path.clone()),
            action: "이 `##` 뒤에 topic 이름을 평문으로 쓰세요. 이름은 kang show 의 주소가 됩니다"
                .to_string(),
        }],
    }
}

/// 백틱 쌍 안이 비어 있다는 진단을 만든다.
///
/// # 매개변수
/// - `path`: 대상 문서 경로
/// - `line`: 빈 쌍이 있는 줄 번호
///
/// # 반환값
/// `K108` 진단
fn 빈_심볼(path: &DocPath, line: usize) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: "K108",
        message: "백틱 쌍 안이 비어 있어 가리키는 심볼이 없습니다.".to_string(),
        locations: vec![Location {
            doc: path.clone(),
            line,
            note: "이 줄의 빈 백틱 쌍".to_string(),
        }],
        fixes: vec![Fix {
            kind: FixKind::Edit,
            doc: Some(path.clone()),
            action: "심볼 참조는 `<이름>` 처럼 백틱 하나로 감싸세요. 마크다운의 이중 백틱 표기(``…``)는 빈 참조가 되므로 쓰지 마세요"
                .to_string(),
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

/// import 선언 문법이 올바르지 않다는 진단을 만든다.
///
/// # 매개변수
/// - `path`: 대상 문서 경로
/// - `line`: 선언이 있는 줄 번호
/// - `message`: 무엇이 틀렸는지 알리는 한 문장
///
/// # 반환값
/// `K109` 진단
fn import_문법_오류(path: &DocPath, line: usize, message: String) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: "K109",
        message,
        locations: vec![Location {
            doc: path.clone(),
            line,
            note: "이 import 선언".to_string(),
        }],
        fixes: vec![Fix {
            kind: FixKind::Edit,
            doc: Some(path.clone()),
            action: "이 줄을 `import `<경로>`/`<문서>`.`<keyword>`` 형식으로 고치세요. topic 은 `#`, exception 은 `!` 로 가리킵니다. 별칭은 ` as `<별칭>``, 핀은 ` rev \"<해시>\"` 로 덧붙입니다".to_string(),
        }],
    }
}

/// modifier 문법이 올바르지 않다는 진단을 만든다.
///
/// # 매개변수
/// - `path`: 대상 문서 경로
/// - `line`: modifier 가 붙은 줄 번호
/// - `message`: 무엇이 틀렸는지 알리는 한 문장
///
/// # 반환값
/// `K110` 진단
fn modifier_문법_오류(path: &DocPath, line: usize, message: String) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: "K110",
        message,
        locations: vec![Location {
            doc: path.clone(),
            line,
            note: "이 줄의 modifier".to_string(),
        }],
        fixes: vec![Fix {
            kind: FixKind::Edit,
            doc: Some(path.clone()),
            action: "이 줄 끝의 modifier 를 `// iknow `<경로>`/`<문서>`.`<이름>`` 로 고치세요. 대상이 여럿이면 쉼표로 나열합니다. topic 헤딩이라면 `// uncoded` 도 쓸 수 있습니다. kang 에는 주석 문법이 없으므로 그 밖의 `//` 는 지우세요".to_string(),
        }],
    }
}

/// exception / cover 선언이 **놓인 자리**가 틀렸다는 진단을 만든다.
///
/// # 매개변수
/// - `path`: 대상 문서 경로
/// - `line`: 선언이 있는 줄 번호
/// - `message`: 무엇이 틀렸는지 알리는 한 문장
///
/// # 반환값
/// `K111` 진단
fn 선언_위치_오류(path: &DocPath, line: usize, message: String) -> Diagnostic {
    선언_오류(
        path,
        line,
        message,
        "이 선언을 `##` 헤딩으로 시작하는 topic 안으로 옮기세요".to_string(),
    )
}

/// exception / cover 선언의 **문법**이 틀렸다는 진단을 만든다.
///
/// 위치를 고치라고 하지 않는다 — 이미 topic 안에 있는 줄에는 적용할 대상이 없다.
///
/// # 매개변수
/// - `path`: 대상 문서 경로
/// - `line`: 선언이 있는 줄 번호
/// - `message`: 무엇이 틀렸는지 알리는 한 문장
///
/// # 반환값
/// `K111` 진단
fn 선언_문법_오류(path: &DocPath, line: usize, message: String) -> Diagnostic {
    선언_오류(
        path,
        line,
        message,
        "이 줄을 `exception `<이름>`` (정책이 아직 없으면 뒤에 ` pending`) 또는 `cover `<이름>`` 형식으로 고치세요. 두 선언 모두 이름 하나만 받고 cover 는 modifier 를 받지 않습니다".to_string(),
    )
}

/// exception / cover 선언 진단의 공통 뼈대.
///
/// # 매개변수
/// - `path`: 대상 문서 경로
/// - `line`: 선언이 있는 줄 번호
/// - `message`: 무엇이 틀렸는지 알리는 한 문장
/// - `action`: 적용할 수정 한 문장
///
/// # 반환값
/// `K111` 진단
fn 선언_오류(path: &DocPath, line: usize, message: String, action: String) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: "K111",
        message,
        locations: vec![Location {
            doc: path.clone(),
            line,
            note: "이 exception / cover 선언".to_string(),
        }],
        fixes: vec![Fix {
            kind: FixKind::Edit,
            doc: Some(path.clone()),
            action,
        }],
    }
}

/// topic 밖 내용 진단이 가리키는 위치 하나를 만든다.
///
/// # 매개변수
/// - `path`: 대상 문서 경로
/// - `line`: 위반한 줄 번호
/// - `first`: 이 진단의 첫 위치인지
///
/// # 반환값
/// 줄과 역할 설명을 담은 [`Location`]
fn topic_밖_위치(path: &DocPath, line: usize, first: bool) -> Location {
    Location {
        doc: path.clone(),
        line,
        note: if first {
            "여기서부터 어떤 topic 에도 속하지 않는 내용이 시작합니다".to_string()
        } else {
            "이 줄도 어떤 topic 에도 속하지 않습니다".to_string()
        },
    }
}

/// import 선언이 첫 topic 뒤에 있다는 진단을 만든다 (스펙 4.7 "파일 최상단").
///
/// **이 줄이 산문이 아니라 선언임은 호출자가 [`parse_import_line`] 으로 이미 확인했다.**
/// 그것이 이 진단의 유일한 발화 조건이며, 접두사만 보면 합법 산문을 거부한다.
///
/// 진단하지 않고 본문으로 흘리면 주소의 백틱이 전부 심볼 참조로 세어져 문서 경로
/// 조각까지 미해결 심볼이 되고, `K001` 의 fix 가 그 일반 명사를 keyword 로 선언하라고
/// 안내한다. 스펙 4.3 이 선언하지 말라고 못박은 이름이 그렇게 SoT 에 박힌다.
///
/// # 매개변수
/// - `path`: 대상 문서 경로
/// - `line`: 자리가 틀린 import 선언이 있는 줄 번호
///
/// # 반환값
/// `K114` 진단
fn import_자리_오류(path: &DocPath, line: usize) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: "K114",
        message: "import 선언이 첫 topic 뒤에 있습니다. import 는 파일 최상단에 모아 씁니다 (스펙 4.7). 이 자리에서는 심볼을 당겨 오지 못합니다.".to_string(),
        locations: vec![Location {
            doc: path.clone(),
            line,
            note: "이 import 선언".to_string(),
        }],
        // **줄 번호를 좌표로 쓰지 않는다** (ADR-0003). 옮길 자리는 첫 `##` 헤딩과의
        // 앞뒤 관계로 말한다 — 그것은 문서를 고쳐 줄이 밀려도 변하지 않는다.
        fixes: vec![Fix {
            kind: FixKind::Edit,
            doc: Some(path.clone()),
            action: "이 줄을 파일 최상단의 import 블록으로, 첫 `##` 헤딩보다 앞에 옮기세요"
                .to_string(),
        }],
    }
}

/// topic 밖에 내용이 있다는 진단을 만든다 (스펙 3절).
///
/// 연속된 줄은 한 덩어리로 보고 첫 줄에서 한 번만 낸다.
///
/// # 매개변수
/// - `path`: 대상 문서 경로
/// - `line`: 덩어리가 시작하는 줄 번호
///
/// # 반환값
/// `K112` 진단
fn topic_밖_내용(path: &DocPath, line: usize) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: "K112",
        message: "topic 밖에는 내용을 둘 수 없습니다. 이 내용은 어떤 rev 해시에도 들어가지 않고 kang show 로도 볼 수 없습니다."
            .to_string(),
        locations: vec![topic_밖_위치(path, line, true)],
        fixes: vec![Fix {
            kind: FixKind::Edit,
            doc: Some(path.clone()),
            action: "이 내용을 `##` 헤딩으로 시작하는 topic 안으로 옮기세요. 문서 전체를 소개하는 한 문장이라면 frontmatter 의 `description` 으로 접고, 문서 제목 줄이라면 지웁니다 — 문서의 식별자는 파일 경로 하나뿐입니다. 선언을 쓰려던 것이라면 `import ` 또는 `keyword ` 로 시작하는지 확인하세요".to_string(),
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
            note: "이 줄에 닫히지 않은 여는 백틱이 있습니다 (이스케이프된 `\\`` 는 세지 않습니다)"
                .to_string(),
        }],
        fixes: vec![Fix {
            kind: FixKind::Edit,
            doc: Some(path.clone()),
            action: "이 줄의 심볼 참조를 `<이름>` 처럼 백틱 쌍으로 닫으세요. 문자 그대로의 백틱이라면 `\\`` 로 이스케이프합니다".to_string(),
        }],
    }
}
