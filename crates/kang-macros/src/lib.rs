//! `kang` 심볼을 Rust 코드에서 가리키는 속성 매크로 (V0003 §4·§5).
//!
//! **컴파일 타임에 검증한다.** `kang index` 가 낸 심볼 인덱스를 읽어 심볼의 실재와
//! rev 일치를 확인하고, 어긋나면 컴파일 에러를 낸다. `kang inspect` 이전에 빌드에서
//! 잡히는 것이 이 크레이트의 존재 이유다.
//!
//! 매크로는 원본 아이템을 그대로 반환하므로 런타임 비용이 0 이다.
//!
//! **인덱스가 없으면 warn 을 내고 통과한다** — 부트스트랩이 가능해야 하기 때문이다
//! (kang 자신의 코드가 이 매크로를 쓰려면 `kang index` 가 먼저 돌아야 하고, 그 바이너리는
//! 매크로가 붙은 코드를 컴파일해야 만들어진다). `KANG_REQUIRE_INDEX=1` 이면 컴파일
//! 에러로 승격한다 — CI 와 릴리즈에서 켠다. 조용히 통과시키면 인덱스를 지웠을 때
//! 검증이 사라지는데 빌드가 성공한다.
//!
//! # 인덱스를 찾는 순서
//!
//! 1. 환경 변수 `KANG_INDEX` — 컴파일 타임에 읽는다. `.cargo/config.toml` 의 `[env]` 나
//!    소비자의 `build.rs` (`cargo::rustc-env`) 로 줄 수 있다.
//! 2. `CARGO_MANIFEST_DIR` 부터 위로 훑어 처음 만나는 `.kang/index.tsv`. 워크스페이스
//!    루트에 인덱스를 두는 경우가 이 경로다 — 매크로는 워크스페이스 루트를 모른다.
//!
//! **재빌드 추적은 [`build.rs`](../build.rs) 가 붙인다.** 매크로가 파일을 읽는 것은 cargo
//! 에게 보이지 않으므로 그것 없이는 문서를 고쳐도 아무것도 재컴파일되지 않는다.
//!
//! # 인덱스 형식
//!
//! `{종류}\t{rev}\t{주소}` 한 줄에 심볼 하나다. **가변 길이 필드가 마지막이라** 파서가
//! `splitn(3, '\t')` 세 줄로 끝나고, 심볼 이름에 든 탭이 주소를 자르지 않는다
//! (V0004 Task 3). 인덱스 파서는 V0001 §10.1 의 허용 의존성 목록에 없으므로 손으로 쓴다.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as 토큰들;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use syn::parse::{ParseStream, Parser};
use syn::{Ident, LitStr, Token};

/// 인덱스를 찾지 못했을 때 위로 훑는 관례 경로.
const 관례_경로: &str = ".kang/index.tsv";

/// kang topic 을 참조한다.
///
/// ```ignore
/// #[kang::topic("docs/A#결제의 방법", rev = "a3f9c1")]
/// pub fn process_payment() {}
/// ```
///
/// # 매개변수
/// - `attr`: `"<주소>", rev = "<핀>"`
/// - `item`: 속성이 붙은 아이템
///
/// # 반환값
/// 원본 아이템 그대로. 검증이 실패하면 그 앞에 `compile_error!` 가 붙는다
#[proc_macro_attribute]
pub fn topic(attr: TokenStream, item: TokenStream) -> TokenStream {
    확장("topic", attr.into(), item.into()).into()
}

/// kang keyword 를 참조한다.
///
/// ```ignore
/// #[kang::keyword("docs/A.결제", rev = "8f4efc")]
/// pub struct Payment;
/// ```
///
/// # 매개변수
/// - `attr`: `"<주소>", rev = "<핀>"`
/// - `item`: 속성이 붙은 아이템
///
/// # 반환값
/// 원본 아이템 그대로. 검증이 실패하면 그 앞에 `compile_error!` 가 붙는다
#[proc_macro_attribute]
pub fn keyword(attr: TokenStream, item: TokenStream) -> TokenStream {
    확장("keyword", attr.into(), item.into()).into()
}

