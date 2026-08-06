//! 인덱스를 cargo 의 재빌드 추적에 등록하고, 인덱스가 문서보다 낡았는지 본다.
//!
//! **매크로가 파일을 읽는 것은 cargo 에게 보이지 않는다.** 이 스크립트가 없으면 문서를
//! 고치고 `kang index` 를 다시 돌려도 아무것도 재컴파일되지 않아, 낡은 검증 결과가 그대로
//! 통과한다 — 검증이 있는데 거짓인 상태다. V0003 §5 가 `cargo:rerun-if-changed` 를
//! 명시한 이유가 이것이다.
//!
//! 환경 변수 둘도 추적한다. `KANG_REQUIRE_INDEX` 를 켜고 다시 빌드했을 때 캐시에서
//! 통과해 버리면 CI 의 게이트가 아무것도 검사하지 않게 된다.
//!
//! **그리고 인덱스가 문서보다 낡으면 경고한다.** 매크로가 추적하는 것은 인덱스 파일이고
//! `.kang` 문서가 아니다. 그래서 문서만 고치고 `kang index` 를 안 돌리면 빌드가 초록인 채
//! 낡은 핀이 통과한다 — 빌드가 "코드와 문서가 맞다" 고 말하는데 검증하면 거짓이다.
//! 문서 하나라도 인덱스보다 새로우면 그 사실을 말한다.

use std::path::{Path, PathBuf};

/// 추적 지시를 내고 인덱스 신선도를 본다.
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
    let Some(경로) = std::env::var_os("KANG_INDEX") else {
        return;
    };
    let 인덱스 = PathBuf::from(&경로);
    println!("cargo::rerun-if-changed={}", 인덱스.display());

    낡은_인덱스_경고(&인덱스);
}

/// 인덱스보다 새로운 `.kang` 문서가 있으면 경고하고, 문서들을 재빌드 추적에 올린다.
///
/// **에러가 아니라 경고인 이유**는 판정 근거가 mtime 이기 때문이다. `git checkout` 은
/// 파일 시각을 체크아웃 시점으로 새로 찍으므로, 옛 커밋을 꺼내면 문서가 인덱스보다
/// 새로워 보일 수 있다. 그 상태에서 빌드를 막으면 정상 작업을 못 한다. 반대로 조용히
/// 넘기면 빌드가 "검증했다" 고 말하면서 낡은 핀을 통과시킨다 — 그것이 이 저장소가
/// 열세 번 걸린 함정이다. 사실만 말하고 판단을 사람에게 넘긴다.
///
/// 문서를 `rerun-if-changed` 에 올리므로, 문서를 고치면 다음 빌드에서 이 경고가 다시
/// 계산된다. 인덱스만 추적하면 문서를 고쳐도 이 스크립트가 돌지 않는다.
///
/// ponytail: 인덱스가 `<루트>/.kang/index.tsv` 형태라고 가정해 루트를 두 단계 위로
/// 잡는다. `KANG_INDEX` 를 다른 모양으로 두면 문서를 찾지 못해 경고가 조용히 사라진다 —
/// 그때는 경고가 없는 것이 아니라 검사가 없는 것이다. 인덱스 산출 경로가 관례에서
/// 벗어날 수 있게 되면 루트를 별도 변수로 받는다.
///
/// # 매개변수
/// - `인덱스`: 심볼 인덱스 파일 경로
fn 낡은_인덱스_경고(인덱스: &Path) {
    // 인덱스가 없는 경우는 매크로가 이미 다룬다 (없으면 warn, `KANG_REQUIRE_INDEX` 면 에러).
    let Ok(인덱스_시각) = 인덱스.metadata().and_then(|m| m.modified()) else {
        return;
    };

    // `<루트>/.kang/index.tsv` 에서 루트는 두 단계 위다.
    let Some(루트) = 인덱스.parent().and_then(Path::parent) else {
        return;
    };

    let mut 문서들 = Vec::new();
    문서_모으기(루트, &mut 문서들);

    let mut 낡음 = Vec::new();
    // 문서마다 재빌드 추적을 걸고, 인덱스보다 새로운 것을 모은다.
    for 문서 in &문서들 {
        println!("cargo::rerun-if-changed={}", 문서.display());
        // 시각을 못 읽는 문서는 판정하지 않는다. 읽지 못한 것을 "낡았다" 고 말하면
        // 그 문장이 검증되지 않는 사실이 된다.
        if 문서
            .metadata()
            .and_then(|m| m.modified())
            .is_ok_and(|문서_시각| 문서_시각 > 인덱스_시각)
        {
            낡음.push(문서.display().to_string());
        }
    }

    if !낡음.is_empty() {
        낡음.sort();
        println!(
            "cargo::warning=kang 인덱스가 문서보다 낡았습니다 ({}). \
             이 빌드의 kang 속성은 옛 내용으로 검증됩니다 — kang index {} 를 다시 돌리세요.",
            낡음.join(", "),
            인덱스.display()
        );
    }
}

/// 루트 아래의 `.kang` 파일을 모은다. 숨은 디렉토리는 파고들지 않는다.
///
/// 컴파일러의 순회(`resolve::수집`)와 달리 `.gitignore` 를 보지 않는다. 여기서 노리는
/// 것은 "인덱스보다 새로운 문서가 있는가" 하나이고, 무시된 디렉토리의 문서는 인덱스에
/// 애초에 없으므로 경고가 붙어도 처방(`kang index`)이 같다.
///
/// # 매개변수
/// - `dir`: 훑을 디렉토리
/// - `모음`: 찾은 경로를 담을 곳
fn 문서_모으기(dir: &Path, 모음: &mut Vec<PathBuf>) {
    let Ok(항목들) = std::fs::read_dir(dir) else {
        return;
    };

    // 디렉토리 항목을 순회하며 하위 디렉토리는 파고들고 `.kang` 파일은 모은다.
    for 항목 in 항목들.flatten() {
        let 이름 = 항목.file_name().to_string_lossy().into_owned();
        // 숨은 항목과 빌드 산출 디렉토리는 문서가 아니다. `target/` 은 파일이 많아
        // 훑는 비용만 든다 — 컴파일러가 같은 이유로 건너뛴다.
        if 이름.starts_with('.') || 이름 == "target" {
            continue;
        }
        let 경로 = 항목.path();
        if 경로.is_dir() {
            문서_모으기(&경로, 모음);
        } else if 경로.extension().is_some_and(|확장자| 확장자 == "kang") {
            모음.push(경로);
        }
    }
}
