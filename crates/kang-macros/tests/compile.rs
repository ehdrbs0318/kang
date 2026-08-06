//! 매크로가 실제 `cargo` 빌드에서 무엇을 하는지 잰다.
//!
//! **컴파일 에러를 내는 것이 정상 동작이라 일반 테스트로는 잴 수 없다.** `trybuild` 는
//! V0001 §10.1 의 허용 의존성 밖이라 쓰지 않고, `crates/kang/tests/cli.rs` 가 바이너리를
//! [`Command`] 로 띄우는 것과 같은 방식으로 `cargo run` 을 띄워 종료 코드와 stderr 를 읽는다.
//!
//! 진단 **문면**의 단위 검증은 `src/lib.rs` 의 `검사` 테스트에 있다. 여기서 재는 것은
//! 문면이 아니라 **그 문면이 rustc 의 에러로 실제로 나가 빌드를 세우는지** 다 — 조용히
//! 통과하는 것이 이 크레이트의 유일한 치명적 실패 양식이다 (V0004 B3).
//!
//! ponytail: 한 임시 프로젝트에서 열한 경우를 이어 잰다 (`cargo` 를 열한 번 띄운다, 약 9초).
//! 경우마다 새 프로젝트를 만들면 `syn` 컴파일 비용을 매번 낸다. 경우가 서로를 오염시키는
//! 것이 보이면 그때 가른다.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// 소스에 처음 적히는 핀. 인덱스와 같으므로 첫 빌드가 통과한다.
const 첫_핀: &str = "cc9e1c";

/// 문서가 바뀐 뒤의 핀. 인덱스만 이 값으로 갈아 불일치를 만든다.
const 새_핀: &str = "7b21e0";

/// keyword 의 핀. 문서 a 의 topic 핀과 갈라 두어 진단이 엉키지 않게 한다.
const 키워드_핀: &str = "8f4efc";

/// 임시 프로젝트를 만들고 그 루트를 돌려준다.
///
/// 매크로 셋을 **함수·구조체·상수·`impl` 블록** 넷에 붙인다. `main` 이 그 넷의 값을
/// 단언하므로, 매크로가 토큰을 건드리면 컴파일이 깨지거나 단언이 깨진다 — `cargo expand`
/// 없이 "원본 아이템을 그대로 반환한다" 를 재는 방법이다.
///
/// 의존성을 `kang` 으로 개명해 선언한다. V0003 §4 가 적는 표기가 `#[kang::topic(...)]`
/// 이고, 소비자가 그 이름을 쓰는 방법이 이것 하나다 (`crates/kang` 은 건드리지 않는다).
///
/// # 반환값
/// 임시 프로젝트 루트
fn 임시_프로젝트() -> PathBuf {
    let 루트 = std::env::temp_dir().join(format!("kang-macros-{}", std::process::id()));
    let _ = fs::remove_dir_all(&루트);
    fs::create_dir_all(루트.join("src")).expect("임시 디렉토리를 만들 수 있어야 한다");

    let 매크로_크레이트 = env!("CARGO_MANIFEST_DIR");
    fs::write(
        루트.join("Cargo.toml"),
        format!(
            "[package]\nname = \"kang-macros-fixture\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\
             \n[dependencies]\nkang = {{ path = {매크로_크레이트:?}, package = \"kang-macros\" }}\n"
        ),
    )
    .expect("Cargo.toml 을 쓸 수 있어야 한다");

    fs::write(
        루트.join("src/main.rs"),
        format!(
            r#"#[kang::topic("docs/a#결제의 기본", rev = "{첫_핀}")]
fn 결제한다() -> u8 {{ 7 }}

#[kang::keyword("docs/a.결제", rev = "{키워드_핀}")]
#[derive(Clone, Debug, PartialEq)]
struct 결제 {{ 금액: u8 }}

#[kang::covers("docs/a!해외 결제", rev = "{첫_핀}")]
const 해외_수수료: u8 = 9;

struct 정산;

#[kang::topic("docs/a#결제의 기본", rev = "{첫_핀}")]
impl 정산 {{
    fn 합계(&self) -> u8 {{ 11 }}
}}

fn main() {{
    assert_eq!(결제한다(), 7);
    assert_eq!(결제 {{ 금액: 3 }}, 결제 {{ 금액: 3 }}.clone());
    assert_eq!(해외_수수료, 9);
    assert_eq!(정산.합계(), 11);
    println!("아이템 넷 모두 원본 동작");
}}
"#
        ),
    )
    .expect("main.rs 를 쓸 수 있어야 한다");

    루트
}