/// kang exception 을 커버하는 구현임을 표시한다.
///
/// 이름만 다르고 검증하는 종류는 `exception` 이다 — 코드는 예외를 *커버*하므로
/// 문서 문법의 `cover` 와 같은 낱말을 쓴다.
///
/// ```ignore
/// #[kang::covers("docs/A!해외 결제", rev = "cc9e1c")]
/// pub fn settle_overseas() {}
/// ```
///
/// # 매개변수
/// - `attr`: `"<주소>", rev = "<핀>"`
/// - `item`: 속성이 붙은 아이템
///
/// # 반환값
/// 원본 아이템 그대로. 검증이 실패하면 그 앞에 `compile_error!` 가 붙는다
#[proc_macro_attribute]
pub fn covers(attr: TokenStream, item: TokenStream) -> TokenStream {
    확장("exception", attr.into(), item.into()).into()
}

/// 세 매크로가 공유하는 본체.
///
/// 종류 문자열만 다르다. 사본을 만들면 진단 문면이 갈린다.
///
/// # 매개변수
/// - `종류`: 인덱스의 종류 칸 값 (`keyword`·`topic`·`exception`)
/// - `attr`: 속성 인자 토큰
/// - `item`: 속성이 붙은 아이템 토큰
///
/// # 반환값
/// 성공하면 `item` 그대로, 실패하면 `compile_error!` + `item`
fn 확장(종류: &str, attr: 토큰들, item: 토큰들) -> 토큰들 {
    // 인자 형식이 어긋나면 인덱스를 볼 이유가 없다. syn 의 기본 문면("expected `,`")
    // 만으로는 무엇을 적어야 하는지 알 수 없으므로 형식 안내를 함께 낸다.
    let 파서: fn(ParseStream) -> syn::Result<(LitStr, LitStr)> = 인자;
    let (주소, 핀) = match 파서.parse2(attr) {
        Ok(값) => 값,
        Err(오류) => {
            let 문장 = format!("{오류} — {}", 형식_안내(종류));
            return 붙여(syn::Error::new(오류.span(), 문장), item);
        }
    };

    match 인덱스() {
        Ok((경로, 내용)) => match 검사(내용, 종류, &주소.value(), &핀.value(), 경로)
        {
            Ok(()) => item,
            // 위치는 주소 리터럴에 붙인다. rustc 가 파일·줄·밑줄을 그려 주므로
            // 진단이 좌표를 직접 적지 않는다 (ADR-0003).
            Err(문장) => 붙여(syn::Error::new(주소.span(), 문장), item),
        },
        // 인덱스가 없어도 통과시킨다 (V0004 B3) — 부트스트랩이 가능해야 한다.
        // `KANG_REQUIRE_INDEX` 가 켜지면 그 부재가 컴파일 에러다. **꼬리 문장이 warn 과
        // 다르다** — 여기서는 통과하지 않았으므로 "검증되지 않습니다" 가 거짓이다.
        Err(사유) if 필수() => {
            let 문장 = format!(
                "{사유}  `KANG_REQUIRE_INDEX` 가 켜져 있어 인덱스 부재가 컴파일 에러입니다."
            );
            붙여(syn::Error::new(주소.span(), 문장), item)
        }
        // warn 은 로더가 한 번만 냈다. 여기서 또 내면 속성 개수만큼 반복된다.
        Err(_) => item,
    }
}

/// 진단을 원본 아이템 **앞에** 붙인다.
///
/// 아이템은 한 토큰도 손대지 않는다. 아이템을 함께 내는 것은 이어지는 "이름을 찾을 수
/// 없다" 연쇄 에러를 막기 위해서다 — 진단 하나만 보이는 것이 5.1.1 의 목적에 맞는다.
///
/// # 매개변수
/// - `오류`: 낼 진단
/// - `item`: 원본 아이템 토큰
///
/// # 반환값
/// `compile_error!` 뒤에 `item` 이 그대로 이어진 토큰
fn 붙여(오류: syn::Error, item: 토큰들) -> 토큰들 {
    let mut 결과 = 오류.to_compile_error();
    결과.extend(item);
    결과
}

/// 속성 인자 `"<주소>", rev = "<핀>"` 를 가른다.
///
/// # 매개변수
/// - `입력`: 속성 인자 토큰 스트림
///
/// # 반환값
/// `(주소 리터럴, 핀 리터럴)`
///
/// # 오류
/// 형식이 어긋나면 그 자리의 [`syn::Error`]
fn 인자(입력: ParseStream) -> syn::Result<(LitStr, LitStr)> {
    let 주소: LitStr = 입력.parse()?;
    입력.parse::<Token![,]>()?;
    let 이름: Ident = 입력.parse()?;
    // `rev` 외의 키는 받지 않는다. 조용히 무시하면 핀 없는 참조가 검증을 통과한다.
    if 이름 != "rev" {
        return Err(syn::Error::new(이름.span(), "`rev` 만 받습니다"));
    }
    입력.parse::<Token![=]>()?;
    let 핀: LitStr = 입력.parse()?;
    Ok((주소, 핀))
}

