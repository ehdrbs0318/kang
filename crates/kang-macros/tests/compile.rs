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
/// # 매개변수
/// - `이름`: 테스트를 구분하는 이름. **프로세스 id 만으로는 부족하다** — 같은 실행 안의
///   두 테스트가 같은 디렉토리를 쓰면 서로의 인덱스를 덮어써 진단이 엉킨다
///
/// # 반환값
/// 임시 프로젝트 루트
fn 임시_프로젝트(이름: &str) -> PathBuf {
    let 루트 = std::env::temp_dir().join(format!("kang-macros-{}-{이름}", std::process::id()));
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
    let 루트 = 임시_프로젝트("어긋난_참조");
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

    // ⑧ 여기부터 두 경우는 `build.rs` 의 **환경 변수** 추적을 잰다. 인덱스 파일이 없으면
    //    이 임시 프로젝트에는 추적할 파일이 없으므로, 다음 빌드를 부르는 것은 환경 변수
    //    변경뿐이다. 먼저 통과 상태를 만든다.
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

    // ⑫ `KANG_INDEX` 가 정규 파일이 아닌 것을 가리키면 **열기 전에** 거절한다.
    //
    //    FIFO 를 가리키면 여는 순간 쓰는 쪽을 기다리며 rustc 가 매달리고, `/dev/zero`
    //    를 가리키면 끝없는 0 을 읽어 메모리를 다 쓴다. 그 둘을 테스트로 직접 재면
    //    실패가 "매달림" 이라 테스트 자체가 끝나지 않는다 — 같은 갈래인 디렉토리로
    //    잰다. 걸리는 자리가 `인덱스_읽기` 하나이므로 셋이 같은 판정을 지난다.
    //    **문면까지 재는 이유**는 그냥 읽으면 여기서도 실패하기 때문이다. 그때의
    //    사유는 OS 의 "Is a directory" 이고, 그것은 검사가 있었다는 증거가 아니다.
    let (코드, 오류) = 빌드(&루트, Some(&루트.join(".kang")), false);
    assert_eq!(코드, 0, "인덱스를 못 읽으면 warn 이고 통과다: {오류}");
    assert!(
        오류.contains("정규 파일이 아닙니다"),
        "정규 파일 검사를 거쳐야 한다: {오류}"
    );

    let _ = fs::remove_dir_all(&루트);
}

/// 인덱스가 문서보다 낡으면 빌드가 그 사실을 말한다.
///
/// **매크로가 추적하는 것은 인덱스 파일이고 `.kang` 문서가 아니다.** 그래서 문서만
/// 고치고 `kang index` 를 안 돌리면 빌드가 초록인 채 낡은 핀이 통과한다 — 빌드가
/// "코드와 문서가 맞다" 고 말하는데 검증하면 거짓이다. `build.rs` 가 문서 시각을 보고
/// 그 사실을 경고한다.
///
/// **에러가 아니라 경고인 것이 계약이다.** 판정 근거가 mtime 이고 `git checkout` 은
/// 파일 시각을 새로 찍으므로, 에러로 막으면 옛 커밋을 꺼낸 사람이 빌드를 못 한다.
#[test]
fn 인덱스가_문서보다_낡으면_빌드가_말한다() {
    let 루트 = 임시_프로젝트("낡은_인덱스");
    let 인덱스 = 루트.join(".kang/index.tsv");
    쓰기_인덱스(&인덱스, 첫_핀, true);

    // `build.rs` 가 인덱스에서 두 단계 위를 루트로 잡아 `.kang` 문서를 찾는다.
    let 문서 = 루트.join("docs/a.kang");
    fs::create_dir_all(문서.parent().expect("상위 디렉토리")).expect("docs 를 만들 수 있어야 한다");
    fs::write(
        &문서,
        "---\ndescription: 시험\n---\n\n## 결제의 기본\n\n내용이다.\n",
    )
    .expect("문서를 쓸 수 있어야 한다");

    // 인덱스를 문서보다 새롭게 만든다 — `kang index` 를 돌린 직후의 정상 상태다.
    let 나중 = std::time::SystemTime::now() + std::time::Duration::from_secs(2);
    fs::File::options()
        .write(true)
        .open(&인덱스)
        .expect("인덱스를 열 수 있어야 한다")
        .set_modified(나중)
        .expect("인덱스 시각을 정할 수 있어야 한다");

    let (코드, stderr) = 빌드(&루트, Some(&인덱스), false);
    assert_eq!(코드, 0, "정상 상태는 통과해야 한다: {stderr}");
    assert!(
        !stderr.contains("인덱스가 문서보다 낡았습니다"),
        "인덱스가 새로우면 경고가 없어야 한다: {stderr}"
    );

    // 이제 문서를 인덱스보다 새롭게 만든다 — 문서를 고치고 인덱스를 안 돌린 상태다.
    let 더_나중 = 나중 + std::time::Duration::from_secs(2);
    fs::File::options()
        .append(true)
        .open(&문서)
        .expect("문서를 열 수 있어야 한다")
        .set_modified(더_나중)
        .expect("문서 시각을 정할 수 있어야 한다");

    let (코드, stderr) = 빌드(&루트, Some(&인덱스), false);
    assert_eq!(코드, 0, "경고이므로 빌드는 통과한다: {stderr}");
    assert!(
        stderr.contains("인덱스가 문서보다 낡았습니다"),
        "낡은 인덱스를 말해야 한다: {stderr}"
    );
    assert!(
        stderr.contains("a.kang"),
        "어느 문서가 새로운지 지목해야 한다: {stderr}"
    );
    assert!(
        stderr.contains("kang index"),
        "처방을 담아야 한다: {stderr}"
    );
    let _ = fs::remove_dir_all(&루트);
}