/// 인덱스를 쓴다. 문서 a 의 핀만 인자로 갈린다.
///
/// `exception` 의 핀이 선언 topic 과 같은 것은 스펙 4.8 이 요구하는 모양이다.
///
/// # 매개변수
/// - `경로`: 인덱스 파일 경로
/// - `문서a_핀`: topic 과 exception 에 쓸 핀
/// - `topic_포함`: `false` 면 topic 줄을 빼 "심볼 부재" 를 만든다
fn 쓰기_인덱스(경로: &Path, 문서a_핀: &str, topic_포함: bool) {
    fs::create_dir_all(경로.parent().expect("인덱스에 상위 디렉토리가 있어야 한다"))
        .expect("인덱스 디렉토리를 만들 수 있어야 한다");
    let mut 내용 = format!("keyword\t{키워드_핀}\tdocs/a.결제\n");
    // topic 줄을 빼면 그 topic 을 가리키는 두 속성이 "인덱스에 없습니다" 로 깨진다.
    if topic_포함 {
        내용.push_str(&format!("topic\t{문서a_핀}\tdocs/a#결제의 기본\n"));
    }
    내용.push_str(&format!("exception\t{문서a_핀}\tdocs/a!해외 결제\n"));
    fs::write(경로, 내용).expect("인덱스를 쓸 수 있어야 한다");
}

/// 임시 프로젝트를 빌드해 실행한다.
///
/// # 매개변수
/// - `루트`: 임시 프로젝트 루트
/// - `인덱스`: `Some` 이면 `KANG_INDEX` 로 준다. `None` 이면 관례 경로를 위로 훑게 한다
/// - `필수`: `KANG_REQUIRE_INDEX` 를 켤지
///
/// # 반환값
/// `(종료 코드, stderr)`
fn 빌드(루트: &Path, 인덱스: Option<&Path>, 필수: bool) -> (i32, String) {
    let mut 명령 = Command::new(env!("CARGO"));
    명령
        .arg("run")
        .current_dir(루트)
        // 바깥 `cargo test` 가 잡은 target 디렉토리의 락을 건드리지 않는다.
        .env("CARGO_TARGET_DIR", 루트.join("target"))
        .env_remove("KANG_INDEX")
        .env_remove("KANG_REQUIRE_INDEX");

    // 인덱스를 명시하면 위로 훑기를 건너뛴다.
    if let Some(경로) = 인덱스 {
        명령.env("KANG_INDEX", 경로);
    }
    // 켜면 인덱스 부재가 컴파일 에러로 올라간다 (V0004 B3).
    if 필수 {
        명령.env("KANG_REQUIRE_INDEX", "1");
    }

    let 결과 = 명령.output().expect("cargo 를 실행할 수 있어야 한다");
    (
        결과.status.code().expect("종료 코드가 있어야 한다"),
        String::from_utf8_lossy(&결과.stderr).into_owned(),
    )
}

/// 소스의 핀을 바꾼다. 진단이 준 `[edit]` 처방을 그대로 적용하는 것과 같다.
///
/// # 매개변수
/// - `루트`: 임시 프로젝트 루트
/// - `이전`: 지금 적힌 핀
/// - `다음`: 새 핀
fn 소스_핀_바꾸기(루트: &Path, 이전: &str, 다음: &str) {
    let 파일 = 루트.join("src/main.rs");
    let 내용 = fs::read_to_string(&파일).expect("main.rs 를 읽을 수 있어야 한다");
    fs::write(&파일, 내용.replace(이전, 다음)).expect("main.rs 를 쓸 수 있어야 한다");
}

