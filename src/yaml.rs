//! `kang show` 가 쓰는 YAML 이미터.
//!
//! ponytail: 직렬화 크레이트를 들이지 않고 직접 쓴다. 스키마가 스펙 6.4 하나로 고정이고,
//! 필요한 것은 **literal scalar 제어**다 — 멀티라인 본문의 개행이 접히면 마크다운 구조가
//! 깨지므로 어떤 스칼라 형식으로 나가는지를 이미터가 정해야 한다. 한계는 인용 규칙이며
//! ([`scalar`] 가 그 경계를 명명한다), 스키마가 늘어 규칙이 갈라지면 serde 기반 크레이트로 옮긴다.
//!
//! **수동 들여쓰기 커서를 노출하지 않는다.** [`Emitter::seq`] 와 [`Emitter::map`] 은 자식
//! 이미터를 통째로 받아 자기가 들여쓴다. 여는 자리와 닫는 자리를 손으로 맞추는 API 였다면
//! 짝이 어긋난 YAML 을 만들 수 있지만, 여기서는 그것이 구조적으로 불가능하다.

/// YAML 문서를 조립한다.
///
/// 완성된 줄들을 그대로 들고 있다가 [`Emitter::finish`] 에서 잇는다. 중첩은 자식 이미터의
/// 줄에 들여쓰기를 붙이는 방식이므로, 자식은 자기가 몇 단계 안에 놓일지 알 필요가 없다.
#[derive(Default)]
pub struct Emitter {
    /// 지금까지 만들어진 줄들. 들여쓰기는 이미 반영되어 있고 개행은 들어 있지 않다.
    lines: Vec<String>,
}

impl Emitter {
    /// 빈 이미터를 만든다.
    ///
    /// # 반환값
    /// 아무 줄도 없는 이미터
    pub fn new() -> Self {
        Self::default()
    }

    /// 스칼라 하나만 담은 이미터를 만든다.
    ///
    /// 목록의 항목이 매핑이 아니라 스칼라인 자리에 쓴다 — 스펙 6.4 의 `referencedBy` 가
    /// 그렇고, 중복 제거된 두 번째 도달을 경로 문자열 하나로 줄이는 자리가 그렇다.
    ///
    /// # 매개변수
    /// - `value`: 담을 값
    ///
    /// # 반환값
    /// 스칼라 한 줄짜리 이미터
    pub fn value(value: &str) -> Self {
        Self {
            lines: vec![scalar(value)],
        }
    }

    /// `키: 값` 한 줄을 넣는다.
    ///
    /// # 매개변수
    /// - `key`: 스키마가 정한 키. 스펙 6.4 의 키는 전부 ASCII 이므로 인용하지 않는다
    /// - `value`: 값. [`scalar`] 의 규칙으로 인용된다
    pub fn pair(&mut self, key: &str, value: &str) {
        self.lines.push(format!("{key}: {}", scalar(value)));
    }

    /// literal scalar(`|`) 로 멀티라인 본문을 넣는다.
    ///
    /// folded scalar(`>`) 는 개행이 접혀 마크다운이 깨지므로 쓰지 않는다.
    ///
    /// **명시적 들여쓰기 지시자(`|2`)를 쓴다.** 지시자가 없으면 파서가 첫 줄에서
    /// 들여쓰기를 추론하는데, topic 본문의 첫 줄인 헤딩이 들여써져 있으면
    /// (`  ## 제목` 도 합법 문서다) 그 공백이 통째로 사라진다.
    ///
    /// **후행 개행은 chomping 지시자로 보존한다.** 본문이 개행으로 끝나지 않으면 strip(`-`),
    /// 하나로 끝나면 clip(지시자 없음), 둘 이상이면 keep(`+`) 이다. 이 계산은 마지막 줄
    /// 뒤에 개행이 **정확히 하나** 붙는다고 전제한다 — [`Emitter::finish`] 가 마지막 개행을
    /// 붙이지 않고 호출자의 `writeln!` 이 그 하나를 붙인다.
    ///
    /// # 매개변수
    /// - `key`: 스키마가 정한 키
    /// - `body`: 원문 그대로의 본문
    pub fn block(&mut self, key: &str, body: &str) {
        // 빈 본문은 literal scalar 로 쓸 수 없다. 내용 없는 블록은 파서마다 해석이
        // 갈리므로 빈 문자열로 낸다.
        if body.is_empty() {
            self.lines.push(format!("{key}: \"\""));
            return;
        }

        // 개행으로 끝나는 본문은 split 이 마지막에 빈 조각을 하나 더 낸다. 그 조각은
        // 줄이 아니라 마지막 줄의 끝이므로 버린다 — chomping 지시자가 그 자리를 대신한다.
        let 줄들 = body.strip_suffix('\n').unwrap_or(body).split('\n');
        let chomping = if !body.ends_with('\n') {
            "-"
        } else if body.ends_with("\n\n") {
            "+"
        } else {
            ""
        };

        self.lines.push(format!("{key}: |2{chomping}"));
        // 본문 줄을 두 칸 들여써 넣는다. 빈 줄은 들여쓰지 않는다 — 공백만 남은 줄은
        // 눈에 보이지 않는 차이를 만들고, 빈 줄은 들여쓰기 없이도 블록의 내용이다.
        for 줄 in 줄들 {
            self.lines.push(if 줄.is_empty() {
                String::new()
            } else {
                format!("  {줄}")
            });
        }
    }

