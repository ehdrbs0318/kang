// `parse::parse_document` 의 frontmatter / keyword / topic / import / exception /
// cover / modifier 파싱을 검증하는 통합 테스트.
use kang::ast::{DocPath, Severity, SymbolKind};
use kang::parse;

/// 테스트가 공용으로 쓰는 문서 경로 `docs/A`.
fn 문서경로() -> DocPath {
    DocPath(vec!["docs".to_string(), "A".to_string()])
}

/// frontmatter 의 description 을 읽어야 한다.
#[test]
fn frontmatter_description_을_읽는다() {
    let source = "---\ndescription: 결제 정책 문서\n---\n";

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(doc.description, "결제 정책 문서");
}

/// frontmatter 에 description 이 없으면 error 여야 한다.
#[test]
fn description_이_없으면_에러다() {
    let source = "---\ntitle: 결제\n---\n";

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K102");
    assert_eq!(diagnostics[0].severity, Severity::Error);
    assert!(!diagnostics[0].locations.is_empty());
    assert!(!diagnostics[0].fixes.is_empty());
    // 진단은 어느 문서의 어디인지를 입력 경로 그대로 담아야 한다.
    assert_eq!(diagnostics[0].locations[0].doc, 문서경로());
    assert_eq!(diagnostics[0].fixes[0].doc, Some(문서경로()));
}

/// frontmatter 가 닫히지 않았으면 블록을 새로 만들라고 하면 안 된다.
/// 그대로 적용하면 열린 `---` 이 남아 문서가 더 망가진다.
#[test]
fn 닫히지_않은_frontmatter_는_닫는_구분자를_안내한다() {
    // 여는 `---` 이 3번째 줄이다. fix 를 그대로 적용해 새 블록이 앞에 붙으면
    // 에러가 사라지면서 사용자의 frontmatter 가 본문으로 강등된다 — 진단이 문제를 은폐한다.
    let source = "\n\n---\ndescription: 결제 정책 문서\n본문";

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K101");
    assert_eq!(diagnostics[0].locations[0].line, 3);
    assert!(diagnostics[0].fixes[0].action.contains("닫는"));
}

/// keyword 선언의 이름·한 줄 정의·상세 topic·줄 번호를 읽어야 한다.
#[test]
fn keyword_의_이름과_한줄정의를_읽는다() {
    let source = r#"---
description: 결제 정책 문서
---

keyword `결제`: 사용자가 상품 대금을 지불하는 행위 #`결제의 상세`
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(doc.keywords.len(), 1);
    assert_eq!(doc.keywords[0].name.0, vec!["결제".to_string()]);
    assert_eq!(
        doc.keywords[0].definition,
        "사용자가 상품 대금을 지불하는 행위"
    );
    assert_eq!(doc.keywords[0].detail, Some("결제의 상세".to_string()));
    assert_eq!(doc.keywords[0].line, 5);
}

/// 계층 keyword 는 이름 조각 배열로 읽어야 한다.
#[test]
fn 계층_키워드를_이름_배열로_읽는다() {
    let source = r#"---
description: 결제 정책 문서
---

keyword `결제수단`.`카드`: 카드를 사용한 결제
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(doc.keywords.len(), 1);
    assert_eq!(
        doc.keywords[0].name.0,
        vec!["결제수단".to_string(), "카드".to_string()]
    );
    assert_eq!(doc.keywords[0].definition, "카드를 사용한 결제");
    assert_eq!(doc.keywords[0].detail, None);
}

/// keyword 한 줄 정의 안의 백틱도 심볼 참조다 (스펙 4.2 "본문과 선언부의 모든 백틱").
/// 이름과 상세 topic 은 각각 name·detail 이 들고 있으므로 refs 에 넣지 않는다.
#[test]
fn keyword_정의_안의_백틱을_참조로_수집한다() {
    let source = r#"---
description: 결제 정책 문서
---

keyword `환불`: `결제` 를 되돌리는 행위 #`환불의 상세`
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(doc.keywords[0].refs, vec![("결제".to_string(), 5)]);
    assert_eq!(doc.keywords[0].name.0, vec!["환불".to_string()]);
    assert_eq!(doc.keywords[0].detail, Some("환불의 상세".to_string()));
    // iknow 는 Task 3 의 몫이다. 침범하면 여기서 깨진다.
    assert!(doc.keywords[0].iknow.is_empty());
}

/// 심볼 이름이 ` #` 로 끝나면 마커 뒤가 심볼로 파싱되지 않는다.
/// 그때 K104 를 내면 짝이 맞는 줄에 "닫히지 않았다" 고 거짓말하게 되므로,
/// 마커가 아니었던 것으로 보고 정의 전체를 그대로 읽는다.
#[test]
fn 샾으로_끝나는_심볼_이름을_그대로_읽는다() {
    let source = r#"---
description: 결제 정책 문서
---

keyword `a`: `b #`
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(doc.keywords[0].refs, vec![("b #".to_string(), 5)]);
    assert_eq!(doc.keywords[0].detail, None);
}