/// 통과 → 인덱스 변경으로 깨짐 → 처방 적용으로 회복 → 부재 → warn → 강제 에러 → 관례 경로.
///
/// 열한 경우를 한 프로젝트에서 순서대로 잰다. **순서 자체가 증거다** — ②가 캐시에서
/// 통과하면 `build.rs` 의 파일 추적이 없다는 뜻이고, ⑨·⑪이 통과하면 환경 변수 추적이
/// 없다는 뜻이다. 세 지시 전부를 뮤테이션으로 확인했다.
#[test]
fn 인덱스와_어긋난_참조가_빌드를_세운다() {
    let 루트 = 임시_프로젝트();
    let 인덱스 = 루트.join(".kang/index.tsv");

    // ① 인덱스와 맞으면 통과하고, 붙은 아이템 넷이 원본 값을 낸다.
    쓰기_인덱스(&인덱스, 첫_핀, true);
    let (코드, 오류) = 빌드(&루트, Some(&인덱스), false);
    assert_eq!(코드, 0, "맞는 인덱스에서는 통과해야 한다: {오류}");

    // ② 문서가 바뀌면 인덱스의 핀이 갈린다. 재빌드 추적이 없으면 이 빌드가 캐시에서
    //    통과해 버린다 — 그때 "검증이 있는데 거짓" 이 된다.
    쓰기_인덱스(&인덱스, 새_핀, true);
    let (코드, 오류) = 빌드(&루트, Some(&인덱스), false);
    assert_ne!(코드, 0, "인덱스가 바뀌면 재빌드되어 깨져야 한다: {오류}");
    assert!(
        오류.contains("rev 핀이 대상의 현재 내용과 다릅니다"),
        "{오류}"
    );
    assert!(
        오류.contains(&format!(
            r#"rev = "{첫_핀}" 을 rev = "{새_핀}" 으로 바꾸세요"#
        )),
        "{오류}"
    );

    // ③ 그 처방을 그대로 적용하면 낫는다 (V0002 가 세운 fix 계약).
    소스_핀_바꾸기(&루트, 첫_핀, 새_핀);
    let (코드, 오류) = 빌드(&루트, Some(&인덱스), false);
    assert_eq!(코드, 0, "처방대로 고치면 통과해야 한다: {오류}");

    // ④ 가리키는 심볼이 인덱스에 없으면 컴파일 에러이고, 처방은 인덱스 재생성이다.
    쓰기_인덱스(&인덱스, 새_핀, false);
    let (코드, 오류) = 빌드(&루트, Some(&인덱스), false);
    assert_ne!(코드, 0, "없는 심볼은 깨져야 한다: {오류}");
    assert!(오류.contains("kang 심볼이 인덱스에 없습니다"), "{오류}");
    assert!(
        오류.contains(&format!("[shell] kang index '{}'", 인덱스.display())),
        "{오류}"
    );

    // ⑤ 인덱스가 아예 없으면 warn 을 내고 통과한다 — 부트스트랩이 가능해야 한다.
    //
    //    **문면이 참인지도 여기서 잰다.** `KANG_INDEX` 는 설정되어 있고 파일만 없는
    //    상황이므로, "`KANG_INDEX` 가 없다" 고 말하면 진단이 거짓이 된다.
    fs::remove_file(&인덱스).expect("인덱스를 지울 수 있어야 한다");
    let (코드, 오류) = 빌드(&루트, Some(&인덱스), false);
    assert_eq!(코드, 0, "인덱스가 없으면 통과해야 한다: {오류}");
    assert!(
        오류.contains("warning: kang 심볼 인덱스를 읽지 못했습니다"),
        "{오류}"
    );
    assert!(
        !오류.contains("찾지 못했습니다"),
        "KANG_INDEX 가 있는데 없다고 말한다: {오류}"
    );
    assert!(
        오류.contains(&format!("[shell] kang index '{}'", 인덱스.display())),
        "처방이 실제로 읽으려던 경로를 가리켜야 한다: {오류}"
    );
    assert!(
        오류.contains("이 빌드에서 kang 속성은 검증되지 않습니다"),
        "{오류}"
    );

    // ⑥ `KANG_REQUIRE_INDEX` 를 켜면 같은 상황이 컴파일 에러다. 이 빌드가 통과하면
    //    환경 변수 변경이 재빌드를 부르지 않는다는 뜻이다.
    //
    //    **에러 모드에서 "통과합니다" 라고 말하면 거짓이다.** 통과하지 않았다.
    let (코드, 오류) = 빌드(&루트, Some(&인덱스), true);
    assert_ne!(
        코드, 0,
        "KANG_REQUIRE_INDEX 는 부재를 에러로 올려야 한다: {오류}"
    );
    assert!(오류.contains("kang 심볼 인덱스를"), "{오류}");
    assert!(
        !오류.contains("검증되지 않습니다"),
        "에러로 세운 빌드를 통과했다고 말한다: {오류}"
    );
    assert!(
        오류.contains("가 켜져 있어 인덱스 부재가 컴파일 에러입니다"),
        "{오류}"
    );

    // ⑦ `KANG_INDEX` 가 없으면 관례 경로를 위로 훑는다. 틀린 핀을 그 자리에 두어
    //    "찾았다" 를 실패로 증명한다 — 통과로는 찾았는지 알 수 없다.
    쓰기_인덱스(&인덱스, 첫_핀, true);
    let (코드, 오류) = 빌드(&루트, None, false);
    assert_ne!(코드, 0, "관례 경로의 인덱스를 찾아 검증해야 한다: {오류}");
    assert!(
        오류.contains("rev 핀이 대상의 현재 내용과 다릅니다"),
        "{오류}"
    );

    // ⑧ 여기부터 두 경우는 `build.rs` 의 **환경 변수** 추적을 잰다. `KANG_INDEX` 가 없고
    //    인덱스 파일도 없으면 `build.rs` 는 파일 추적 지시를 하나도 내지 않으므로, 다음
    //    빌드를 부르는 것은 환경 변수 변경 하나뿐이다. 먼저 통과 상태를 만든다.
    fs::remove_file(&인덱스).expect("인덱스를 지울 수 있어야 한다");
    let (코드, 오류) = 빌드(&루트, None, false);
    assert_eq!(코드, 0, "인덱스가 없으면 통과해야 한다: {오류}");
    // 이 경우에만 "찾지 못했습니다" 가 참이다 — 경로 자체를 정할 수 없었다.
    assert!(
        오류.contains("kang 심볼 인덱스를 찾지 못했습니다"),
        "{오류}"
    );
    assert!(오류.contains("KANG_INDEX"), "{오류}");

    // ⑨ `KANG_INDEX` 를 틀린 핀 인덱스로 지정한다. 통과 상태에서 환경 변수만 바뀌었다.
    쓰기_인덱스(&인덱스, 첫_핀, true);
    let (코드, 오류) = 빌드(&루트, Some(&인덱스), false);
    assert_ne!(코드, 0, "KANG_INDEX 변경이 재빌드를 불러야 한다: {오류}");

    // ⑩ 다시 통과 상태로 — 인덱스도 없고 `KANG_INDEX` 도 없다.
    fs::remove_file(&인덱스).expect("인덱스를 지울 수 있어야 한다");
    let (코드, 오류) = 빌드(&루트, None, false);
    assert_eq!(코드, 0, "인덱스가 없으면 통과해야 한다: {오류}");

    // ⑪ `KANG_REQUIRE_INDEX` 만 켠다. 이것이 캐시에서 통과하면 CI 의 게이트가 아무것도
    //    검사하지 않는다 — Task 1 의 `KANG_REQUIRE_YAML` 이 같은 함정을 지났다.
    let (코드, 오류) = 빌드(&루트, None, true);
    assert_ne!(
        코드, 0,
        "KANG_REQUIRE_INDEX 변경이 재빌드를 불러야 한다: {오류}"
    );

    let _ = fs::remove_dir_all(&루트);
}
