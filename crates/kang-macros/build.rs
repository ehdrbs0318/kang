//! 인덱스를 cargo 의 재빌드 추적에 등록한다.
//!
//! **매크로가 파일을 읽는 것은 cargo 에게 보이지 않는다.** 이 스크립트가 없으면 문서를
//! 고치고 `kang index` 를 다시 돌려도 아무것도 재컴파일되지 않아, 낡은 검증 결과가 그대로
//! 통과한다 — 검증이 있는데 거짓인 상태다. V0003 §5 가 `cargo:rerun-if-changed` 를
//! 명시한 이유가 이것이다.
//!
//! 환경 변수 둘도 추적한다. `KANG_REQUIRE_INDEX` 를 켜고 다시 빌드했을 때 캐시에서
//! 통과해 버리면 CI 의 게이트가 아무것도 검사하지 않게 된다.

use std::path::Path;

/// 추적 지시를 낸다.
fn main() {
    // 값이 바뀌면 이 스크립트가 다시 돌고, 그 결과로 매크로가 다시 펼쳐진다.
    println!("cargo::rerun-if-env-changed=KANG_INDEX");
    println!("cargo::rerun-if-env-changed=KANG_REQUIRE_INDEX");

    // ponytail: 파일 추적은 `KANG_INDEX` 가 있을 때만 붙는다. 관례 경로
    // (`.kang/index.tsv`) 는 이 스크립트가 알 수 없다 — 의존성의 build script 는 자기를
    // 의존하는 크레이트의 manifest 디렉토리를 받지 못하고, 여기서 위로 훑으면 이
    // 크레이트가 놓인 곳(레지스트리 캐시일 수 있다)을 훑게 된다. 관례 경로를 쓰는
    // 프로젝트는 `.cargo/config.toml` 의 `[env]` 로 `KANG_INDEX` 를 주면 추적이 붙는다:
    //
    //     [env]
    //     KANG_INDEX = { value = ".kang/index.tsv", relative = true }
    if let Some(경로) = std::env::var_os("KANG_INDEX") {
        println!("cargo::rerun-if-changed={}", Path::new(&경로).display());
    }
}
