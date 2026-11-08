//! `rev` 해시 산출 모듈.
//!
//! kang 문서가 다른 문서의 심볼을 `import` 할 때, 대상 본문의 해시(`rev`)를
//! 핀으로 박아 둔다. 대상 본문이 바뀌면 `rev` 값이 달라져 참조처가
//! 컴파일 에러로 깨지는 식으로 동작한다.

use kang_macros as kang;
use sha2::{Digest, Sha256};

/// 해시 입력 텍스트를 정규화한다.
///
/// 정규화 규칙:
/// - 각 줄의 줄 끝 공백(trailing whitespace)을 제거한다.
/// - 연속된 빈 줄은 하나로 축약한다.
/// - 텍스트 앞뒤의 공백(개행 포함)을 제거한다.
///
/// # 매개변수
/// - `text`: 정규화할 원본 텍스트
///
/// # 반환값
/// 정규화된 텍스트
pub fn normalize(text: &str) -> String {
    let mut normalized_lines: Vec<&str> = Vec::new();

    // 줄 단위로 순회하며 줄 끝 공백 제거 및 연속 빈 줄 축약
    for line in text.lines() {
        let trimmed = line.trim_end();
        // 직전 줄도 빈 줄이고 현재 줄도 빈 줄이면 축약(건너뜀)
        if trimmed.is_empty() && normalized_lines.last().is_some_and(|l| l.is_empty()) {
            continue;
        }
        normalized_lines.push(trimmed);
    }

    normalized_lines.join("\n").trim().to_string()
}

/// 문서 원문의 해시, 곧 정규화한 텍스트의 SHA-256 전체 hex 문자열(64자)을 반환한다.
///
/// 그래프 인덱스가 문서 원문 해시로 쓴다. 맨 앞 BOM(U+FEFF) 하나는 떼고 계산한다.
/// 컴파일러는 BOM 을 떼고 문서를 읽고, 편집기는 BOM 을 줄 수도 주지 않을 수도 있기 때문이다.
/// [`normalize`] 와 [`rev`] 는 BOM 을 떼지 않는다. 그래야 rev 핀이 바뀌지 않는다.
///
/// # 매개변수
/// - `text`: 문서 파일 원문
///
/// # 반환값
/// SHA-256 해시값의 소문자 hex 문자열 64자
pub fn doc_hash(text: &str) -> String {
    sha256_hex(text.strip_prefix('\u{feff}').unwrap_or(text))
}

/// 정규화된 텍스트의 SHA-256 해시 전체 hex 문자열(64자)을 반환한다.
///
/// # 매개변수
/// - `text`: 해시를 계산할 원본 텍스트 (내부적으로 [`normalize`] 를 거친다)
///
/// # 반환값
/// SHA-256 해시값의 소문자 hex 문자열 64자
fn sha256_hex(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(normalize(text).as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

/// 정규화된 텍스트의 SHA-256 해시 앞 6자리 hex 문자열을 반환한다.
///
/// # 매개변수
/// - `text`: 해시를 계산할 원본 텍스트 (내부적으로 [`normalize`] 를 거친다)
///
/// # 반환값
/// SHA-256 해시값의 앞 6자리 hex 문자열
#[kang::keyword("CONTEXT.rev 핀", rev = "718dd2")]
pub fn rev(text: &str) -> String {
    // 전체 hex 문자열 중 앞 6자리만 사용
    sha256_hex(text)[..6].to_string()
}