/// `build.rs` 를 그 자리에서 컴파일해 한 번 돌리고 stdout 을 준다.
///
/// **build script 를 `cargo` 로 띄울 수는 없다.** cargo 는 그것을 자기 판단으로 돌리고
/// 출력을 삼키며, 무엇보다 `CARGO_MANIFEST_DIR` 을 `kang-macros` 자신의 디렉토리로
/// 고정한다 — 관례 경로 탐색이 어디서부터 올라가는지 잴 수가 없다. `rustc` 로 직접
/// 컴파일해 환경을 우리가 정해 주면 그 규칙을 그대로 관찰할 수 있다. 외부 의존성이
/// 늘지 않는다 (`build.rs` 는 std 만 쓴다).
///
/// # 매개변수
/// - `작업`: 컴파일 산출물을 둘 디렉토리
/// - `기준`: build script 에 줄 `CARGO_MANIFEST_DIR`
/// - `인덱스`: `Some` 이면 `KANG_INDEX` 로 준다. `None` 이면 관례 경로를 위로 훑게 한다
///
/// # 반환값
/// build script 가 낸 stdout
fn 빌드스크립트(작업: &Path, 기준: &Path, 인덱스: Option<&Path>) -> String {
    let 소스 = Path::new(env!("CARGO_MANIFEST_DIR")).join("build.rs");
    let 바이너리 = 작업.join("build-script");
    let 컴파일 = Command::new("rustc")
        .args(["--edition", "2024"])
        .arg(&소스)
        .arg("-o")
        .arg(&바이너리)
        .output()
        .expect("rustc 를 실행할 수 있어야 한다");
    assert!(
        컴파일.status.success(),
        "build.rs 가 컴파일되어야 한다: {}",
        String::from_utf8_lossy(&컴파일.stderr)
    );

    let mut 명령 = Command::new(&바이너리);
    명령
        .env("CARGO_MANIFEST_DIR", 기준)
        .env_remove("KANG_INDEX")
        .env_remove("KANG_REQUIRE_INDEX");
    // 인덱스를 명시하면 위로 훑기를 건너뛴다.
    if let Some(경로) = 인덱스 {
        명령.env("KANG_INDEX", 경로);
    }

    let 결과 = 명령
        .output()
        .expect("build script 를 실행할 수 있어야 한다");
    assert!(
        결과.status.success(),
        "build script 는 통과해야 한다: {}",
        String::from_utf8_lossy(&결과.stderr)
    );
    String::from_utf8_lossy(&결과.stdout).into_owned()
}

/// build script 시험용 저장소를 만든다. 인덱스와 문서 하나를 관례 자리에 둔다.
///
/// # 매개변수
/// - `이름`: 테스트를 구분하는 이름
/// - `추가_줄`: 인덱스에 덧붙일 줄들 (없으면 빈 문자열)
///
/// # 반환값
/// 만들어진 저장소 루트
fn 시험_저장소(이름: &str, 추가_줄: &str) -> PathBuf {
    let 루트 = std::env::temp_dir().join(format!("kang-macros-bs-{}-{이름}", std::process::id()));
    let _ = fs::remove_dir_all(&루트);

    let 문서 = 루트.join("docs/a.kang");
    fs::create_dir_all(문서.parent().expect("상위 디렉토리")).expect("docs 를 만들 수 있어야 한다");
    fs::write(
        &문서,
        "---\ndescription: 시험\n---\n\n## 결제의 기본\n\n내용이다.\n",
    )
    .expect("문서를 쓸 수 있어야 한다");

    let 인덱스 = 루트.join(".kang/index.tsv");
    fs::create_dir_all(인덱스.parent().expect("상위 디렉토리"))
        .expect("인덱스 디렉토리를 만들 수 있어야 한다");
    fs::write(
        &인덱스,
        format!("topic\t{첫_핀}\tdocs/a#결제의 기본\n{추가_줄}"),
    )
    .expect("인덱스를 쓸 수 있어야 한다");

    루트
}

