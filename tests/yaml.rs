// YAML 이미터의 인용·이스케이프·literal scalar 규칙을 검증한다.
//
// **이 파일만 라이브러리를 직접 부른다.** 인용 규칙의 위험 지점(한글 description 의 `: `,
// literal scalar 의 후행 개행)은 문서 하나로 만들어 내기 어려운 문자열 단위의 계약이므로,
// `tests/cli.rs` 의 바이너리 호출로는 그 경계를 다 짚을 수 없다. 조립된 뷰가 실제로
// 나오는지는 `tests/cli.rs` 가 바이너리로 본다.
use kang::yaml::{Emitter, scalar};

// ---------------------------------------------------------------------------
// scalar 인용 규칙
// ---------------------------------------------------------------------------

/// 한글 description 에 `: ` 가 들어가면 인용해야 한다. 인용하지 않으면 그 자리에서
/// 매핑이 하나 더 생겨 값이 통째로 다른 구조가 된다.
#[test]
fn 콜론이_포함된_설명은_인용된다() {
    assert_eq!(
        scalar("결제: 대금을 지불하는 행위"),
        "\"결제: 대금을 지불하는 행위\""
    );

    let mut e = Emitter::new();
    e.pair("description", "결제: 대금을 지불하는 행위");
    assert_eq!(e.finish(), "description: \"결제: 대금을 지불하는 행위\"");
}

/// 인용된 스칼라 안의 따옴표와 역슬래시는 이스케이프해야 한다.
/// 이스케이프하지 않으면 인용이 값 중간에서 닫혀 문서가 깨진다.
#[test]
fn 따옴표가_이스케이프된다() {
    // 큰따옴표만 있으면 구조는 안 깨지므로 인용 자체는 필요 없다.
    assert_eq!(scalar("그는 \"결제\" 라고 했다"), "그는 \"결제\" 라고 했다");
    // 인용을 유발하는 `: ` 와 함께 오면 따옴표를 이스케이프해야 한다.
    assert_eq!(
        scalar("주의: \"결제\" 는 \\ 가 아니다"),
        "\"주의: \\\"결제\\\" 는 \\\\ 가 아니다\""
    );
    // 큰따옴표로 시작하면 그 자체가 인용 시작으로 읽히므로 인용 대상이다.
    assert_eq!(scalar("\"결제\""), "\"\\\"결제\\\"\"");
}

/// 평범한 한글 이름과 주소는 인용하지 않는다. 전부 인용하면 출력이 읽기 나빠진다.
#[test]
fn 한글_이름과_주소는_인용하지_않는다() {
    assert_eq!(scalar("결제의 방법"), "결제의 방법");
    assert_eq!(scalar("docs/a#결제의 방법"), "docs/a#결제의 방법");
    assert_eq!(scalar("docs/a.결제수단.카드"), "docs/a.결제수단.카드");
    // 스키마의 bool 자리는 평문이어야 한다.
    assert_eq!(scalar("true"), "true");
    assert_eq!(scalar("false"), "false");
}

/// 다른 타입으로 읽힐 표기는 인용한다. 인용하지 않으면 이름이 숫자나 null 이 된다.
#[test]
fn 타입으로_읽히는_이름은_인용된다() {
    assert_eq!(scalar("2024"), "\"2024\"");
    assert_eq!(scalar("1.5"), "\"1.5\"");
    assert_eq!(scalar("null"), "\"null\"");
    assert_eq!(scalar("no"), "\"no\"");
    assert_eq!(scalar("True"), "\"True\"");
    assert_eq!(scalar(""), "\"\"");
    assert_eq!(scalar(" 앞뒤 공백 "), "\" 앞뒤 공백 \"");
    assert_eq!(scalar("- 목록처럼"), "\"- 목록처럼\"");
    assert_eq!(scalar("주석 # 처럼"), "\"주석 # 처럼\"");
}

// ---------------------------------------------------------------------------
// literal scalar
// ---------------------------------------------------------------------------

