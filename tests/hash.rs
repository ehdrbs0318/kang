// `hash::normalize` / `hash::rev` 동작을 검증하는 통합 테스트.
use kang::hash;

/// 줄 끝 공백이 제거되어 해시가 동일해야 한다.
#[test]
fn 줄_끝_공백은_해시를_바꾸지_않는다() {
    let a = "hello\nworld";
    let b = "hello   \nworld  ";
    assert_eq!(hash::rev(a), hash::rev(b));
}

/// 연속된 빈 줄이 하나로 축약되어 해시가 동일해야 한다.
#[test]
fn 연속_빈_줄은_하나로_축약된다() {
    let a = "hello\n\nworld";
    let b = "hello\n\n\n\nworld";
    assert_eq!(hash::rev(a), hash::rev(b));
}

/// 본문이 다르면 해시도 달라야 한다.
#[test]
fn 본문이_다르면_해시가_다르다() {
    let a = "hello\nworld";
    let b = "hello\nkang";
    assert_ne!(hash::rev(a), hash::rev(b));
}