    /// 키 아래에 항목 목록을 넣는다.
    ///
    /// # 매개변수
    /// - `key`: 스키마가 정한 키
    /// - `items`: 항목마다 하나씩인 이미터들. 스칼라 항목은 [`Emitter::value`] 로 만든다
    pub fn seq(&mut self, key: &str, items: impl IntoIterator<Item = Emitter>) {
        let items: Vec<Emitter> = items.into_iter().collect();
        // 항목이 없으면 `키:` 만 남아 소비자가 null 을 받는다. 빈 목록이라고 말한다.
        if items.is_empty() {
            self.lines.push(format!("{key}: []"));
            return;
        }

        self.lines.push(format!("{key}:"));
        // 항목마다 첫 줄에 `- ` 를 붙이고 나머지 줄을 그 자리에 맞춰 들여쓴다.
        for item in items {
            for (자리, 줄) in item.lines.into_iter().enumerate() {
                self.lines.push(if 줄.is_empty() {
                    String::new()
                } else if 자리 == 0 {
                    format!("  - {줄}")
                } else {
                    format!("    {줄}")
                });
            }
        }
    }

    /// 키 아래에 중첩 매핑을 넣는다.
    ///
    /// # 매개변수
    /// - `key`: 스키마가 정한 키
    /// - `body`: 중첩될 매핑
    pub fn map(&mut self, key: &str, body: Emitter) {
        // 빈 매핑도 목록과 같은 이유로 명시한다.
        if body.lines.is_empty() {
            self.lines.push(format!("{key}: {{}}"));
            return;
        }

        self.lines.push(format!("{key}:"));
        // 자식의 줄을 통째로 두 칸 들여쓴다.
        for 줄 in body.lines {
            self.lines.push(if 줄.is_empty() {
                String::new()
            } else {
                format!("  {줄}")
            });
        }
    }

    /// 지금까지 넣은 것을 YAML 텍스트로 만든다.
    ///
    /// **마지막 개행을 붙이지 않는다.** 호출자가 `writeln!` 로 정확히 하나를 붙인다.
    /// 여기서 붙이면 개행이 둘이 되어 keep chomping 한 본문의 끝이 한 줄 늘어난다.
    ///
    /// # 반환값
    /// 조립된 YAML 텍스트
    pub fn finish(self) -> String {
        self.lines.join("\n")
    }
}

/// 스칼라를 안전하게 인용한다.
///
/// 인용을 유발하는 것은 **구조를 깨는 표기**와 **다른 타입으로 읽히는 표기** 둘이다.
///
/// - 빈 문자열, 앞뒤 공백, 제어 문자 (개행 포함)
/// - 지시자로 시작 (`- ? : , [ ] { } # & * ! | > ' " % @ \` \` 와 역슬래시)
/// - `: ` 를 포함하거나 `:` 로 끝남 — **한글 description 의 위험 지점이다.**
///   `결제: 대금을 지불하는 행위` 를 그대로 두면 그 자리에서 매핑이 하나 더 생긴다
/// - ` #` 를 포함 — 뒤가 주석이 된다
/// - 숫자로 읽힐 표기(숫자·`+`·`.` 로 시작)와 null/bool 표기
///
/// ponytail: YAML 스펙의 전체 규칙이 아니라 **이 스키마에 필요한 만큼**이다. 값에 오는
/// 것은 문서 경로·심볼 이름·한 줄 정의·`true`/`false` 뿐이며, 그 넷에서 구조를 깨거나
/// 타입을 바꾸는 표기만 본다. 판단이 갈리면 인용하는 쪽으로 기운다 — 불필요한 인용은
/// 읽기만 나빠지지만 빠뜨린 인용은 문서를 깬다. 스키마에 자유 텍스트 필드가 늘어나면
/// 이 규칙 대신 직렬화 크레이트를 쓴다.
///
/// **`true` 와 `false` 는 인용하지 않는다.** 스펙 6.4 의 `pending`·`uncoded` 가 bool 이며,
/// 그 자리에 오는 문자열은 이미터가 만든 것이다. 그래서 정확히 그 둘만 예외이고
/// `True`·`yes` 같은 다른 표기는 인용한다.
///
/// # 매개변수
/// - `s`: 인용할 값
///
/// # 반환값
/// 그대로 쓸 수 있는 평문, 또는 큰따옴표로 감싸고 이스케이프한 문자열
pub fn scalar(s: &str) -> String {
    let 타입으로_읽힘 = s != "true"
        && s != "false"
        && (matches!(
            s.to_ascii_lowercase().as_str(),
            "null" | "nil" | "~" | "true" | "false" | "yes" | "no" | "on" | "off"
        ) || s.starts_with(|c: char| c.is_ascii_digit())
            || s.starts_with(['+', '.']));

    let 인용 = s.is_empty()
        || s.trim() != s
        || s.starts_with([
            '-', '?', ':', ',', '[', ']', '{', '}', '#', '&', '*', '!', '|', '>', '\'', '"', '%',
            '@', '`', '\\',
        ])
        || s.contains(": ")
        || s.ends_with(':')
        || s.contains(" #")
        || s.chars().any(char::is_control)
        || 타입으로_읽힘;

    if !인용 {
        return s.to_string();
    }

    let mut 결과 = String::with_capacity(s.len() + 2);
    결과.push('"');
    // 큰따옴표 안에서 뜻이 달라지는 문자만 이스케이프한다. 그 밖의 문자는 UTF-8 그대로다.
    for c in s.chars() {
        match c {
            '\\' => 결과.push_str("\\\\"),
            '"' => 결과.push_str("\\\""),
            '\n' => 결과.push_str("\\n"),
            '\t' => 결과.push_str("\\t"),
            '\r' => 결과.push_str("\\r"),
            // 그 밖의 제어 문자는 눈에 보이지 않으므로 코드로 적는다.
            c if c.is_control() => 결과.push_str(&format!("\\x{:02x}", c as u32)),
            c => 결과.push(c),
        }
    }
    결과.push('"');
    결과
}