/// 인덱스 한 줄을 `(종류, rev, 주소)` 로 가른다.
///
/// 칸이 셋보다 적은 줄은 버린다 — 빈 줄과 잘린 줄이 조용히 심볼로 읽히지 않게 한다.
///
/// # 매개변수
/// - `인덱스`: 인덱스 파일 내용
///
/// # 반환값
/// 줄마다 `(종류, rev, 주소)`
fn 줄들(인덱스: &str) -> impl Iterator<Item = (&str, &str, &str)> {
    인덱스.lines().filter_map(|줄| {
        let mut 칸 = 줄.splitn(3, '\t');
        Some((칸.next()?, 칸.next()?, 칸.next()?))
    })
}

/// 주소와 핀을 인덱스에 비춰 본다. 세 매크로의 유일한 판정이다.
///
/// # 매개변수
/// - `인덱스`: 인덱스 파일 내용
/// - `종류`: 속성이 주장하는 종류
/// - `주소`: 속성이 가리키는 심볼 주소
/// - `핀`: 속성에 적힌 rev
/// - `인덱스_경로`: 진단에 적을 인덱스 위치
///
/// # 오류
/// 어긋나면 스펙 5.1.1 의 세 요소(무엇이 틀렸나 / 어디인가 / 어떻게 고치나)를 담은 문장
fn 검사(
    인덱스: &str,
    종류: &str,
    주소: &str,
    핀: &str,
    인덱스_경로: &Path,
) -> Result<(), String> {
    let 위치 = 인덱스_경로.display();

    // 종류까지 맞는 줄이 있으면 남은 판정은 핀 하나다.
    if let Some((_, 실제, _)) = 줄들(인덱스).find(|&(k, _, a)| k == 종류 && a == 주소) {
        if 실제 == 핀 {
            return Ok(());
        }
        return Err(format!(
            "kang rev 핀이 대상의 현재 내용과 다릅니다 — {종류} `{주소}`\n\
             \n  핀 {핀}, 현재 {실제}\n  인덱스 {위치}\n\
             \n  참조한 뒤 대상 문서가 바뀌었습니다. 이 코드가 여전히 맞는지 확인한 뒤 핀을 갱신하세요.\n\
             \n  fix:\n    [edit] 이 속성의 rev = \"{핀}\" 을 rev = \"{실제}\" 으로 바꾸세요\n"
        ));
    }

    // 주소는 있는데 종류가 다른 경우. 주소의 구분자와 속성 이름이 어긋난 것이며,
    // 이때 "심볼이 없다" 고 말하면 사용자가 있는 심볼을 찾아 헤맨다.
    if let Some((실제종류, 실제, _)) = 줄들(인덱스).find(|&(_, _, a)| a == 주소) {
        return Err(format!(
            "kang 심볼의 종류가 다릅니다 — `{주소}` 는 {종류} 가 아니라 {실제종류} 입니다\n\
             \n  인덱스 {위치}\n\
             \n  주소의 구분자와 속성 이름이 어긋났습니다. keyword 는 `.`, topic 은 `#`, exception 은 `!` 로 잇습니다.\n\
             \n  fix:\n    [edit] `#[kang::{}(\"{주소}\", rev = \"{실제}\")]` 로 바꾸세요\n",
            속성_이름(실제종류)
        ));
    }

    Err(format!(
        "kang 심볼이 인덱스에 없습니다 — {종류} `{주소}`\n\
         \n  인덱스 {위치}\n\
         \n  이 주소로 선언된 {종류} 이 인덱스에 없습니다. 인덱스가 낡았거나 주소가 틀렸습니다.\n\
         \n  fix (인덱스가 낡았을 때):\n    [shell] kang index {}\n\
         \n  fix (주소가 틀렸을 때):\n    [edit] 이 속성의 주소를 문서가 선언한 심볼로 바꾸거나, 그 심볼을 문서에 선언하세요\n",
        셸_인용(&위치.to_string())
    ))
}