/// 멀티라인 본문은 literal scalar(`|`) 로 나온다. folded(`>`) 는 개행을 접어
/// 마크다운 구조를 깬다.
#[test]
fn 멀티라인_본문이_literal_scalar_로_나온다() {
    let mut e = Emitter::new();
    e.block("topic", "## 카드 결제\n\n`결제` 를 하는 수단이다.");

    assert_eq!(
        e.finish(),
        "topic: |2-\n  ## 카드 결제\n\n  `결제` 를 하는 수단이다."
    );
}

/// 후행 개행은 chomping 지시자로 보존한다. 지시자를 고르지 않으면 본문이
/// 개행 하나만큼 달라져 파싱 결과가 원문과 어긋난다.
#[test]
fn literal_scalar_가_후행_개행을_보존한다() {
    // 후행 개행 없음 → strip(`-`)
    let mut 없음 = Emitter::new();
    없음.block("topic", "## 제목\n본문");
    assert_eq!(없음.finish(), "topic: |2-\n  ## 제목\n  본문");

    // 후행 개행 하나 → clip(지시자 없음)
    let mut 하나 = Emitter::new();
    하나.block("topic", "## 제목\n본문\n");
    assert_eq!(하나.finish(), "topic: |2\n  ## 제목\n  본문");

    // 후행 개행 둘 이상 → keep(`+`)
    let mut 둘 = Emitter::new();
    둘.block("topic", "## 제목\n본문\n\n");
    assert_eq!(둘.finish(), "topic: |2+\n  ## 제목\n  본문\n");
}

/// 본문이 이미 들여쓰기를 가진 줄(중첩 목록·코드펜스)을 담아도 그대로 보존한다.
/// 명시적 들여쓰기 지시자가 있으므로 첫 줄이 공백으로 시작해도 깨지지 않는다.
#[test]
fn literal_scalar_가_본문의_들여쓰기를_보존한다() {
    let mut e = Emitter::new();
    e.block("topic", "  ## 들여쓴 제목\n\n- 목록\n  - 중첩");

    assert_eq!(
        e.finish(),
        "topic: |2-\n    ## 들여쓴 제목\n\n  - 목록\n    - 중첩"
    );
}

/// 본문이 비면 literal scalar 로 쓸 수 없다. 빈 문자열로 낸다.
#[test]
fn 빈_본문은_빈_문자열로_나온다() {
    let mut e = Emitter::new();
    e.block("topic", "");
    assert_eq!(e.finish(), "topic: \"\"");
}

// ---------------------------------------------------------------------------
// 구조 조립
// ---------------------------------------------------------------------------

/// 중첩된 seq/map 이 두 칸씩 들여써지고 목록 항목의 뒤 줄이 정렬된다.
#[test]
fn 중첩_구조가_두_칸씩_들여써진다() {
    let mut 항목 = Emitter::new();
    항목.pair("name", "결제");
    항목.seq(
        "referencedBy",
        [
            Emitter::value("docs/a#결제의 방법"),
            Emitter::value("docs/b#카드 결제"),
        ],
    );

    let mut 안쪽 = Emitter::new();
    안쪽.seq("keywords", [항목]);

    let mut e = Emitter::new();
    e.pair("path", "docs/a");
    e.map("references", 안쪽);

    assert_eq!(
        e.finish(),
        "path: docs/a\n\
         references:\n\
         \x20 keywords:\n\
         \x20   - name: 결제\n\
         \x20     referencedBy:\n\
         \x20       - docs/a#결제의 방법\n\
         \x20       - docs/b#카드 결제"
    );
}

/// 빈 목록과 빈 매핑은 null 이 아니라 빈 컬렉션으로 나온다.
/// `key:` 만 찍으면 소비자가 null 을 받는다.
#[test]
fn 빈_목록과_빈_매핑은_빈_컬렉션으로_나온다() {
    let mut e = Emitter::new();
    e.seq("keywords", []);
    e.map("references", Emitter::new());
    assert_eq!(e.finish(), "keywords: []\nreferences: {}");
}
