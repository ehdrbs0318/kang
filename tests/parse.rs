// `parse::parse_document` 의 frontmatter / keyword / topic 파싱을 검증하는 통합 테스트.
use kang::ast::{DocPath, Severity};
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

/// `##` 헤딩마다 topic 을 만들고, 다음 헤딩 직전까지를 본문으로 잘라야 한다.
#[test]
fn topic_헤딩과_본문을_잘라낸다() {
    let source = r#"---
description: 결제 정책 문서
---

## 결제의 방법

사용자는 결제를 한다.

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
    assert_eq!(doc.topics[1].name, "청구서");
    assert_eq!(doc.topics[1].line, 9);
    assert!(doc.topics[1].body.contains("청구서는 결제로 생긴다."));
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
