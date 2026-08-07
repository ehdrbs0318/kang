// `parse::parse_document` 의 frontmatter / keyword / topic / import / exception /
// cover / modifier 파싱을 검증하는 통합 테스트.
use kang::ast::{DocPath, FixKind, Severity, SymbolKind};
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

/// keyword 이름의 조각 사이에 낀 텍스트는 조용히 사라지면 안 된다.
///
/// `scan_symbols` 는 백틱 쌍 사이의 텍스트를 그냥 버린다. 정본 재조립 대조가 없으면
/// `` keyword `결제` blah `수단` `` 이 **exit 0 으로** `결제.수단` 이 되고 `blah` 는
/// 어디에도 남지 않는다 — 사용자는 자기가 쓴 것이 무엇이 되었는지 모른다. 조용히 다른
/// 뜻으로 읽는 것은 거부보다 나쁘다.
#[test]
fn keyword_이름_조각_사이의_텍스트는_에러다() {
    let source = r#"---
description: 결제 정책 문서
---

keyword `결제`: 상위 정의다
keyword `결제` blah `수단`: 하위 정의다
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K103");
    assert_eq!(diagnostics[0].locations[0].line, 6);
    // 무엇으로 읽혔는지 보여 줘야 사용자가 자기 의도와 대조할 수 있다.
    assert!(
        diagnostics[0].message.contains("blah"),
        "원문을 인용해야 한다: {}",
        diagnostics[0].message
    );
    assert!(
        diagnostics[0].message.contains("`결제`.`수단`"),
        "무엇으로 읽혔는지 말해야 한다: {}",
        diagnostics[0].message
    );
    assert!(!diagnostics[0].fixes.is_empty());
}

