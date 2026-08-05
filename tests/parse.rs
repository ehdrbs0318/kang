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

    assert_eq!(diagnostics[0].code, "K108");
    assert_eq!(diagnostics[0].severity, Severity::Error);
    assert_eq!(diagnostics[0].locations[0].line, 7);
    assert!(!diagnostics[0].fixes.is_empty());
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
/// 짝이 맞는 백틱은 K104 로 잡히지 않으므로 K105 만이 막는다.
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