/// 인덱스의 종류 이름에 대응하는 속성 이름.
///
/// `exception` 만 이름이 다르다 — 코드는 예외를 커버하므로 `covers` 다.
///
/// # 매개변수
/// - `종류`: 인덱스의 종류 칸 값
///
/// # 반환값
/// `#[kang::…]` 에 적는 이름
fn 속성_이름(종류: &str) -> &str {
    // exception 을 가리키는 속성은 `covers` 다. 나머지는 종류 이름과 같다.
    if 종류 == "exception" {
        "covers"
    } else {
        종류
    }
}

/// 속성 인자 형식 안내. 파싱이 실패했을 때 붙는 처방이다.
///
/// # 매개변수
/// - `종류`: 인덱스의 종류 칸 값
///
/// # 반환값
/// 올바른 속성 한 줄
fn 형식_안내(종류: &str) -> String {
    format!(
        "kang 속성은 `#[kang::{}(\"<주소>\", rev = \"<핀>\")]` 형태입니다",
        속성_이름(종류)
    )
}

/// 셸에 그대로 붙일 수 있게 인용한다.
///
/// `crates/kang/src/resolve.rs` 의 `셸_인용` 과 같은 규약이다. 크레이트가 갈려 있어
/// 한 줄을 다시 쓴다 — 인덱스 파서를 손으로 쓰는 것과 같은 이유다 (V0001 §10.1).
///
/// # 매개변수
/// - `값`: 인용할 문자열
///
/// # 반환값
/// 단일 인용부호로 감싼 문자열
fn 셸_인용(값: &str) -> String {
    format!("'{}'", 값.replace('\'', r"'\''"))
}

/// `KANG_REQUIRE_INDEX` 가 켜졌는지.
///
/// `KANG_REQUIRE_YAML` 의 선례와 같다 — 값이 아니라 **존재**를 본다.
///
/// # 반환값
/// 인덱스 부재를 컴파일 에러로 올려야 하면 `true`
fn 필수() -> bool {
    std::env::var_os("KANG_REQUIRE_INDEX").is_some()
}

/// 인덱스를 한 번 읽어 캐시한다.
///
/// 속성 하나마다 파일을 다시 읽으면 warn 도 그만큼 반복된다. 한 컴파일 안에서
/// 인덱스는 바뀌지 않으므로 한 번 읽는 것이 맞다.
///
/// # 반환값
/// `(인덱스 경로, 내용)` 또는 읽지 못한 사정
fn 인덱스() -> &'static Result<(PathBuf, String), String> {
    static 캐시: OnceLock<Result<(PathBuf, String), String>> = OnceLock::new();
    캐시.get_or_init(|| {
        let 결과 = 읽기();
        // warn 은 여기서 한 번만 낸다. `필수()` 면 `확장` 이 에러로 내므로 겹쳐 말하지 않는다.
        //
        // **꼬리 문장이 모드마다 다르다.** 통과시킬 때만 "검증되지 않습니다" 가 참이다 —
        // 에러로 세운 빌드에 그 말을 붙이면 진단이 거짓을 말한다.
        if let Err(사유) = &결과
            && !필수()
        {
            eprintln!(
                "warning: {사유} 이 빌드에서 kang 속성은 검증되지 않습니다. \
                 `KANG_REQUIRE_INDEX` 를 켜면 컴파일 에러가 됩니다."
            );
        }
        결과
    })
}

/// 인덱스 파일을 찾아 읽는다.
///
/// # 반환값
/// `(인덱스 경로, 내용)`
///
/// # 오류
/// 찾지 못했거나 읽지 못한 사정. 둘은 다른 사실이므로 다른 문장을 쓴다 —
/// 경로를 아는데 "찾지 못했다" 고 말하면 진단이 거짓이 된다
fn 읽기() -> Result<(PathBuf, String), String> {
    let 경로 = 경로()?;
    std::fs::read_to_string(&경로)
        .map(|내용| (경로.clone(), 내용))
        .map_err(|오류| {
            // 경로는 안다. 그러므로 처방의 인덱스 경로도 **그 경로** 다 — 관례 경로를
            // 적으면 `KANG_INDEX` 가 다른 곳을 가리키는 프로젝트에서 엉뚱한 곳에 쓴다.
            format!(
                "kang 심볼 인덱스를 읽지 못했습니다 ({}) — {오류}.\n\
                 \n  fix:\n    [shell] kang index {}\n",
                경로.display(),
                셸_인용(&경로.display().to_string())
            )
        })
}