/// `KANG_INDEX` 없이 관례 경로로 찾은 인덱스도 재빌드 추적에 오른다.
///
/// **이것이 빠지면 검증이 한 번 성공한 뒤 영원히 캐시된다.** 매크로는 `KANG_INDEX` 가
/// 없으면 `CARGO_MANIFEST_DIR` 부터 위로 훑어 `.kang/index.tsv` 를 읽는데, `build.rs` 가
/// 그 파일을 cargo 에게 말하지 않으면 인덱스를 다시 내도 아무것도 재컴파일되지 않는다 —
/// 문서를 고쳐도 낡은 rev 로 통과한다.
#[test]
fn 관례_경로로_찾은_인덱스도_재빌드_추적에_오른다() {
    let 루트 = 시험_저장소("관례_추적", "");
    // 소비자 크레이트는 저장소 안쪽에 있다. 인덱스는 거기서 두 단계 위에 있으므로
    // 위로 훑기가 실제로 돌아야 찾는다.
    let 크레이트 = 루트.join("crates/소비자");
    fs::create_dir_all(&크레이트).expect("크레이트 디렉토리를 만들 수 있어야 한다");

    let stdout = 빌드스크립트(&루트, &크레이트, None);
    let 인덱스 = 루트.join(".kang/index.tsv");
    assert!(
        stdout.contains(&format!("cargo::rerun-if-changed={}", 인덱스.display())),
        "관례 경로로 찾은 인덱스를 추적해야 한다: {stdout}"
    );
    // 문서도 함께 올라야 문서만 고쳤을 때 신선도 판정이 다시 돈다.
    assert!(
        stdout.contains(&format!(
            "cargo::rerun-if-changed={}",
            루트.join("docs/a.kang").display()
        )),
        "문서도 추적해야 한다: {stdout}"
    );

    let _ = fs::remove_dir_all(&루트);
}

/// 인덱스가 가리키는 문서가 사라지면 빌드가 그 사실을 말한다.
///
/// **mtime 판정으로는 삭제와 이름 변경이 잡히지 않는다.** 그 둘은 어떤 생존 문서의
/// 시각도 올리지 않으므로 "인덱스보다 새로운 문서" 가 하나도 없다. 그동안 인덱스에는
/// 사라진 문서의 심볼이 남고 매크로는 그것으로 검증을 통과시킨다.
#[test]
fn 인덱스가_사라진_문서를_가리키면_빌드가_말한다() {
    // `docs/gone.kang` 은 만들지 않는다 — 지우거나 이름을 바꾼 뒤의 인덱스와 같다.
    let 루트 = 시험_저장소("사라진_문서", "topic\tdeadbe\tdocs/gone#없어진 것\n");
    let 인덱스 = 루트.join(".kang/index.tsv");

    let stdout = 빌드스크립트(&루트, &루트, Some(&인덱스));
    assert!(
        stdout.contains("cargo::warning=kang 인덱스가 사라진 문서를 가리킵니다"),
        "사라진 문서를 말해야 한다: {stdout}"
    );
    assert!(
        stdout.contains("gone.kang"),
        "어느 문서가 사라졌는지 지목해야 한다: {stdout}"
    );
    assert!(
        stdout.contains("kang index"),
        "처방을 담아야 한다: {stdout}"
    );
    // 살아 있는 문서를 사라졌다고 말하면 진단이 거짓이 된다.
    assert!(
        !stdout.contains("docs/a.kang)") && !stdout.contains("docs/a.kang,"),
        "살아 있는 문서를 지목하면 안 된다: {stdout}"
    );

    let _ = fs::remove_dir_all(&루트);
}