/// 심볼 이름 안의 `#` 는 상세 마커가 아니다. `C#`·`F#` 같은 이름이 현실적이다.
/// 마커는 공백 뒤에만 오므로 이름 끝의 `#` 와 그 닫는 백틱을 마커로 오인하면 안 된다.
#[test]
fn 샾이_든_심볼을_상세_마커로_오인하지_않는다() {
    let source = r#"---
description: 결제 정책 문서
---

keyword `언어`: `C#` 과 `F#` 을 뜻한다
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(
        doc.keywords[0].refs,
        vec![("C#".to_string(), 5), ("F#".to_string(), 5)]
    );
    assert_eq!(doc.keywords[0].detail, None);
    assert_eq!(doc.keywords[0].definition, "`C#` 과 `F#` 을 뜻한다");
}

/// 상세 마커는 줄 끝의 선택 항목이다. 뒤에 텍스트가 남으면 정의가 조용히 잘리므로 error 다.
#[test]
fn 줄_끝이_아닌_상세_마커는_에러다() {
    let source = r#"---
description: 결제 정책 문서
---

keyword `채널`: 슬랙 #`일반` 채널을 뜻한다
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K103");
    assert_eq!(diagnostics[0].locations[0].line, 5);
    assert!(!diagnostics[0].fixes.is_empty());
}

/// 줄 끝의 백틱이 이스케이프된 것이면 마커의 끝이 아니다.
/// 이 구멍이 열려 있으면 줄 끝에 `` \` `` 하나를 붙이는 것만으로 정의 손실 검사를 우회한다.
#[test]
fn 이스케이프된_백틱으로_끝나면_상세_마커가_아니다() {
    let source = r#"---
description: 결제 정책 문서
---

keyword `x`: 정의다 #`상세` 뒤 텍스트 \`
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K103");
    assert_eq!(diagnostics[0].locations[0].line, 5);
}