/// 인덱스 경로를 정한다.
///
/// `KANG_INDEX` 가 있으면 그것이고, 없으면 `CARGO_MANIFEST_DIR` 부터 위로 훑어 처음
/// 만나는 `.kang/index.tsv` 다. 매크로는 워크스페이스 루트를 모르므로 위로 훑는다.
///
/// # 반환값
/// 인덱스 파일 경로
///
/// # 오류
/// 어느 쪽으로도 찾지 못한 사정
fn 경로() -> Result<PathBuf, String> {
    // 환경 변수가 있으면 그것이 답이다. 파일이 없으면 읽기가 그 사정을 낸다 —
    // 여기서 존재를 확인해 관례 경로로 넘어가면 "지정한 곳과 다른 곳을 읽었다" 가 된다.
    if let Some(값) = std::env::var_os("KANG_INDEX") {
        return Ok(PathBuf::from(값));
    }

    // cargo 없이 rustc 를 직접 부르면 이 변수가 없다. 그때는 훑을 기준점이 없다.
    let 기준 = std::env::var_os("CARGO_MANIFEST_DIR").ok_or_else(안내)?;

    Path::new(&기준)
        .ancestors()
        .map(|디렉토리| 디렉토리.join(관례_경로))
        .find(|후보| 후보.is_file())
        .ok_or_else(안내)
}