/// 문서 순회가 심볼릭 링크를 따라가지 않는다.
///
/// `Path::is_dir()` 은 링크를 따라가므로 저장소에 링크 순환이 있으면 순회가 자기
/// 자신으로 되돌아오고, `/` 를 가리키는 링크가 하나 있으면 **모든 `cargo build` 가
/// 저장소 밖을 훑는다.** 컴파일러 본체(`resolve::수집`)가 같은 자리에서 `DirEntry` 의
/// 종류를 보는 이유가 이것이고, 여기도 같게 맞춘다.
#[cfg(unix)]
#[test]
fn 문서_순회가_심볼릭_링크를_따라가지_않는다() {
    let 루트 = 시험_저장소("링크", "");
    // `docs/순환` 이 저장소 루트를 가리킨다. 링크를 따라가면 `docs/순환/docs/순환/…`
    // 으로 자기 자신을 끝없이 되짚는다.
    let 링크 = 루트.join("docs/순환");
    std::os::unix::fs::symlink("..", &링크).expect("링크를 만들 수 있어야 한다");

    let 인덱스 = 루트.join(".kang/index.tsv");
    let stdout = 빌드스크립트(&루트, &루트, Some(&인덱스));

    // 링크 밖의 문서는 그대로 보여야 한다 — 순회 자체가 죽으면 이 단언이 먼저 깨진다.
    assert!(
        stdout.contains(&format!(
            "cargo::rerun-if-changed={}",
            루트.join("docs/a.kang").display()
        )),
        "링크가 아닌 문서는 여전히 보여야 한다: {stdout}"
    );
    // 링크를 통해 닿은 경로가 하나라도 있으면 따라간 것이다.
    assert!(
        !stdout.contains(&링크.display().to_string()),
        "심볼릭 링크를 통해 순회하면 안 된다: {stdout}"
    );

    let _ = fs::remove_dir_all(&루트);
}

/// 파일 이름에 든 개행이 새 cargo 지시가 되면 안 된다.
///
/// cargo 지시는 한 줄에 하나이고 이스케이프 문법이 없다. 값에 개행이 들어가면 그 뒷부분이
/// **새 지시**로 읽히므로, `docs/a\ncargo::rustc-env=X=Y.kang` 이라는 파일 하나로 임의의
/// cargo 지시를 심을 수 있다. `.cargo/config.toml` 이 `KANG_INDEX` 를 심으므로 이
/// 스크립트는 이 저장소의 **모든** `cargo build`·`cargo test` 에서 돈다.
///
/// 컴파일러 쪽은 `K116` 이 같은 것을 막지만(`crates/kang/src/resolve.rs`) build script 는
/// 컴파일러를 거치지 않고 파일 시스템을 직접 훑으므로 그 방어가 닿지 않는다.
#[test]
fn 파일_이름의_개행이_cargo_지시가_되지_못한다() {
    let 루트 = 시험_저장소("지시_주입", "");

    // 이 이름이 그대로 stdout 에 실리면 둘째 줄이 유효한 cargo 지시가 된다.
    let 악성 = 루트.join("docs/a\ncargo::rustc-env=INJECTED=yes.kang");
    fs::write(
        &악성,
        "---\ndescription: 주입 시험\n---\n\n## 첫\n\n본문.\n",
    )
    .expect("개행이 든 이름의 문서를 만들 수 있어야 한다");

    let 인덱스 = 루트.join(".kang/index.tsv");
    let stdout = 빌드스크립트(&루트, &루트, Some(&인덱스));

    // 이것이 이 시험의 전부다 — 어떤 줄도 `cargo::rustc-env=INJECTED` 로 시작하지 않는다.
    let 주입된: Vec<&str> = stdout
        .lines()
        .filter(|줄| 줄.starts_with("cargo::rustc-env=INJECTED"))
        .collect();
    assert!(
        주입된.is_empty(),
        "파일 이름이 cargo 지시가 됐다: {주입된:?}\n전체:\n{stdout}"
    );

    // 조용히 건너뛰지 않는다. 추적이 빠진 문서는 고쳐도 재빌드가 걸리지 않으므로,
    // 그 사실을 말하지 않으면 검증이 낡은 채로 통과한다.
    assert!(
        stdout.contains("제어 문자가 든 경로를 재빌드 추적에서 뺐습니다"),
        "무엇을 뺐는지 말해야 한다: {stdout}"
    );

    // 경로를 이스케이프해서 보여 준다 — 사용자가 어느 파일인지 알아야 고칠 수 있다.
    assert!(
        stdout.contains("docs/a\\ncargo::rustc-env=INJECTED=yes.kang"),
        "문제의 경로를 이스케이프해 보여야 한다: {stdout}"
    );

    let _ = fs::remove_dir_all(&루트);
}
