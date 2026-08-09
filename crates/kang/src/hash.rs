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

/// 정규화된 텍스트의 SHA-256 해시 앞 6자리 hex 문자열을 반환한다.
///
/// # 매개변수
/// - `text`: 해시를 계산할 원본 텍스트 (내부적으로 [`normalize`] 를 거친다)
///
/// # 반환값
/// SHA-256 해시값의 앞 6자리 hex 문자열
#[kang::keyword("CONTEXT.rev 핀", rev = "29c60b")]
pub fn rev(text: &str) -> String {
    let normalized = normalize(text);

    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    let digest = hasher.finalize();

    // 전체 hex 문자열 중 앞 6자리만 사용
    digest
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()[..6]
        .to_string()
}