/// 마커가 없는 줄은 리터럴 백틱으로 끝나도 그대로 통과해야 한다.
#[test]
fn 리터럴_백틱으로_끝나는_정의는_통과한다() {
    let source = r#"---
description: 결제 정책 문서
---

keyword `x`: 리터럴 백틱 \` 을 쓴다
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(doc.keywords[0].definition, r"리터럴 백틱 \` 을 쓴다");
    assert_eq!(doc.keywords[0].detail, None);
}

/// `##` 헤딩마다 topic 을 만들고, 다음 헤딩 직전까지를 본문으로 잘라야 한다.
#[test]
fn topic_헤딩과_본문을_잘라낸다() {
    let source = r#"---
description: 결제 정책 문서
---

## 결제의 방법

사용자는 결제를 한다.

keyword `결제일`: 실제 대금이 처리되는 날짜

## 청구서

청구서는 결제로 생긴다.
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(doc.topics.len(), 2);
    assert_eq!(doc.topics[0].name, "결제의 방법");
    assert_eq!(doc.topics[0].line, 5);
    assert!(doc.topics[0].body.starts_with("## 결제의 방법"));
    assert!(doc.topics[0].body.contains("사용자는 결제를 한다."));
    assert!(!doc.topics[0].body.contains("## 청구서"));
    // topic 한가운데의 keyword 선언은 합법이지만 서술이 아니므로 본문에서 빠진다
    // (스펙 4.8). 섞여 들어가면 Task 8 의 rev 해시가 조용히 전부 바뀐다.
    assert_eq!(doc.keywords.len(), 1);
    assert!(!doc.topics[0].body.contains("keyword"));
    assert_eq!(doc.topics[1].name, "청구서");
    assert_eq!(doc.topics[1].line, 11);
    assert!(doc.topics[1].body.contains("청구서는 결제로 생긴다."));
    // Task 3 의 몫은 이 태스크가 건드리지 않는다. 침범하면 여기서 깨진다.
    assert!(doc.imports.is_empty());
    assert!(doc.topics[0].exceptions.is_empty());
    assert!(doc.topics[0].covers.is_empty());
    assert!(doc.topics[0].iknow.is_empty());
    assert!(!doc.topics[0].uncoded);
}

/// 진단은 첫 개에서 멈추지 않고 파일 전체에서 모아야 한다.
/// description 누락에서 조기 반환하면 뒤의 두 진단을 놓친다.
#[test]
fn 진단을_파일_전체에서_모은다() {
    // 백틱 스캔은 topic 안에서만 돌므로 K104 를 낼 줄보다 헤딩이 먼저 와야 한다.
    let source = r#"---
title: description 이 없다
---

keyword `결제`

## 결제의 방법

`짝없음
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    let codes: Vec<&str> = diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, vec!["K102", "K103", "K104"]);
}

/// 본문의 백틱 쌍은 심볼 참조로 등장 줄과 함께 수집해야 한다.
#[test]
fn 본문_백틱을_심볼_참조로_수집한다() {
    let source = r#"---
description: 결제 정책 문서
---

## 결제의 방법

사용자는 `결제수단` 으로 `결제` 를 한다.
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(
        doc.topics[0].refs,
        vec![("결제수단".to_string(), 7), ("결제".to_string(), 7)]
    );
}

/// 이스케이프된 백틱과 코드 펜스 내부는 심볼 참조가 아니다.
#[test]
fn 이스케이프된_백틱과_코드펜스_안은_참조가_아니다() {
    let source = r#"---
description: 결제 정책 문서
---

## 백틱 규칙

이 문장은 \`리터럴\` 백틱이다.

```text
`펜스안` 은 참조가 아니다
```

`진짜참조` 만 참조다.
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(doc.topics[0].refs, vec![("진짜참조".to_string(), 13)]);
    // 펜스 줄과 그 안의 내용은 참조가 아닐 뿐 본문에는 원문 그대로 남는다.
    assert!(doc.topics[0].body.contains("```text"));
    assert!(doc.topics[0].body.contains("`펜스안` 은 참조가 아니다"));
}

/// 코드 펜스가 닫히지 않으면 나머지 줄이 전부 삼켜져 진단이 조용히 꺼진다.
/// 인라인 백틱 불일치가 error 인데 블록 레벨의 같은 실패가 통과하면 방향이 반대다.
#[test]
fn 닫히지_않은_코드펜스는_에러다() {
    let source = r#"---
description: 결제 정책 문서
---

## 결제의 방법

```rust
fn 결제() {}
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K106");
    assert_eq!(diagnostics[0].severity, Severity::Error);
    assert_eq!(diagnostics[0].locations[0].line, 7);
    assert!(!diagnostics[0].fixes.is_empty());
}

/// 빈 백틱 쌍은 가리키는 심볼이 없다. 조용히 버리면 참조가 사라지므로 error 다.
#[test]
fn 빈_백틱_쌍은_에러다() {
    let source = r#"---
description: 결제 정책 문서
---

## 결제의 방법

마크다운 습관으로 ``결제`` 라고 쓰면 안 된다.
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K108");
    assert_eq!(diagnostics[0].severity, Severity::Error);
    assert_eq!(diagnostics[0].locations[0].line, 7);
    assert!(!diagnostics[0].fixes.is_empty());
}

/// 여는 펜스보다 짧은 백틱 런은 펜스를 닫지 못한다.
/// 4-백틱 블록 안에 3-백틱 줄을 담는 것은 마크다운을 설명하는 문서에서 흔하다.
#[test]
fn 네개짜리_펜스는_세개짜리로_닫히지_않는다() {
    let source = r#"---
description: 결제 정책 문서
---

## 마크다운 안내

````markdown
```rust
````
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(doc.topics.len(), 1);
    assert!(doc.topics[0].body.contains("```rust"));
}

/// `##` 뒤에 공백이 없어도 topic 헤딩이다. 아니면 이름 검사를 통째로 빠져나가
/// 선언 줄이 본문에 섞여 Task 8 의 rev 해시에 들어간다.
#[test]
fn 공백_없는_이중샾도_헤딩_이름_검사를_받는다() {
    let source = "---\ndescription: 결제 정책 문서\n---\n\n##\n\n본문이다.\n";

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K107");
    assert_eq!(diagnostics[0].locations[0].line, 5);
}

/// 이름이 빈 topic 은 CLI 로 주소를 댈 수 없다 — K105 가 막는 것과 같은 부류다.
#[test]
fn 이름이_빈_topic_헤딩은_에러다() {
    let source = "---\ndescription: 결제 정책 문서\n---\n\n## \n\n본문이다.\n";

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K107");
    assert_eq!(diagnostics[0].severity, Severity::Error);
    assert_eq!(diagnostics[0].locations[0].line, 5);
    assert!(!diagnostics[0].fixes.is_empty());
}

/// frontmatter 블록 자체가 없으면 error 여야 한다.
#[test]
fn frontmatter_블록_자체가_없으면_에러다() {
    let source = "# 그냥 마크다운\n\n본문이다.\n";

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K101");
    assert_eq!(diagnostics[0].severity, Severity::Error);
    assert!(!diagnostics[0].locations.is_empty());
    assert!(!diagnostics[0].fixes.is_empty());
}

/// keyword 선언에 한 줄 정의가 없으면 error 여야 한다.
#[test]
fn keyword_에_한줄정의가_없으면_에러다() {
    let source = r#"---
description: 결제 정책 문서
---

keyword `결제`
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K103");
    assert_eq!(diagnostics[0].severity, Severity::Error);
    assert_eq!(diagnostics[0].locations[0].line, 5);
    assert!(!diagnostics[0].fixes.is_empty());
}

/// 짝이 맞지 않는 백틱은 error 여야 한다.
#[test]
fn 짝이_맞지_않는_백틱은_에러다() {
    let source = r#"---
description: 결제 정책 문서
---

## 결제의 방법

`결제 를 한다.
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K104");
    assert_eq!(diagnostics[0].severity, Severity::Error);
    assert_eq!(diagnostics[0].locations[0].line, 7);
    assert!(!diagnostics[0].fixes.is_empty());
}

/// topic 헤딩에 백틱이 있으면 error 여야 한다.
/// 백틱이 든 헤딩은 CLI 인자로 주소를 댈 수 없다 (스펙 6.0).
/// 헤딩 줄은 짝이 맞든 아니든 K104 를 아예 받지 않으므로 이 판정만이 막는다.
#[test]
fn topic_헤딩에_백틱이_있으면_에러다() {
    let source = r#"---
description: 결제 정책 문서
---

## `결제` 의 방법
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K105");
    assert_eq!(diagnostics[0].severity, Severity::Error);
    assert_eq!(diagnostics[0].locations[0].line, 5);
    assert!(!diagnostics[0].fixes.is_empty());
}

/// 헤딩 줄은 짝 검사를 아예 받지 않으므로 헤딩 판정이 유일한 방어선이다.
/// 이 판정이 없으면 짝 없는 백틱이 아무 진단도 없이 통과한다 — 폴백은 존재하지 않는다.
#[test]
fn 헤딩의_짝없는_백틱은_헤딩_판정만이_잡는다() {
    let source = r#"---
description: 결제 정책 문서
---

## `결제 의 방법
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K105");
    assert_eq!(diagnostics[0].severity, Severity::Error);
    assert_eq!(diagnostics[0].locations[0].line, 5);
    assert!(!diagnostics[0].fixes.is_empty());
}

/// 첫 `.`·`#`·`!` 구분자가 문서 경로와 심볼 이름을 가르고 종류를 정한다.
/// 그것이 `.` 이면 keyword import 다.
#[test]
fn keyword_import_를_읽는다() {
    let source = r#"---
description: 결제 정책 문서
---

import `docs`/`A`.`결제`
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(doc.imports.len(), 1);
    assert_eq!(
        doc.imports[0].target.doc,
        DocPath(vec!["docs".to_string(), "A".to_string()])
    );
    assert_eq!(doc.imports[0].target.kind, SymbolKind::Keyword);
    assert_eq!(doc.imports[0].target.name, vec!["결제".to_string()]);
    assert_eq!(doc.imports[0].alias, None);
    assert_eq!(doc.imports[0].rev, None);
    assert_eq!(doc.imports[0].line, 5);
}

/// 첫 구분자가 `#` 이면 topic import 다.
#[test]
fn topic_import_를_읽는다() {
    let source = r#"---
description: 결제 정책 문서
---

import `docs`/`A`#`상품의 정보`
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(doc.imports.len(), 1);
    assert_eq!(
        doc.imports[0].target.doc,
        DocPath(vec!["docs".to_string(), "A".to_string()])
    );
    assert_eq!(doc.imports[0].target.kind, SymbolKind::Topic);
    assert_eq!(doc.imports[0].target.name, vec!["상품의 정보".to_string()]);
}

/// 첫 구분자가 `!` 이면 exception import 다. 경로 조각은 여러 개일 수 있다.
#[test]
fn exception_import_를_읽는다() {
    let source = r#"---
description: 결제 정책 문서
---

import `docs`/`details`/`payment`!`무료 상품에 대한 청구서`
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(doc.imports.len(), 1);
    assert_eq!(
        doc.imports[0].target.doc,
        DocPath(vec![
            "docs".to_string(),
            "details".to_string(),
            "payment".to_string()
        ])
    );
    assert_eq!(doc.imports[0].target.kind, SymbolKind::Exception);
    assert_eq!(
        doc.imports[0].target.name,
        vec!["무료 상품에 대한 청구서".to_string()]
    );
}

/// 첫 `.` 이후는 전부 키워드 이름 조각이다.
#[test]
fn 계층_키워드_import_를_읽는다() {
    let source = r#"---
description: 결제 정책 문서
---

import `docs`/`A`.`결제수단`.`카드`
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(
        doc.imports[0].target.doc,
        DocPath(vec!["docs".to_string(), "A".to_string()])
    );
    assert_eq!(doc.imports[0].target.kind, SymbolKind::Keyword);
    assert_eq!(
        doc.imports[0].target.name,
        vec!["결제수단".to_string(), "카드".to_string()]
    );
}

/// `as` 는 이 문서 안에서만 통하는 다른 이름을 준다.
#[test]
fn as_alias_를_읽는다() {
    let source = r#"---
description: 결제 정책 문서
---

import `docs`/`A`.`결제` as `A 결제`
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(doc.imports[0].alias, Some("A 결제".to_string()));
    assert_eq!(doc.imports[0].rev, None);
    assert_eq!(doc.imports[0].target.name, vec!["결제".to_string()]);
}

/// `rev` 는 참조 시점 내용의 해시 핀이다. 큰따옴표 안의 값만 담는다.
#[test]
fn rev_핀을_읽는다() {
    let source = r#"---
description: 결제 정책 문서
---

import `docs`/`A`.`결제` as `A 결제` rev "a3f9c1"
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(doc.imports[0].alias, Some("A 결제".to_string()));
    assert_eq!(doc.imports[0].rev, Some("a3f9c1".to_string()));
}

/// `as` 와 `rev` 는 각각 선택이다. alias 없이 핀만 붙일 수 있어야 한다.
#[test]
fn as_없이_rev_만_있는_import_를_읽는다() {
    let source = r#"---
description: 결제 정책 문서
---

import `docs`/`A`#`상품의 정보` rev "7b21e0"
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(doc.imports[0].alias, None);
    assert_eq!(doc.imports[0].rev, Some("7b21e0".to_string()));
    assert_eq!(doc.imports[0].target.kind, SymbolKind::Topic);
}

/// exception 도 rev 핀을 가질 수 있다 (스펙 4.7 "세 종류 모두").
#[test]
fn exception_import_도_rev_핀을_읽는다() {
    let source = r#"---
description: 결제 정책 문서
---

import `docs`/`A`!`해외 결제` as `A 해외 결제` rev "e91b04"
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(doc.imports[0].target.kind, SymbolKind::Exception);
    assert_eq!(doc.imports[0].alias, Some("A 해외 결제".to_string()));
    assert_eq!(doc.imports[0].rev, Some("e91b04".to_string()));
}

/// `exception` 선언과 `pending` 표시를 topic 본문 안에서 읽어야 한다.
#[test]
fn exception_과_pending_을_읽는다() {
    let source = r#"---
description: 결제 정책 문서
---

## 결제 청구서와 결제의 관계

모든 `청구서` 는 `결제` 에 의해 생겨난다.

exception `무료 상품에 대한 청구서`
exception `해외 결제` pending
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(doc.topics[0].exceptions.len(), 2);
    assert_eq!(doc.topics[0].exceptions[0].name, "무료 상품에 대한 청구서");
    assert!(!doc.topics[0].exceptions[0].pending);
    assert_eq!(doc.topics[0].exceptions[0].line, 9);
    assert_eq!(doc.topics[0].exceptions[1].name, "해외 결제");
    assert!(doc.topics[0].exceptions[1].pending);
    assert_eq!(doc.topics[0].exceptions[1].line, 10);
}

/// `cover` 는 다른 문서의 예외를 다루는 정책임을 이름과 줄로 기록한다.
#[test]
fn cover_를_읽는다() {
    let source = r#"---
description: 결제 정책 문서
---

## 무료상품 결제일 때 청구서

무료상품은 0원 기록만 남긴다.

cover `무료상품 청구서 예외`
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(
        doc.topics[0].covers,
        vec![("무료상품 청구서 예외".to_string(), 9)]
    );
}

/// exception·cover 줄은 서술이 아니라 선언이므로 본문에서 빠진다 (스펙 4.8).
/// 포함하면 예외를 하나 추가하는 것만으로 무관한 커버 문서가 전부 깨진다.
#[test]
fn exception_과_cover_줄은_body_에서_빠진다() {
    let source = r#"---
description: 결제 정책 문서
---

## 결제 청구서

모든 `청구서` 는 `결제` 로 생긴다.

exception `무료 상품에 대한 청구서`
cover `해외 결제 예외`
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(
        doc.topics[0].body,
        "## 결제 청구서\n\n모든 `청구서` 는 `결제` 로 생긴다.\n"
    );
    // 선언의 이름은 exceptions·covers 가 들고 있으므로 refs 에 중복해 넣지 않는다.
    assert_eq!(
        doc.topics[0].refs,
        vec![("청구서".to_string(), 7), ("결제".to_string(), 7)]
    );
}

/// exception 선언에도 `// iknow` 가 붙는다 (스펙 4.4 "세 자리").
#[test]
fn exception_선언에도_iknow_가_붙는다() {
    let source = r#"---
description: 결제 정책 문서
---

## 결제 청구서

exception `해외 결제` // iknow `docs`/`B`!`해외 결제`
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(doc.topics[0].exceptions[0].name, "해외 결제");
    assert!(!doc.topics[0].exceptions[0].pending);
    assert_eq!(doc.topics[0].exceptions[0].iknow.len(), 1);
    assert_eq!(
        doc.topics[0].exceptions[0].iknow[0].doc,
        DocPath(vec!["docs".to_string(), "B".to_string()])
    );
    assert_eq!(
        doc.topics[0].exceptions[0].iknow[0].kind,
        SymbolKind::Exception
    );
    assert_eq!(
        doc.topics[0].exceptions[0].iknow[0].name,
        vec!["해외 결제".to_string()]
    );
}

/// exception 은 선언된 topic 의 맥락에서 의미가 나온다 (스펙 4.8).
/// topic 밖의 선언은 맥락도 해시도 없으므로 error 다.
#[test]
fn topic_밖의_exception_은_에러다() {
    let source = r#"---
description: 결제 정책 문서
---

exception `해외 결제`
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K111");
    assert_eq!(diagnostics[0].severity, Severity::Error);
    assert_eq!(diagnostics[0].locations[0].line, 5);
    assert!(!diagnostics[0].fixes.is_empty());
}

/// `// iknow` 는 쉼표로 여러 대상을 나열한다.
#[test]
fn iknow_대상_목록을_쉼표로_읽는다() {
    let source = r#"---
description: 결제 정책 문서
---

keyword `금액`: 청구되는 원화 액수 // iknow `docs`/`B`.`금액`, `docs`/`C`.`금액`
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(doc.keywords[0].iknow.len(), 2);
    assert_eq!(
        doc.keywords[0].iknow[0].doc,
        DocPath(vec!["docs".to_string(), "B".to_string()])
    );
    assert_eq!(doc.keywords[0].iknow[0].kind, SymbolKind::Keyword);
    assert_eq!(doc.keywords[0].iknow[0].name, vec!["금액".to_string()]);
    assert_eq!(
        doc.keywords[0].iknow[1].doc,
        DocPath(vec!["docs".to_string(), "C".to_string()])
    );
    assert_eq!(doc.keywords[0].iknow[1].name, vec!["금액".to_string()]);
}

/// `// iknow` 는 keyword 의 한 줄 정의에도 참조에도 섞이면 안 된다.
/// 한 줄 정의는 rev 해시의 입력이므로(스펙 4.8) 섞이면 핀이 조용히 어긋난다.
#[test]
fn iknow_는_keyword_한줄정의와_참조에서_빠진다() {
    let source = r#"---
description: 결제 정책 문서
---

keyword `금액`: 청구되는 원화 액수 // iknow `docs`/`B`.`금액`
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(doc.keywords[0].definition, "청구되는 원화 액수");
    assert!(doc.keywords[0].refs.is_empty());
    assert_eq!(doc.keywords[0].iknow.len(), 1);
}

/// 상세 topic 마커는 modifier 를 뗀 뒤의 줄 끝을 본다.
/// modifier 를 남긴 채 마커를 찾으면 뒤에 텍스트가 남았다며 합법 선언을 K103 으로 거부한다.
#[test]
fn 상세_마커와_iknow_가_한_줄에_같이_온다() {
    let source = r#"---
description: 결제 정책 문서
---

keyword `금액`: 청구되는 원화 액수 #`금액의 상세` // iknow `docs`/`B`.`금액`
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(doc.keywords[0].definition, "청구되는 원화 액수");
    assert_eq!(doc.keywords[0].detail, Some("금액의 상세".to_string()));
    assert_eq!(doc.keywords[0].iknow.len(), 1);
}

/// `iknow` 는 부인이지 참조가 아니다 — 그래프 간선을 만들지 않는다 (스펙 4.4).
#[test]
fn iknow_대상은_imports_에_들어가지_않는다() {
    let source = r#"---
description: 결제 정책 문서
---

keyword `금액`: 청구되는 원화 액수 // iknow `docs`/`B`.`금액`

## 결제의 방법 // iknow `docs`/`B`#`결제의 방법`
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert!(doc.imports.is_empty());
    assert_eq!(doc.keywords[0].iknow.len(), 1);
    assert_eq!(doc.topics[0].iknow.len(), 1);
}

/// topic 헤딩의 `// iknow` 대상은 백틱을 쓴다 (스펙 4.4).
/// modifier 를 먼저 잘라내지 않으면 이 합법 문서가 K105 로 거부된다.
#[test]
fn iknow_가_붙은_topic_헤딩은_합법이며_이름에서_빠진다() {
    let source = r#"---
description: 결제 정책 문서
---

## 결제의 방법 // iknow `docs`/`B`#`결제의 방법`

사용자는 결제를 한다.
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(doc.topics[0].name, "결제의 방법");
    assert_eq!(doc.topics[0].iknow.len(), 1);
    assert_eq!(
        doc.topics[0].iknow[0].doc,
        DocPath(vec!["docs".to_string(), "B".to_string()])
    );
    assert_eq!(doc.topics[0].iknow[0].kind, SymbolKind::Topic);
    assert_eq!(doc.topics[0].iknow[0].name, vec!["결제의 방법".to_string()]);
}

/// `// uncoded` 는 이름에서도 본문에서도 빠진다. 본문은 헤딩 줄을 남긴다.
#[test]
fn uncoded_modifier_는_body_에서_제외된다() {
    let source = r#"---
description: 결제 정책 문서
---

## 조직의 문서 검토 절차 // uncoded

본문이다.
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert!(doc.topics[0].uncoded);
    assert_eq!(doc.topics[0].name, "조직의 문서 검토 절차");
    assert_eq!(doc.topics[0].body, "## 조직의 문서 검토 절차\n\n본문이다.");
}

/// 구분자가 `/` 뿐이면 어떤 심볼을 가리키는지 알 수 없다.
#[test]
fn 구분자가_없는_import_는_에러다() {
    let source = r#"---
description: 결제 정책 문서
---

import `docs`/`A`
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K109");
    assert_eq!(diagnostics[0].severity, Severity::Error);
    assert_eq!(diagnostics[0].locations[0].line, 5);
    assert!(!diagnostics[0].fixes.is_empty());
}

/// import 대상 뒤에 정체 모를 텍스트가 남으면 조용히 버리지 않고 error 다.
#[test]
fn import_대상_뒤의_잉여_텍스트는_에러다() {
    let source = r#"---
description: 결제 정책 문서
---

import `docs`/`A`.`결제` 쓰레기
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K109");
    assert_eq!(diagnostics[0].locations[0].line, 5);
}

/// import 줄의 심볼 주소는 백틱으로 쓰므로 짝 검사를 반드시 받아야 한다.
#[test]
fn import_줄의_짝없는_백틱은_에러다() {
    let source = r#"---
description: 결제 정책 문서
---

import `docs`/`A`.`결제
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K104");
    assert_eq!(diagnostics[0].locations[0].line, 5);
}

/// import 경로의 빈 백틱 쌍은 가리키는 문서가 없다.
#[test]
fn import_대상의_빈_백틱은_에러다() {
    let source = r#"---
description: 결제 정책 문서
---

import `docs`/``.`결제`
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K108");
    assert_eq!(diagnostics[0].locations[0].line, 5);
}

/// topic 밖의 짝없는 백틱은 내용 규칙(K112)이 먼저 잡는다.
/// 그 줄은 애초에 존재할 수 없으므로 짝 검사를 덧붙이면 진단이 둘이 된다.
#[test]
fn topic_밖의_짝없는_백틱은_내용_규칙이_잡는다() {
    let source = r#"---
description: 결제 정책 문서
---

`짝없음

## 결제의 방법
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K112");
    assert_eq!(diagnostics[0].locations[0].line, 5);
}

/// 문서의 식별자는 파일 경로 하나뿐이다 (스펙 3절). `#` 제목 줄은 topic 밖 내용이다.
#[test]
fn 문서_제목이_topic_밖에_있으면_에러다() {
    let source = r#"---
description: 결제 정책 문서
---

# 결제 정책

## 결제의 방법
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K112");
    assert_eq!(diagnostics[0].severity, Severity::Error);
    assert_eq!(diagnostics[0].locations[0].line, 5);
    assert!(!diagnostics[0].fixes.is_empty());
}

/// 서문 단락은 어떤 rev 해시에도 들어가지 않고 kang show 로도 볼 수 없다 (스펙 3절).
#[test]
fn 서문_단락은_에러다() {
    let source = r#"---
description: 결제 정책 문서
---

이 문서는 결제 정책을 설명한다.

## 결제의 방법
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K112");
    assert_eq!(diagnostics[0].locations[0].line, 5);
}

/// 첫 `##` 앞의 `###` 이하 헤딩도 topic 을 열지 못하므로 topic 밖 내용이다.
#[test]
fn 첫_topic_앞의_삼중샾_헤딩은_에러다() {
    let source = r#"---
description: 결제 정책 문서
---

### 배경

## 결제의 방법
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K112");
    assert_eq!(diagnostics[0].locations[0].line, 5);
}

/// import 와 keyword 선언만 있는 파일은 통과해야 한다.
/// 새 진단이 합법 입력을 거부하는 것이 가장 위험한 회귀다.
#[test]
fn import_와_keyword_만_있는_파일은_통과한다() {
    let source = r#"---
description: 결제 정책 문서
---

import `docs`/`A`.`결제` as `A 결제` rev "a3f9c1"
import `docs`/`A`#`상품의 정보`

keyword `결제일`: 실제 대금이 처리되는 날짜
keyword `금액`: 청구되는 원화 액수 // iknow `docs`/`B`.`금액`
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(doc.imports.len(), 2);
    assert_eq!(doc.keywords.len(), 2);
    assert!(doc.topics.is_empty());
}

/// 빈 줄과 공백만 있는 줄은 topic 밖에서도 내용이 아니다.
#[test]
fn 빈_줄과_공백만_있는_줄은_통과한다() {
    let source = "---\ndescription: 결제 정책 문서\n---\n\n   \n\t\n\nkeyword `결제`: 대금을 지불하는 행위\n\n## 결제의 방법\n";

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(doc.keywords.len(), 1);
    assert_eq!(doc.topics.len(), 1);
}

/// topic **안**의 `###` 이하는 본문 마크다운이다. 내용 규칙이 여기까지 번지면 안 된다.
#[test]
fn topic_안의_소제목은_통과한다() {
    let source = r#"---
description: 결제 정책 문서
---

## 결제의 방법

### 카드 결제

카드로 결제한다.
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(doc.topics.len(), 1);
    assert!(doc.topics[0].body.contains("### 카드 결제"));
    assert!(doc.topics[0].body.contains("카드로 결제한다."));
}

/// 연속된 topic 밖 내용은 한 덩어리로 본다. 서문 열 줄에 진단 열 개는 소음이다.
#[test]
fn 연속된_topic_밖_줄은_진단_하나로_모인다() {
    let source = r#"---
description: 결제 정책 문서
---

# 결제 정책
이 문서는 결제 정책을 설명한다.
읽기 전에 배경을 알아 두자.

## 결제의 방법
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K112");
    // 덩어리의 첫 줄을 가리킨다.
    assert_eq!(diagnostics[0].locations[0].line, 5);
}

/// 빈 줄로 나뉘면 서로 다른 덩어리다. 하나로 묶으면 뒤쪽 내용의 위치를 잃는다.
#[test]
fn 빈_줄로_나뉜_topic_밖_내용은_각각_걸린다() {
    let source = r#"---
description: 결제 정책 문서
---

# 결제 정책

이 문서는 결제 정책을 설명한다.

## 결제의 방법
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    let 줄: Vec<usize> = diagnostics.iter().map(|d| d.locations[0].line).collect();
    let codes: Vec<&str> = diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, vec!["K112", "K112"]);
    assert_eq!(줄, vec![5, 7]);
}

