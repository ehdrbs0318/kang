// `hash::rev` 동작을 검증하는 통합 테스트. 정규화 규칙도 `rev` 의 출력으로만 본다 —
// `hash::normalize` 를 직접 부르는 테스트는 이 파일에 없다.
use kang::hash;

/// **rev 의 실제 값을 리터럴로 고정한다.**
///
/// 나머지 테스트는 전부 `rev(a) == rev(b)` 같은 상대 비교라, 알고리즘을 SHA-256 에서 다른
/// 것으로 바꾸거나 절단을 6자리에서 8자리로 늘리거나 정규화 규칙을 하나 더해도 **전부
/// 통과한다.** 그러면 이 저장소와 모든 소비자 문서의 `rev = "..."` 핀이 한꺼번에 무효가
/// 되고, 그 사실을 아무도 알려 주지 않는다. 핀은 kang 의 중심 계약이다.
///
/// 여섯 값이 각각 고정하는 것:
///
/// - `""` → `e3b0c4` — **SHA-256 이다.** 빈 문자열의 SHA-256 은
///   `e3b0c44298fc1c14...` 이므로 이 값 하나가 해시 함수와 "정규화가 빈 입력을 그대로
///   둔다" 를 함께 못박는다.
/// - `"hello"` → `2cf24d` — SHA-256("hello") 의 앞 6자리다. 정규화가 평범한 한 줄을
///   건드리지 않는다.
/// - `"결제의 방법"` → `099357` — UTF-8 **바이트**를 해시한다. 인코딩을 바꾸면 깨진다.
/// - `"hello\nworld"` → `26c60a` — 여러 줄의 기준값.
/// - `"hello   \nworld  "` → `26c60a` — 줄 끝 공백이 제거된다. 위와 **같은 값**이어야
///   한다는 것이 정규화 규칙 하나다.
/// - `"hello\n\n\n\nworld"` → `dc90a9` — 연속 빈 줄이 **하나로** 축약된다. 위와 다른
///   값인 것이 요점이다 — 빈 줄이 0개로 접히면 `26c60a` 가 된다.
///
/// **이 값들을 고쳐야 할 때는 그것이 breaking change 다.** 소비자 문서의 핀을 전부
/// 다시 계산해야 하므로 CHANGELOG 에 적고 버전을 올린다.
#[test]
fn rev_is_fixed_first_6_of_sha256() {
    assert_eq!(hash::rev(""), "e3b0c4", "빈 입력 — SHA-256 과 절단 자릿수");
    assert_eq!(hash::rev("hello"), "2cf24d", "ascii 한 줄");
    assert_eq!(hash::rev("결제의 방법"), "099357", "UTF-8 바이트");
    assert_eq!(hash::rev("hello\nworld"), "26c60a", "여러 줄 기준값");
    assert_eq!(
        hash::rev("hello   \nworld  "),
        "26c60a",
        "줄 끝 공백 제거 — 기준값과 같아야 한다"
    );
    assert_eq!(
        hash::rev("hello\n\n\n\nworld"),
        "dc90a9",
        "빈 줄은 하나로 축약 — 0개로 접히면 기준값과 같아진다"
    );
}

/// 출력은 **언제나** 6자리 소문자 16진수다.
///
/// 인덱스 TSV 의 가운데 필드가 이 형식이고(`crates/kang/src/index.rs`), 매크로와
/// TypeScript 소비자가 그 형식으로 파싱한다. 자릿수가 흔들리면 그 파서들이 조용히
/// 어긋난다.
#[test]
fn rev_is_always_6_lowercase_hex() {
    // 길이가 다른 입력을 섞는다 — 짧은 입력에서 앞자리가 0 이 되는 경우를 포함한다.
    for input in [
        "",
        "a",
        "결제",
        "긴 본문\n여러 줄\n\n계속",
        &"x".repeat(10_000),
    ] {
        let pin = hash::rev(input);
        assert_eq!(pin.len(), 6, "6자리여야 한다: {pin:?}");
        assert!(
            pin.chars().all(|ch| matches!(ch, '0'..='9' | 'a'..='f')),
            "소문자 16진수여야 한다: {pin:?}"
        );
    }
}

/// 줄 끝 공백이 제거되어 해시가 동일해야 한다.
#[test]
fn trailing_whitespace_does_not_change_hash() {
    let a = "hello\nworld";
    let b = "hello   \nworld  ";
    assert_eq!(hash::rev(a), hash::rev(b));
}

/// 연속된 빈 줄이 하나로 축약되어 해시가 동일해야 한다.
#[test]
fn consecutive_blank_lines_collapse_to_one() {
    let a = "hello\n\nworld";
    let b = "hello\n\n\n\nworld";
    assert_eq!(hash::rev(a), hash::rev(b));
}

/// 본문이 다르면 해시도 달라야 한다.
#[test]
fn different_body_gives_different_hash() {
    let a = "hello\nworld";
    let b = "hello\nkang";
    assert_ne!(hash::rev(a), hash::rev(b));
}