/// 정당한 계층 선언은 그대로 통과해야 한다. 위 검사가 넓게 잡으면 이 단언이 깨진다.
#[test]
fn 백틱을_점으로_이은_계층_이름은_통과한다() {
    let source = r#"---
description: 결제 정책 문서
---

keyword `결제`: 상위 정의다
keyword `결제`.`수단`: 하위 정의다
"#;

    let document = parse::parse_document(문서경로(), source).expect("정당한 계층 선언이다");

    assert_eq!(document.keywords.len(), 2);
    assert_eq!(document.keywords[1].name.0, vec!["결제", "수단"]);
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

/// topic 헤딩 이름의 `/` 는 error 여야 한다 (스펙 6.0 `:415`).
/// CLI 주소는 마지막 `/` 뒤에서 갈리므로 이 이름은 `kang bless` 로 가리킬 수 없고,
/// 이 topic 을 import 한 문서는 핀을 붙일 방법이 없어 빌드가 영구히 error 에 머문다.
#[test]
fn topic_헤딩_이름의_슬래시는_에러다() {
    let source = "---\ndescription: 환불 정책 문서\n---\n\n## 환불/취소\n\n본문이다.\n";

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K115");
    assert_eq!(diagnostics[0].severity, Severity::Error);
    assert_eq!(diagnostics[0].locations[0].line, 5);
    // 고칠 것은 이름이고 고칠 사람은 그 뜻을 아는 사람이다 — 셸 명령이 아니라 편집이다.
    assert_eq!(diagnostics[0].fixes[0].kind, FixKind::Edit);
    assert_eq!(diagnostics[0].fixes[0].doc, Some(문서경로()));
}

/// keyword 이름 조각의 `/` 도 같은 이유로 error 여야 한다.
/// 스펙 `:415` 는 "심볼 이름" 이라 쓰므로 세 종류 전부에 걸린다.
#[test]
fn keyword_이름_조각의_슬래시는_에러다() {
    let source = "---\ndescription: 결제 정책 문서\n---\n\nkeyword `카드/현금`: 결제 수단 두 가지다.\n\n## 결제 수단\n\n본문이다.\n";

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K115");
    assert_eq!(diagnostics[0].severity, Severity::Error);
    assert_eq!(diagnostics[0].locations[0].line, 5);
    assert_eq!(diagnostics[0].fixes[0].kind, FixKind::Edit);
}

/// exception 이름의 `/` 도 error 여야 한다.
#[test]
fn exception_이름의_슬래시는_에러다() {
    let source =
        "---\ndescription: 결제 정책 문서\n---\n\n## 결제 수단\n\nexception `무료/할인 상품`\n";

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K115");
    assert_eq!(diagnostics[0].severity, Severity::Error);
    assert_eq!(diagnostics[0].locations[0].line, 7);
}

/// 본문 참조의 `/` 도 같은 진단을 받아야 한다.
///
/// **선언만 막으면 `K001` 의 fix 가 새 error 를 만든다** — 어느 문서도 이 이름을 선언할 수
/// 없게 되므로 `K001` 은 언제나 "이 문서에서 keyword 로 선언하세요" 를 처방하고, 그대로
/// 적용하면 선언 자리의 진단이 난다. 스펙 5.1.1 이 요구하는 "그대로 적용 가능한 fix" 가
/// 아니게 된다. 참조를 읽는 자리에서 같은 근거로 거절하면 그 사슬이 아예 생기지 않는다.
#[test]
fn 본문_참조의_슬래시도_같은_진단을_받는다() {
    let source =
        "---\ndescription: 결제 정책 문서\n---\n\n## 결제 수단\n\n`카드/현금` 을 지원한다.\n";

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K115");
    assert_eq!(diagnostics[0].locations[0].line, 7);
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
    // 위치 위반이므로 옮기라고 하는 것이 맞다.
    assert!(diagnostics[0].fixes[0].action.contains("옮기"));
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
    // 걸린 줄을 하나도 빠짐없이 가리킨다. 첫 줄만 가리키면 끝이 어디인지 알 수 없다.
    let 줄: Vec<usize> = diagnostics[0].locations.iter().map(|l| l.line).collect();
    assert_eq!(줄, vec![5, 6, 7]);
}

/// 빈 줄이 끼어도 같은 진단에 모인다. 빈 줄 유무로 진단 개수가 달라지면
/// 사용자가 보기에 같은 서문인데 개수가 다르다.
#[test]
fn 빈_줄로_나뉜_topic_밖_줄도_같은_진단에_모인다() {
    let source = r#"---
description: 결제 정책 문서
---

# 결제 정책

이 문서는 결제 정책을 설명한다.

## 결제의 방법
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K112");
    let 줄: Vec<usize> = diagnostics[0].locations.iter().map(|l| l.line).collect();
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

/// modifier 로 인정하는 낱말은 `uncoded` 와 `iknow` 뿐이다.
/// 그 밖의 낱말은 modifier 가 아니라 그 줄의 일부다.
///
/// **헤딩에서는 그 줄의 일부가 되는 것이 곧 error 다** — 이름에 `/` 가 들어가므로 CLI 주소로
/// 가리킬 수 없다 (스펙 6.0 `:415`). 여기서 보는 것은 낱말 판정이 그대로 살아 있다는 사실이며,
/// 진단이 이름을 `// 메모` 까지 담아 보고하는 것이 그 증거다 — modifier 로 잘렸다면 이름은
/// `결제의 방법` 이고 `/` 가 없어 진단이 아예 나지 않는다.
/// `//` 를 쓰는 합법 문장은 이름이 아닌 자리(한 줄 정의·본문)에 남아 있다 —
/// [`정의_안의_두_슬래시_주소는_modifier_가_아니다`] 와 [`본문의_두_슬래시는_modifier_가_아니다`].
#[test]
fn 알_수_없는_낱말은_modifier_가_아니라_이름의_일부다() {
    let source = r#"---
description: 결제 정책 문서
---

## 결제의 방법 // 메모
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K115");
    assert!(
        diagnostics[0].message.contains("결제의 방법 // 메모"),
        "{}",
        diagnostics[0].message
    );
}

/// 알려진 낱말로 시작하면 modifier 자리이므로, 그 뒤가 틀리면 조용히 버리지 않는다.
#[test]
fn uncoded_뒤에_텍스트가_남으면_에러다() {
    let source = r#"---
description: 결제 정책 문서
---

## 결제의 방법 // uncoded 뭐
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K110");
    assert_eq!(diagnostics[0].severity, Severity::Error);
    assert_eq!(diagnostics[0].locations[0].line, 5);
    assert!(!diagnostics[0].fixes.is_empty());
}

/// `//` 로 시작하는 주소는 modifier 가 아니다.
/// kang 에 `//` 이스케이프가 없으므로 이것이 막히면 이 정의를 쓸 방법이 아예 없다.
#[test]
fn 정의_안의_두_슬래시_주소는_modifier_가_아니다() {
    let source = r#"---
description: 결제 정책 문서
---

keyword `CDN`: 기본 주소는 //cdn.example.com 이다
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(
        doc.keywords[0].definition,
        "기본 주소는 //cdn.example.com 이다"
    );
    assert!(doc.keywords[0].iknow.is_empty());
}

/// 공백 뒤의 `//` 라도 알려진 modifier 낱말이 아니면 본문이다.
#[test]
fn 공백_뒤_두_슬래시라도_알_수_없는_낱말이면_본문이다() {
    let source = r#"---
description: 결제 정책 문서
---

keyword `비율`: 50 // 100 으로 계산
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(doc.keywords[0].definition, "50 // 100 으로 계산");
    assert!(doc.keywords[0].iknow.is_empty());
}

/// 앞에 `//` 가 든 주소가 있어도 뒤의 진짜 modifier 를 찾아야 한다.
/// 첫 후보에서 멈추면 이 선언의 `iknow` 를 놓친다.
///
/// **헤딩이 아니라 한 줄 정의로 잰다.** modifier 를 가르는 것은 헤딩과 keyword 가 공유하는
/// `split_modifier` 하나이고, 헤딩 이름에는 `/` 를 쓸 수 없어(스펙 6.0 `:415`) 그 자리에서는
/// 앞선 `//` 후보를 만들 방법이 아예 없다. 정의와 본문에는 남아 있다.
#[test]
fn 주소_뒤에_붙은_modifier_는_여전히_인식된다() {
    let source = r#"---
description: 결제 정책 문서
---

keyword `금액`: 참고 http://a.com 문서 // iknow `docs`/`B`.`금액`
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(doc.keywords[0].definition, "참고 http://a.com 문서");
    assert_eq!(doc.keywords[0].iknow.len(), 1);
    assert_eq!(doc.keywords[0].iknow[0].name, vec!["금액".to_string()]);
}

/// 낱말 판정을 넣어도 정상 modifier 는 그대로 인식되어야 한다.
#[test]
fn 정의_뒤에_붙은_iknow_는_여전히_인식된다() {
    let source = r#"---
description: 결제 정책 문서
---

keyword `금액`: 청구 액수 // iknow `docs`/`B`.`금액`
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert_eq!(doc.keywords[0].definition, "청구 액수");
    assert_eq!(doc.keywords[0].iknow.len(), 1);
    assert_eq!(doc.keywords[0].iknow[0].name, vec!["금액".to_string()]);
}

/// 쉼표만 남은 자리는 원문에 없는 빈 주소를 인용하지 말고 쉼표를 지적해야 한다.
#[test]
fn iknow_의_후행_쉼표는_빈_자리를_알린다() {
    let source = r#"---
description: 결제 정책 문서
---

keyword `금액`: 청구 액수 // iknow `docs`/`B`.`금액`,
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K110");
    // 원문에 없는 빈 문자열을 가리키면 고칠 자리를 못 찾는다.
    assert!(!diagnostics[0].message.contains("\"\""));
    assert!(diagnostics[0].message.contains("쉼표"));
}

/// rev 핀은 큰따옴표로 감싼 값 하나다. 뒤에 토큰이 더 붙으면 조용히 삼키지 않는다.
#[test]
fn rev_핀_뒤에_텍스트가_남으면_에러다() {
    let source = r#"---
description: 결제 정책 문서
---

import `docs`/`A`.`결제` rev "a3f9c1" rev "c40d8a"
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K109");
    assert_eq!(diagnostics[0].locations[0].line, 5);
    assert!(!diagnostics[0].fixes.is_empty());
}

/// `as` 는 `rev` 앞에 온다. 순서가 뒤집히면 별칭이 핀 값으로 삼켜지므로 error 다.
#[test]
fn rev_가_as_보다_앞에_오면_에러다() {
    let source = r#"---
description: 결제 정책 문서
---

import `docs`/`A`.`결제` rev "a3f9c1" as `A 결제`
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K109");
    assert_eq!(diagnostics[0].locations[0].line, 5);
}

/// cover 는 `// iknow` 가 붙는 자리가 아니다 (스펙 4.4).
/// 백틱이 여럿이라는 이유로 "대상은 하나입니다" 라고 하면 엉뚱한 원인을 대는 것이다.
#[test]
fn cover_에_붙은_modifier_는_자리를_알린다() {
    let source = r#"---
description: 결제 정책 문서
---

## 무료상품 결제일 때 청구서

cover `무료상품 청구서 예외` // iknow `docs`/`B`!`무료상품 청구서 예외`
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K111");
    assert!(diagnostics[0].message.contains("modifier"));
    // 사용자가 쓴 대상은 하나다. 개수를 탓하면 거짓이다.
    assert!(!diagnostics[0].message.contains("하나입니다"));
}

/// topic **안**의 문법 오류에 "topic 안으로 옮기세요" 라고 하면 적용할 대상이 없다.
#[test]
fn topic_안의_exception_문법_오류는_옮기라고_하지_않는다() {
    let source = r#"---
description: 결제 정책 문서
---

## 결제 청구서

exception `해외 결제`  pending
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K111");
    // 문법 fix 가 위치를 언급하면 이미 topic 안인 이 줄에는 적용할 대상이 없다.
    assert!(!diagnostics[0].fixes[0].action.contains("topic"));
    assert!(diagnostics[0].fixes[0].action.contains("형식"));
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

/// 첫 topic 뒤의 **선언으로 읽히는** import 줄은 자리가 틀린 선언이다 (스펙 4.7 "파일 최상단").
///
/// 본문으로 흘리면 주소의 백틱이 전부 심볼 참조로 세어져 문서 경로 조각까지 미해결 심볼이
/// 되고, 그 진단이 `docs`·`pay` 같은 일반 명사를 keyword 로 선언하라고 안내한다.
/// 스펙 4.3 이 선언하지 말라고 못박은 이름이 그렇게 SoT 에 박힌다.
#[test]
fn 첫_topic_뒤의_import_선언은_에러다() {
    let source = r#"---
description: 카드 결제 정책
---

## 카드 결제

import `docs`/`A`.`결제` as `A 결제` rev "a3f9c1"
"#;

    let diagnostics = parse::parse_document(문서경로(), source).unwrap_err();

    // 거짓 진단 셋이 아니라 참인 진단 하나다.
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, "K114");
    assert_eq!(diagnostics[0].severity, Severity::Error);
    assert_eq!(diagnostics[0].locations[0].line, 7);
    assert_eq!(diagnostics[0].fixes[0].doc, Some(문서경로()));
    let action = &diagnostics[0].fixes[0].action;
    assert!(action.contains("최상단"), "{action}");
    // 줄 번호를 좌표로 쓰지 않는다 (ADR-0003).
    assert!(!action.contains("7"), "{action}");
}

/// **판정 기준은 "그 줄이 최상단에 있었다면 합법 import 선언인가" 하나다.**
///
/// 접두사만 보면 합법 산문이 거부된다 — 이 저장소의 최악 실패 종류다. 선언으로 읽히지
/// 않는 줄은 산문이므로 그대로 본문이고, 코드펜스 안은 참조에서 아예 빠진다 (스펙 4.2).
#[test]
fn 선언으로_읽히지_않는_import_줄은_산문이다() {
    let source = r#"---
description: kang 사용법 문서
---

## import 를 쓰는 자리

import 를 쓸 때는 파일 최상단에 모아 둔다.

```
import `docs`/`A`.`결제` rev "a3f9c1"
```
"#;

    let doc = parse::parse_document(문서경로(), source).unwrap();

    assert!(doc.imports.is_empty());
    assert!(doc.topics[0].body.contains("import 를 쓸 때는"));
    // 코드펜스 안의 주소는 심볼로 세지 않는다.
    assert!(doc.topics[0].refs.is_empty(), "{:?}", doc.topics[0].refs);
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