/// topic 밖의 코드펜스도 내용이다. 블록 전체가 진단 하나로 묶인다.
#[test]
fn topic_밖의_코드펜스는_에러다() {
    let source = r#"---
description: 결제 정책 문서
---

```rust
fn 결제() {}
```

## 결제의 방법
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K112");
    // 펜스를 연 줄을 가리킨다. 안쪽 줄은 그 블록에 딸린 것으로 본다.
    assert_eq!(diagnostics[0].locations[0].line, 5);
}

/// topic 밖의 펜스가 닫히지도 않으면 두 사실이 모두 참이다.
/// 순서는 결정적이어야 한다 — 여는 줄의 K112 가 먼저, 파일 끝의 K106 이 뒤다.
#[test]
fn topic_밖_코드펜스의_진단_우선순위가_결정적이다() {
    let source = r#"---
description: 결제 정책 문서
---

```rust
fn 결제() {}
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    let codes: Vec<&str> = diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, vec!["K112", "K106"]);
    assert_eq!(diagnostics[0].locations[0].line, 5);
    assert_eq!(diagnostics[1].locations[0].line, 5);
}

/// topic 밖의 cover 는 K111 이 더 구체적으로 잡는다. K112 를 겹쳐 내면 한 줄에 진단이 둘이다.
#[test]
fn topic_밖의_cover_는_진단이_하나다() {
    let source = r#"---
description: 결제 정책 문서
---

cover `무료상품 청구서 예외`
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K111");
    assert_eq!(diagnostics[0].locations[0].line, 5);
}

/// kang 에는 주석 문법이 없으므로 알 수 없는 modifier 는 조용히 버리지 않는다.
#[test]
fn 알_수_없는_modifier_는_에러다() {
    let source = r#"---
description: 결제 정책 문서
---

## 결제의 방법 // 메모
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K110");
    assert_eq!(diagnostics[0].severity, Severity::Error);
    assert_eq!(diagnostics[0].locations[0].line, 5);
    assert!(!diagnostics[0].fixes.is_empty());
}

/// 대상이 하나도 없는 `// iknow` 는 아무것도 인지하지 않는다.
#[test]
fn iknow_대상이_없으면_에러다() {
    let source = r#"---
description: 결제 정책 문서
---

## 결제의 방법 // iknow
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K110");
    assert_eq!(diagnostics[0].locations[0].line, 5);
}

/// `uncoded` 는 topic 헤딩 전용이다. keyword 에 붙으면 조용히 무시하지 않는다.
#[test]
fn keyword_에_붙은_uncoded_는_에러다() {
    let source = r#"---
description: 결제 정책 문서
---

keyword `결제`: 대금을 지불하는 행위 // uncoded
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K110");
    assert_eq!(diagnostics[0].locations[0].line, 5);
}

/// modifier 만 있고 이름이 없는 헤딩은 여전히 주소를 댈 수 없다.
#[test]
fn modifier_만_있고_이름이_없는_헤딩은_에러다() {
    let source = r#"---
description: 결제 정책 문서
---

## // uncoded
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K107");
    assert_eq!(diagnostics[0].locations[0].line, 5);
}

/// modifier 는 공백 뒤의 `//` 다. `https://` 같은 본문 표기를 가로채면
/// 합법 문서가 "알 수 없는 modifier" 로 거부된다.
#[test]
fn 본문의_두_슬래시는_modifier_가_아니다() {
    let source = r#"---
description: 결제 정책 문서
---

keyword `주소`: https://example.com 형식의 문자열
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(
        doc.keywords[0].definition,
        "https://example.com 형식의 문자열"
    );
    assert!(doc.keywords[0].iknow.is_empty());
}

/// import 를 선언으로 읽는 것은 첫 topic 이전뿐이다 (스펙 4.7 "파일 최상단").
/// 그 뒤의 같은 낱말은 서술 본문이며, 이 저장소의 스펙 문서 자신이 그렇게 쓴다.
#[test]
fn 첫_topic_뒤의_import_줄은_본문이다() {
    let source = r#"---
description: 결제 정책 문서
---

## 깊이 폭발

import `폐포` 가 깊으면 `결제` 를 다시 본다.
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert!(doc.imports.is_empty());
    assert!(doc.topics[0].body.contains("import `폐포` 가 깊으면"));
    assert_eq!(
        doc.topics[0].refs,
        vec![("폐포".to_string(), 7), ("결제".to_string(), 7)]
    );
}

/// 문서 경로는 `/` 로 이은 전체 경로 하나로만 출력된다.
#[test]
fn 문서_경로는_슬래시로_출력된다() {
    let path = DocPath(vec![
        "docs".to_string(),
        "details".to_string(),
        "payment".to_string(),
    ]);

    assert_eq!(path.to_string(), "docs/details/payment");
}