/// 인덱스 **경로를 정할 수 없을** 때 내는 안내.
///
/// 읽기 실패와 다른 사실이다 — 이쪽은 어느 파일을 읽어야 하는지조차 모른다. 그래서
/// 처방의 경로가 관례 경로이고, 그것을 프로젝트 루트에서 실행하라고 말한다.
///
/// # 반환값
/// 무엇이 없고 어떻게 만드는지 담은 문장
fn 안내() -> String {
    format!(
        "kang 심볼 인덱스를 찾지 못했습니다 — `KANG_INDEX` 가 없고 상위 디렉토리에 `{관례_경로}` 도 없습니다.\n\
         \n  fix:\n    [shell] kang index {}\n  (프로젝트 루트에서 실행하세요)\n",
        셸_인용(관례_경로)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    /// 세 종류가 다 든 인덱스. `exception` 의 rev 가 선언 topic 과 같은 것이 정상이다.
    const 인덱스_예: &str = "keyword\t8f4efc\tdocs/a.결제\n\
                             topic\tcc9e1c\tdocs/a#결제의 기본\n\
                             exception\tcc9e1c\tdocs/a!해외 결제\n";

    /// 진단에 적을 인덱스 경로.
    fn 경로_예() -> PathBuf {
        PathBuf::from("/w/.kang/index.tsv")
    }

    #[test]
    fn 핀이_같으면_통과한다() {
        assert_eq!(
            검사(
                인덱스_예,
                "topic",
                "docs/a#결제의 기본",
                "cc9e1c",
                &경로_예()
            ),
            Ok(())
        );
        assert_eq!(
            검사(인덱스_예, "keyword", "docs/a.결제", "8f4efc", &경로_예()),
            Ok(())
        );
        assert_eq!(
            검사(
                인덱스_예,
                "exception",
                "docs/a!해외 결제",
                "cc9e1c",
                &경로_예()
            ),
            Ok(())
        );
    }

    #[test]
    fn 핀이_다르면_두_핀과_고칠_방법을_말한다() {
        let 오류 = 검사(
            인덱스_예,
            "topic",
            "docs/a#결제의 기본",
            "a3f9c1",
            &경로_예(),
        )
        .expect_err("핀이 다르면 실패해야 한다");
        assert!(
            오류.contains("rev 핀이 대상의 현재 내용과 다릅니다"),
            "{오류}"
        );
        assert!(오류.contains("docs/a#결제의 기본"), "{오류}");
        assert!(오류.contains("핀 a3f9c1, 현재 cc9e1c"), "{오류}");
        assert!(오류.contains("/w/.kang/index.tsv"), "{오류}");
        assert!(
            오류.contains(r#"[edit] 이 속성의 rev = "a3f9c1" 을 rev = "cc9e1c" 으로 바꾸세요"#),
            "{오류}"
        );
    }

    #[test]
    fn 심볼이_없으면_인덱스를_다시_내는_명령을_준다() {
        let 오류 = 검사(인덱스_예, "topic", "docs/a#없는 것", "cc9e1c", &경로_예())
            .expect_err("없는 심볼은 실패해야 한다");
        assert!(오류.contains("kang 심볼이 인덱스에 없습니다"), "{오류}");
        assert!(오류.contains("docs/a#없는 것"), "{오류}");
        assert!(
            오류.contains("[shell] kang index '/w/.kang/index.tsv'"),
            "{오류}"
        );
    }

    #[test]
    fn 종류만_다르면_실제_종류와_고친_속성을_준다() {
        let 오류 = 검사(인덱스_예, "topic", "docs/a.결제", "8f4efc", &경로_예())
            .expect_err("종류가 다르면 실패해야 한다");
        assert!(오류.contains("종류가 다릅니다"), "{오류}");
        assert!(오류.contains("topic 가 아니라 keyword 입니다"), "{오류}");
        assert!(
            오류.contains(
                r#"[edit] `#[kang::keyword("docs/a.결제", rev = "8f4efc")]` 로 바꾸세요"#
            ),
            "{오류}"
        );
    }

    #[test]
    fn exception_의_처방은_covers_속성이다() {
        let 오류 = 검사(인덱스_예, "topic", "docs/a!해외 결제", "cc9e1c", &경로_예())
            .expect_err("종류가 다르면 실패해야 한다");
        assert!(오류.contains("`#[kang::covers("), "{오류}");
    }

    #[test]
    fn 이름에_탭이_있어도_주소가_온전하다() {
        // 심볼 이름의 탭은 오늘 합법이다 (V0004 Task 3 J1). 가변 길이 필드가
        // 마지막이라 `splitn(3, '\t')` 가 이름 안의 탭을 살린다.
        let 인덱스 = "keyword\t8f4efc\tdocs/a.앞\t뒤\n";
        assert_eq!(
            검사(인덱스, "keyword", "docs/a.앞\t뒤", "8f4efc", &경로_예()),
            Ok(())
        );
    }

    #[test]
    fn 칸이_모자란_줄은_심볼로_읽지_않는다() {
        assert_eq!(줄들("\n칸하나\nk\tr\n").count(), 0);
    }

    #[test]
    fn 인용부호가_든_경로도_그대로_실행할_수_있다() {
        assert_eq!(셸_인용("a'b"), r"'a'\''b'");
    }

    /// 인자 파서를 함수 포인터로 집는다. `LitStr` 은 `Debug` 를 구현하지 않으므로
    /// (syn 의 `extra-traits` 를 켜지 않았다) 결과는 `map(|_| ())` 로 눌러 본다.
    fn 파서() -> fn(ParseStream) -> syn::Result<(LitStr, LitStr)> {
        인자
    }

    #[test]
    fn 인자를_주소와_핀으로_가른다() {
        let (주소, 핀) = 파서()
            .parse2(quote! { "docs/a#결제의 기본", rev = "cc9e1c" })
            .expect("올바른 인자는 파싱되어야 한다");
        assert_eq!(주소.value(), "docs/a#결제의 기본");
        assert_eq!(핀.value(), "cc9e1c");
    }

    #[test]
    fn rev_가_아닌_키는_거절한다() {
        let 오류 = 파서()
            .parse2(quote! { "docs/a.결제", pin = "8f4efc" })
            .map(|_| ())
            .expect_err("`rev` 외의 키는 거절해야 한다");
        assert!(오류.to_string().contains("`rev` 만 받습니다"), "{오류}");
    }

    #[test]
    fn 핀이_없으면_거절한다() {
        assert!(파서().parse2(quote! { "docs/a.결제" }).is_err());
    }

    #[test]
    fn 진단을_붙여도_원본_아이템은_그대로다() {
        // 런타임 비용 0 의 근거. 성공 경로는 `item` 그 자체이고, 토큰이 뒤틀릴 수 있는
        // 유일한 자리가 진단을 붙이는 경로다.
        let 아이템 = quote! { #[derive(Clone)] pub struct S<T: Copy> where T: Send { pub a: T } };
        let 결과 = 붙여(
            syn::Error::new(proc_macro2::Span::call_site(), "x"),
            아이템.clone(),
        );
        assert!(결과.to_string().ends_with(&아이템.to_string()), "{결과}");
    }
}
