// `kang` 바이너리를 실제로 실행해 서브커맨드 디스패치·출력·종료 코드를 검증하는 통합 테스트.
//
// 통합 테스트 크레이트는 서로 격리되므로 `tests/check.rs` 의 임시 저장소 헬퍼를 그대로
// 가져다 쓸 수 없다. 격리 전략(프로세스 id + 테스트 이름)만 복제하고, 이 파일이 쓰지 않는
// 헬퍼(문서경로·위치들·코드들 등)는 옮기지 않는다.
//
// **라이브러리를 부르지 않고 바이너리를 부른다.** 진단 함수가 `compile()` 에 연결되지
// 않았을 때 그것을 잡는 것이 이 파일의 목적이므로, 단위 호출로 대신하면 의미가 없다.
use kang::ast::{DocPath, SymbolKind, SymbolRef};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// 테스트 하나가 독점하는 임시 디렉토리를 만든다.
///
/// 경로에 프로세스 id 와 테스트 이름을 함께 넣는다. 테스트 이름이 같은 실행 안의
/// 병렬 실행을, 프로세스 id 가 동시에 두 번 돌리는 경우를 각각 막는다.
///
/// # 매개변수
/// - `이름`: 테스트를 구분하는 이름
///
/// # 반환값
/// 갓 만들어진 빈 디렉토리 경로
fn 임시_루트(이름: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kang-cli-{}-{}", std::process::id(), 이름));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("임시 디렉토리를 만들 수 있어야 한다");
    dir
}

/// 임시 디렉토리를 git 저장소로 만든다.
///
/// # 매개변수
/// - `dir`: 저장소로 만들 디렉토리
fn git_저장소로(dir: &Path) {
    let 결과 = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir)
        .output()
        .expect("git 을 실행할 수 있어야 한다");
    assert!(결과.status.success(), "git init 이 실패했다: {결과:?}");
}

/// 루트 아래 상대 경로에 파일을 쓴다. 중간 디렉토리는 만든다.
///
/// # 매개변수
/// - `root`: 프로젝트 루트
/// - `상대`: 루트 기준 상대 경로
/// - `내용`: 파일 내용
fn 쓰기(root: &Path, 상대: &str, 내용: &str) {
    let path = root.join(상대);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("상위 디렉토리를 만들 수 있어야 한다");
    }
    fs::write(&path, 내용).expect("파일을 쓸 수 있어야 한다");
}

/// 테스트가 끝난 뒤 임시 디렉토리를 지운다.
///
/// # 매개변수
/// - `dir`: 지울 디렉토리
fn 정리(dir: &Path) {
    let _ = fs::remove_dir_all(dir);
}

/// `kang` 바이너리를 주어진 디렉토리에서 실행한다.
///
/// `CARGO_BIN_EXE_kang` 은 cargo 가 통합 테스트에 넣어 주는 환경 변수이므로
/// 새 의존성이 아니다.
///
/// # 매개변수
/// - `cwd`: 실행할 디렉토리
/// - `인자`: 바이너리에 넘길 인자들
///
/// # 반환값
/// `(stdout, stderr, 종료 코드)`
fn 실행(cwd: &Path, 인자: &[&str]) -> (String, String, i32) {
    let 결과 = Command::new(env!("CARGO_BIN_EXE_kang"))
        .args(인자)
        .current_dir(cwd)
        .output()
        .expect("kang 바이너리를 실행할 수 있어야 한다");
    (
        String::from_utf8(결과.stdout).expect("stdout 이 UTF-8 이어야 한다"),
        String::from_utf8(결과.stderr).expect("stderr 이 UTF-8 이어야 한다"),
        결과.status.code().expect("종료 코드가 있어야 한다"),
    )
}

/// 진단이 하나도 없는 최소 프로젝트를 만든다.
///
/// # 매개변수
/// - `root`: 프로젝트 루트
fn 정상_문서(root: &Path) {
    쓰기(
        root,
        "docs/a.kang",
        "---\ndescription: 결제 정책\n---\n\nkeyword `결제`: 대금을 지불하는 행위\n\n## 결제의 방법\n\n사용자는 `결제` 를 한다.\n",
    );
}

/// 스펙 V0001 의 예제 프로젝트를 쓴다 — `docs/A`(결제)·`docs/B`(카드결제)·`docs/C`(무료결제).
///
/// **핀 없이 쓴다.** 스펙 4.8 의 3단계 레시피 1단계가 그 상태이고, 핀을 하드코딩하면
/// 정규화 규칙이 바뀔 때 무관한 이유로 픽스처가 죽는다. 통과 상태가 필요한 시나리오는
/// [`예제_프로젝트_통과`] 가 `kang bless` 를 실제로 돌려 핀을 넣는다.
///
/// **임시 디렉토리에 만든다.** 루트 탐색이 git 저장소를 요구하므로 픽스처도 저장소여야
/// 하는데, `tests/` 안에 두면 이 저장소의 워크트리 안에 중첩 저장소가 생겨 `git status`
/// 와 `.gitignore` 취급이 갈린다. 임시 디렉토리는 그 문제가 없고 격리도 공짜다.
///
/// # 매개변수
/// - `root`: 프로젝트 루트
fn 예제_프로젝트(root: &Path) {
    쓰기(
        root,
        "docs/A.kang",
        "---\ndescription: 결제 정책\n---\n\nkeyword `결제`: 사용자가 상품 대금을 지불하는 행위\nkeyword `청구서`: 결제를 청구하는 문서\nkeyword `결제 내역`: 결제의 기록\n\n## 결제의 구성 요소\n\n`결제` 는 `청구서` 와 `결제 내역` 으로 이루어져 있다.\n\n## 결제의 방식\n\n`결제` 는 즉시 결제와 예약 결제로 나뉜다.\n\n## 결제의 종류\n\n`결제` 는 유료 결제와 무료 결제로 나뉜다.\n",
    );
    쓰기(
        root,
        "docs/B.kang",
        "---\ndescription: 카드 결제 정책\n---\n\nimport `docs`/`A`.`결제`\n\nkeyword `결제수단`: 결제를 실행하는 방법\nkeyword `결제수단`.`카드`: 카드를 사용한 결제\n\n## 카드 결제\n\n`결제` 를 하기 위한 수단 중 `결제수단`.`카드` 에 대한 설명이다.\n\nexception `카드 수단 무료 결제`\n",
    );
    쓰기(
        root,
        "docs/C.kang",
        "---\ndescription: 무료 결제 정책\n---\n\nimport `docs`/`A`.`결제`\nimport `docs`/`B`.`결제수단`.`카드`\nimport `docs`/`B`!`카드 수단 무료 결제`\n\n## 무료결제의 구성요소\n\n무료결제는 0원 `결제` 기록만 남기며 `결제수단`.`카드` 도 같다.\n\ncover `카드 수단 무료 결제`\n",
    );
}

/// 예제 프로젝트를 쓰고 진단이 낸 fix 를 **그대로** 실행해 통과 상태로 만든다.
///
/// # 매개변수
/// - `root`: 프로젝트 루트
fn 예제_프로젝트_통과(root: &Path) {
    예제_프로젝트(root);
    let (_, stderr, 코드) = 실행(root, &["build"]);
    assert_eq!(코드, 1, "핀 없는 예제는 K020 으로 실패해야 한다: {stderr}");
    fix_적용(root, &stderr);
}

/// `K001`(미해결 심볼) 하나가 나는 최소 프로젝트를 만든다.
///
/// # 매개변수
/// - `root`: 프로젝트 루트
fn 에러_문서(root: &Path) {
    쓰기(
        root,
        "docs/a.kang",
        "---\ndescription: 결제 정책\n---\n\n## 정책\n\n여기서 `없는 심볼` 을 쓴다.\n",
    );
}

/// 픽스처의 rev 핀을 컴파일러와 같은 방법으로 계산한다.
///
/// **테스트 대상이 아니라 픽스처 준비다.** import 는 핀이 필수이므로(스펙 4.7) 여러
/// 문서를 엮는 픽스처는 올바른 핀 없이는 컴파일되지 않는데, `kang bless` 는 아직 없다.
/// 값을 하드코딩하면 정규화 규칙이 바뀔 때 `show` 와 무관한 이유로 테스트가 죽는다.
///
/// # 매개변수
/// - `root`: 프로젝트 루트
/// - `조각들`: 대상 문서의 경로 조각들
/// - `kind`: 대상 심볼의 종류
/// - `이름`: 대상 심볼의 전체 이름
///
/// # 반환값
/// 그 심볼의 현재 rev 핀 값
fn 핀(root: &Path, 조각들: &[&str], kind: SymbolKind, 이름: &str) -> String {
    let (project, _) = kang::resolve::load(root);
    let (table, _) = kang::resolve::SymbolTable::build(&project);
    let id = table
        .resolve(&SymbolRef {
            doc: DocPath(조각들.iter().map(|조각| (*조각).to_string()).collect()),
            kind,
            name: vec![이름.to_string()],
        })
        .expect("픽스처의 대상 심볼이 있어야 한다");
    kang::hash::rev(table.hash_source(id))
}

// ---------------------------------------------------------------------------
// build 와 종료 코드
// ---------------------------------------------------------------------------

/// 진단이 없는 프로젝트는 성공이다 (스펙 6절).
#[test]
fn build_는_정상_프로젝트에서_종료코드_0_이다() {
    let root = 임시_루트("build-ok");
    git_저장소로(&root);
    정상_문서(&root);

    let (stdout, stderr, 코드) = 실행(&root, &["build"]);

    assert_eq!(코드, 0, "stderr: {stderr}");
    assert_eq!(stderr, "", "정상 빌드는 진단을 내지 않는다");
    assert_eq!(stdout, "", "build 는 문서를 출력하지 않는다");
    정리(&root);
}

/// error 가 있으면 종료 코드 1 이고 진단이 나와야 한다 (스펙 6절).
#[test]
fn build_는_에러가_있으면_종료코드_1_이다() {
    let root = 임시_루트("build-err");
    git_저장소로(&root);
    에러_문서(&root);

    let (stdout, stderr, 코드) = 실행(&root, &["build"]);

    assert_eq!(코드, 1, "stderr: {stderr}");
    assert!(stderr.contains("K001"), "{stderr}");
    assert_eq!(stdout, "");
    정리(&root);
}

/// `K031` 은 저장소에서 유일한 warn 이다. 종료 코드를 진단 개수로 판정하면
/// 정상 문서가 빌드 실패가 되고 "빌드를 실패시키지 않습니다" 라는 약속이 거짓이 된다.
#[test]
fn 경고만_있으면_종료코드_0_이고_경고는_출력된다() {
    let root = 임시_루트("warn-only");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\n## 정책\n\n본문이다.\n\nexception `해외 결제` pending\n",
    );

    let (_, stderr, 코드) = 실행(&root, &["build"]);

    assert_eq!(코드, 0, "warn 은 빌드를 실패시키지 않는다: {stderr}");
    assert!(stderr.contains("warning[K031]"), "{stderr}");
    정리(&root);
}

/// warn 만 있는 프로젝트도 조회 명령은 정상 동작해야 한다.
#[test]
fn 경고만_있으면_list_가_문서를_출력한다() {
    let root = 임시_루트("warn-list");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\n## 정책\n\n본문이다.\n\nexception `해외 결제` pending\n",
    );

    let (stdout, stderr, 코드) = 실행(&root, &["list"]);

    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(stdout, "docs/a: A\n");
    정리(&root);
}

/// 통과하지 못한 문서는 **어떤** CLI 명령으로도 출력되지 않는다 (스펙 5절).
///
/// 세 조회 함수는 같은 패턴을 손으로 반복한 별개 match 다. `list` 만 못박으면
/// 나머지 둘에서 `compile()` 을 `parse_project()` 로 바꾸거나 `return` 을 빼도 잡히지 않는다.
#[test]
fn 에러가_있으면_조회_명령이_아무것도_출력하지_않는다() {
    let root = 임시_루트("lookup-err");
    git_저장소로(&root);
    에러_문서(&root);

    // 조회 명령 셋을 모두 돈다.
    for 인자 in [
        ["list", ""].as_slice(),
        ["keywords", ""].as_slice(),
        ["refs", "docs/a.결제"].as_slice(),
    ] {
        let 인자: Vec<&str> = 인자.iter().copied().filter(|칸| !칸.is_empty()).collect();
        let (stdout, stderr, 코드) = 실행(&root, &인자);

        assert_eq!(코드, 1, "{인자:?} — stderr: {stderr}");
        assert_eq!(
            stdout, "",
            "{인자:?} — error 가 있으면 문서를 한 줄도 내지 않는다"
        );
        assert!(stderr.contains("K001"), "{인자:?} — {stderr}");
    }
    정리(&root);
}

// ---------------------------------------------------------------------------
// `compile()` 연결 — 진단 함수 일곱이 전부 프로덕션 경로에 걸려 있는가
//
// 각 규칙이 위반되는 최소 프로젝트를 만들고 **바이너리가** 그 진단을 내는지 본다.
// 함수를 직접 부르는 단위 테스트는 연결 누락을 잡지 못한다.
// ---------------------------------------------------------------------------

/// `parse_document` 연결 — frontmatter 가 없으면 `K101`.
#[test]
fn build_가_파싱_오류를_보고한다() {
    let root = 임시_루트("wire-parse");
    git_저장소로(&root);
    쓰기(&root, "docs/a.kang", "frontmatter 가 없는 문서\n");

    let (_, stderr, 코드) = 실행(&root, &["build"]);

    assert_eq!(코드, 1, "{stderr}");
    assert!(stderr.contains("K101"), "{stderr}");
    정리(&root);
}

/// `find_root` 연결 — git 저장소가 아니면 `K050`.
#[test]
fn build_가_git_저장소_아님을_보고한다() {
    let root = 임시_루트("wire-root");

    let (_, stderr, 코드) = 실행(&root, &["build"]);

    assert_eq!(코드, 2, "{stderr}");
    assert!(stderr.contains("K050"), "{stderr}");
    정리(&root);
}

/// `load` 연결 — UTF-8 이 아닌 `.kang` 파일이면 `K051`.
#[test]
fn build_가_읽지_못한_문서를_보고한다() {
    let root = 임시_루트("wire-load");
    git_저장소로(&root);
    fs::create_dir_all(root.join("docs")).expect("디렉토리를 만들 수 있어야 한다");
    fs::write(root.join("docs/a.kang"), [0xff, 0xfe, 0x00]).expect("파일을 쓸 수 있어야 한다");

    let (_, stderr, 코드) = 실행(&root, &["build"]);

    assert_eq!(코드, 1, "{stderr}");
    assert!(stderr.contains("K051"), "{stderr}");
    정리(&root);
}

/// `SymbolTable::build` 연결 — 한 문서가 같은 이름을 두 번 묶으면 `K052`.
#[test]
fn build_가_이름_중복을_보고한다() {
    let root = 임시_루트("wire-table");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nkeyword `금액`: 청구되는 원화 액수\nkeyword `금액`: 환불되는 원화 액수\n",
    );

    let (_, stderr, 코드) = 실행(&root, &["build"]);

    assert_eq!(코드, 1, "{stderr}");
    assert!(stderr.contains("K052"), "{stderr}");
    정리(&root);
}

/// `check_cycles` 연결 — `A → B → A` 는 `K040`.
#[test]
fn build_가_순환을_보고한다() {
    let root = 임시_루트("wire-cycles");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nimport `docs`/`b`.`청구서`\n\nkeyword `결제`: 대금을 지불하는 행위\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: B\n---\n\nimport `docs`/`a`.`결제`\n\nkeyword `청구서`: 청구 내역을 담은 문서\n",
    );

    let (_, stderr, 코드) = 실행(&root, &["build"]);

    assert_eq!(코드, 1, "{stderr}");
    assert!(stderr.contains("K040"), "{stderr}");
    정리(&root);
}

/// `check_symbols` 연결 — 선언되지 않은 백틱 심볼이면 `K001`.
#[test]
fn build_가_미해결_심볼을_보고한다() {
    let root = 임시_루트("wire-symbols");
    git_저장소로(&root);
    에러_문서(&root);

    let (_, stderr, 코드) = 실행(&root, &["build"]);

    assert_eq!(코드, 1, "{stderr}");
    assert!(stderr.contains("K001"), "{stderr}");
    정리(&root);
}

/// `check_exceptions` 연결 — 커버되지 않은 exception 이면 `K030`.
#[test]
fn build_가_커버되지_않은_예외를_보고한다() {
    let root = 임시_루트("wire-exceptions");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\n## 정책\n\n본문이다.\n\nexception `무료 상품`\n",
    );

    let (_, stderr, 코드) = 실행(&root, &["build"]);

    assert_eq!(코드, 1, "{stderr}");
    assert!(stderr.contains("K030"), "{stderr}");
    정리(&root);
}

/// `check_revs` 연결 — rev 핀 없는 import 면 `K020`.
#[test]
fn build_가_핀_없는_import_를_보고한다() {
    let root = 임시_루트("wire-revs");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nkeyword `결제`: 대금을 지불하는 행위\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: B\n---\n\nimport `docs`/`a`.`결제` as `A 결제`\n\n## 카드 결제\n\n`A 결제` 는 카드로도 된다.\n",
    );

    let (_, stderr, 코드) = 실행(&root, &["build"]);

    assert_eq!(코드, 1, "{stderr}");
    assert!(stderr.contains("K020"), "{stderr}");
    정리(&root);
}

// ---------------------------------------------------------------------------
// 목록형 명령 (스펙 6.3)
// ---------------------------------------------------------------------------

/// 경로는 항상 전체 경로이며 계층 축약이 없다. 스코프를 주면 그 아래만 나온다.
#[test]
fn list_가_전체_경로로_출력한다() {
    let root = 임시_루트("list-paths");
    git_저장소로(&root);
    쓰기(&root, "readme.kang", "---\ndescription: 뿌리 문서\n---\n");
    쓰기(&root, "docs/a.kang", "---\ndescription: 결제 정책\n---\n");
    쓰기(
        &root,
        "docs/details/payment.kang",
        "---\ndescription: 결제 상세\n---\n",
    );

    let (전체, stderr, 코드) = 실행(&root, &["list"]);

    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(
        전체,
        "docs/a: 결제 정책\ndocs/details/payment: 결제 상세\nreadme: 뿌리 문서\n"
    );

    let (스코프, stderr, 코드) = 실행(&root, &["list", "docs"]);

    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(
        스코프,
        "docs/a: 결제 정책\ndocs/details/payment: 결제 상세\n"
    );
    정리(&root);
}

/// `keywords` 는 경로 스코프만 지원한다.
#[test]
fn keywords_가_경로_스코프로_필터된다() {
    let root = 임시_루트("keywords-scope");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nkeyword `결제`: 사용자가 상품 대금을 지불하는 행위\nkeyword `결제일`: 실제 대금이 처리되는 날짜\n",
    );
    쓰기(
        &root,
        "other/b.kang",
        "---\ndescription: B\n---\n\nkeyword `배송`: 물건을 보내는 행위\n",
    );

    let (전체, stderr, 코드) = 실행(&root, &["keywords"]);

    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(
        전체,
        "docs/a.결제: 사용자가 상품 대금을 지불하는 행위\ndocs/a.결제일: 실제 대금이 처리되는 날짜\nother/b.배송: 물건을 보내는 행위\n"
    );

    let (스코프, stderr, 코드) = 실행(&root, &["keywords", "docs"]);

    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(
        스코프,
        "docs/a.결제: 사용자가 상품 대금을 지불하는 행위\ndocs/a.결제일: 실제 대금이 처리되는 날짜\n"
    );
    정리(&root);
}

/// 스코프가 아무 문서도 맞히지 못하면 오타 하나가 "이 디렉토리에는 문서가 없다" 는
/// 결론이 된다. 다만 **판정은 맞은 문서 수**여야 한다 — 문서는 있고 키워드만 없는
/// 합법 상태에 거짓 안내가 붙으면 안 된다.
#[test]
fn 맞는_문서가_없는_스코프를_알린다() {
    let root = 임시_루트("empty-scope");
    git_저장소로(&root);
    // 키워드가 없는 문서다. 스코프는 맞지만 `keywords` 는 낼 것이 없다.
    쓰기(&root, "docs/a.kang", "---\ndescription: A\n---\n");

    // 맞은 문서가 0 이면 알린다. 종료 코드는 바꾸지 않는다.
    for 명령 in ["list", "keywords"] {
        let (stdout, stderr, 코드) = 실행(&root, &[명령, "없는경로"]);
        assert_eq!(코드, 0, "{명령} — 필터가 빈 결과를 내는 것은 오류가 아니다");
        assert_eq!(stdout, "", "{명령} — {stdout}");
        assert!(stderr.contains("없는경로"), "{명령} — {stderr}");
    }

    // 문서는 맞았는데 낼 키워드가 없는 것은 합법 상태다. 알리면 거짓이 된다.
    let (stdout, stderr, 코드) = 실행(&root, &["keywords", "docs"]);
    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(stdout, "");
    assert_eq!(stderr, "", "문서가 맞았으므로 스코프는 빗나가지 않았다");
    정리(&root);
}

/// 키워드를 참조하는 topic 을 전체 경로로 낸다 (스펙 6.5).
/// alias 로 참조한 다른 문서의 topic 도 같은 심볼을 가리키므로 함께 나와야 한다.
#[test]
fn refs_가_키워드를_참조하는_topic_을_출력한다() {
    let root = 임시_루트("refs");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nkeyword `결제`: 대금을 지불하는 행위\n\n## 결제의 방식\n\n사용자는 `결제` 를 한다.\n\n## 무관한 이야기\n\n여기에는 심볼이 없다.\n",
    );
    // 핀이 어긋나면 `K021` 로 빌드가 실패하므로 대상 정의 텍스트로 직접 계산한다.
    let 핀 = kang::hash::rev("대금을 지불하는 행위");
    쓰기(
        &root,
        "docs/b.kang",
        &format!(
            "---\ndescription: B\n---\n\nimport `docs`/`a`.`결제` as `A 결제` rev \"{핀}\"\n\n## 카드 결제\n\n`A 결제` 는 카드로도 된다.\n"
        ),
    );

    let (stdout, stderr, 코드) = 실행(&root, &["refs", "docs/a.결제"]);

    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(stdout, "docs/a#결제의 방식\ndocs/b#카드 결제\n");
    정리(&root);
}

/// 인자에 백틱을 쓰지 않는다 (스펙 6.0). `/` 와 `.` 만으로 중첩 경로와 계층 키워드를
/// 가리킬 수 있어야 한다.
///
/// 계층 키워드는 **전체 이름 하나**를 가리킨다. `` `결제수단`.`카드` `` 를 쓴 topic 은
/// `결제수단` 을 참조한 것이 아니므로 상위로 조회하면 나오지 않아야 한다.
#[test]
fn 백틱_없는_인자를_파싱한다() {
    let root = 임시_루트("no-backtick-args");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/details/pay.kang",
        "---\ndescription: 결제 상세\n---\n\nkeyword `결제수단`: 대금을 내는 방법\nkeyword `결제수단`.`카드`: 카드를 사용한 결제\n\n## 카드 결제\n\n`결제수단`.`카드` 로 낸다.\n",
    );

    let (하위, stderr, 코드) = 실행(&root, &["refs", "docs/details/pay.결제수단.카드"]);

    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(하위, "docs/details/pay#카드 결제\n");

    let (상위, stderr, 코드) = 실행(&root, &["refs", "docs/details/pay.결제수단"]);

    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(상위, "", "계층 이름의 조각은 따로 참조된 것이 아니다");
    정리(&root);
}

/// 진단을 내는 층과 조회 층이 같은 문장을 다르게 읽으면, 빌드가 통과하는 문서에서
/// 조회가 있는 참조를 놓치고 없는 참조를 만든다. 둘 다 성공 종료라 신호가 없다.
///
/// 이 픽스처의 스코프는 `결제`·`결제.수단`·`수단`·`수단.카드` 이고 본문은
/// `` `결제`.`수단`.`카드` `` 다. 전부 해석되는 분할은 `결제` + `수단`.`카드` 하나뿐이며
/// 왼쪽부터 탐욕으로 끊으면 `결제`.`수단` 을 잡아 `카드` 를 고아로 만든다.
/// 계층 선언이 상위를 요구하므로(스펙 4.3) 넷이 한 스코프에 있는 것은 합법이다.
#[test]
fn refs_가_진단_층과_같은_분할로_참조를_읽는다() {
    let root = 임시_루트("refs-split");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nkeyword `결제`: 대금을 지불하는 행위\nkeyword `결제`.`수단`: 대금을 내는 방법\nkeyword `수단`: 무언가를 이루는 방법\nkeyword `수단`.`카드`: 카드를 쓰는 방법\n\n## 문장\n\n`결제` 는 `수단`.`카드` 로 한다.\n",
    );

    // 합법 문서여야 이 시나리오가 성립한다. 빌드가 실패하면 분할 이전에 틀린 것이다.
    let (_, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(코드, 0, "{stderr}");

    // 분할이 낸 두 이름은 나와야 한다.
    let (stdout, stderr, 코드) = 실행(&root, &["refs", "docs/a.결제"]);
    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(stdout, "docs/a#문장\n", "있는 참조를 놓치면 안 된다");

    let (stdout, stderr, 코드) = 실행(&root, &["refs", "docs/a.수단.카드"]);
    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(stdout, "docs/a#문장\n", "있는 참조를 놓치면 안 된다");

    // 분할이 내지 않은 이름은 나오면 안 된다.
    let (stdout, stderr, 코드) = 실행(&root, &["refs", "docs/a.결제.수단"]);
    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(stdout, "", "없는 참조를 만들면 안 된다");
    정리(&root);
}

/// 인자가 가리키는 키워드가 없으면 빈 결과와 구분되어야 한다.
/// 명령줄의 **모양**은 맞으므로 `--help` 는 내지 않는다.
#[test]
fn refs_는_없는_키워드에_help_없이_종료코드_2_다() {
    let root = 임시_루트("refs-unknown");
    git_저장소로(&root);
    정상_문서(&root);

    let (stdout, stderr, 코드) = 실행(&root, &["refs", "docs/a.없는키워드"]);

    assert_eq!(코드, 2, "{stderr}");
    assert_eq!(stdout, "");
    assert!(
        !stderr.contains("종료 코드"),
        "사용법을 내면 안 된다: {stderr}"
    );
    정리(&root);
}

/// 주소의 **모양**이 틀린 것은 사용법 오류다. 도움말이 첫 접점이라는 규약이
/// 이 분기에서만 깨지면 안 된다.
#[test]
fn refs_는_주소_모양이_틀리면_사용법을_출력한다() {
    let root = 임시_루트("refs-shape");
    git_저장소로(&root);
    정상_문서(&root);

    let (도움말, _, _) = 실행(&root, &["--help"]);
    let (stdout, stderr, 코드) = 실행(&root, &["refs", "docsA"]);

    assert_eq!(코드, 2, "{stderr}");
    assert_eq!(stdout, "");
    assert!(stderr.contains(&도움말), "{stderr}");
    정리(&root);
}

// ---------------------------------------------------------------------------
// 신뢰 경계 — 인자와 출력이 프로세스 밖에서 오는 자리
// ---------------------------------------------------------------------------

/// `kang list | head -20` 은 에이전트의 관용구다. 읽는 쪽이 먼저 끝나면 `println!` 이
/// **패닉**해 종료 코드 101 과 Rust 런타임 트레이스가 나온다 — 문서화된 종료 코드 밖이고,
/// 코드도 `fix` 도 없는 글이 진단 채널에 섞인다.
///
/// 출력이 파이프 버퍼(64KB)를 넘겨야 재현되므로 description 을 길게 잡는다.
#[test]
fn 읽는_쪽이_파이프를_닫아도_패닉하지_않는다() {
    let root = 임시_루트("broken-pipe");
    git_저장소로(&root);
    // 한 글자가 3바이트이므로 문서 하나가 90KB 다. 두 개면 버퍼를 확실히 넘긴다.
    let 긴_설명 = "가".repeat(30_000);
    쓰기(
        &root,
        "docs/a.kang",
        &format!("---\ndescription: {긴_설명}\n---\n"),
    );
    쓰기(
        &root,
        "docs/b.kang",
        &format!("---\ndescription: {긴_설명}\n---\n"),
    );

    let mut child = Command::new(env!("CARGO_BIN_EXE_kang"))
        .arg("list")
        .current_dir(&root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("kang 바이너리를 실행할 수 있어야 한다");
    // 읽는 쪽이 아무것도 읽지 않고 닫는다 — `| head -0` 과 같은 상황이다.
    drop(child.stdout.take());
    let 결과 = child
        .wait_with_output()
        .expect("자식을 기다릴 수 있어야 한다");
    let stderr = String::from_utf8_lossy(&결과.stderr);

    assert_eq!(결과.status.code(), Some(0), "stderr: {stderr}");
    assert!(!stderr.contains("panicked"), "{stderr}");
    정리(&root);
}

/// `std::env::args()` 는 잘못된 유니코드에 패닉한다. 인자 파싱은 도구의 최외곽
/// 신뢰 경계이고 그 위에 아무것도 없다.
#[cfg(unix)]
#[test]
fn 비_utf8_인자는_사용법_오류로_끝난다() {
    use std::os::unix::ffi::OsStrExt;

    let root = 임시_루트("bad-utf8-arg");
    git_저장소로(&root);
    정상_문서(&root);

    let 결과 = Command::new(env!("CARGO_BIN_EXE_kang"))
        .arg("refs")
        .arg(std::ffi::OsStr::from_bytes(&[0xff, 0xfe]))
        .current_dir(&root)
        .output()
        .expect("kang 바이너리를 실행할 수 있어야 한다");
    let stderr = String::from_utf8_lossy(&결과.stderr);

    assert_eq!(결과.status.code(), Some(2), "stderr: {stderr}");
    assert!(!stderr.contains("panicked"), "{stderr}");
    // 사용법 오류이므로 도움말이 함께 나온다.
    assert!(stderr.contains("종료 코드"), "{stderr}");
    정리(&root);
}

// ---------------------------------------------------------------------------
// 사용법·환경 오류·v2
// ---------------------------------------------------------------------------

/// `--help` 는 에이전트의 첫 접점이다. 명령·인자 형식·종료 코드를 전부 보여야 한다.
#[test]
fn help_이_명령과_인자_형식과_종료코드를_전부_보여준다() {
    let root = 임시_루트("help");
    git_저장소로(&root);

    let (stdout, stderr, 코드) = 실행(&root, &["--help"]);

    assert_eq!(코드, 0, "{stderr}");
    // 명령 여덟 가지가 전부 보여야 재시도가 성공한다.
    for 명령 in [
        "kang init",
        "kang build",
        "kang bless",
        "kang list",
        "kang keywords",
        "kang refs",
        "kang show",
        "kang inspect",
    ] {
        assert!(stdout.contains(명령), "{명령} 이 없다: {stdout}");
    }
    // 인자 형식 — 백틱 금지와 셸 인용.
    assert!(stdout.contains("백틱"), "{stdout}");
    assert!(stdout.contains("kang refs docs/A.결제"), "{stdout}");
    // 종료 코드 네 가지. 숫자만 세면 도움말이 `0 1 2 3` 으로 퇴화해도 통과하고,
    // `"2"` 는 본문의 `v2` 만으로도 만족된다. 설명까지 함께 본다.
    assert!(stdout.contains("종료 코드"), "{stdout}");
    for 칸 in ["0  성공", "1  컴파일", "2  사용법", "3  아직"] {
        assert!(stdout.contains(칸), "종료 코드 칸 {칸} 이 없다: {stdout}");
    }
    // 아직 구현되지 않은 명령을 조건 없이 나열하면, 그것을 치고 종료 코드 3 을 받은
    // 에이전트가 표에 없는 상황을 만나 재시도할 곳이 없다.
    let 미구현_절 = stdout
        .split_once("아직 구현되지 않은 명령")
        .expect("미구현 명령을 따로 알려야 한다")
        .1;
    assert!(
        미구현_절.contains("kang inspect"),
        "kang inspect 가 미구현으로 표시되지 않았다: {stdout}"
    );
    // 구현된 명령이 미구현 목록에 남아 있으면 에이전트가 쓸 수 있는 명령을 쓰지 않는다.
    for 명령 in ["kang show <", "kang bless <", "kang init"] {
        assert!(
            !미구현_절.contains(명령),
            "{명령} 이 아직 미구현으로 표시되어 있다: {stdout}"
        );
    }

    // **양방향 게이트.** 도움말이 미구현이라 한 명령을 실제로 불러 본다. 목록만 검사하면
    // 구현된 명령이 목록에 남아도(`init` 이 그랬다) 도움말이 조용히 거짓이 된다.
    // 절의 끝은 첫 빈 줄이다 — 그 뒤의 `인자 문법` 예시에도 `kang` 줄이 있다.
    let 미구현_명령들: Vec<&str> = 미구현_절
        .lines()
        .skip(1)
        .take_while(|줄| !줄.trim().is_empty())
        .filter_map(|줄| 줄.split_whitespace().nth(1))
        .collect();
    assert!(
        !미구현_명령들.is_empty(),
        "미구현 절을 읽지 못했다: {stdout}"
    );
    // 목록에 오른 명령마다 종료 코드 3 이어야 한다.
    for 명령 in 미구현_명령들 {
        let (_, stderr, 코드) = 실행(&root, &[명령]);
        assert_eq!(코드, 3, "kang {명령} 이 3 이 아니다: {stderr}");
    }
    정리(&root);
}

/// 알 수 없는 명령은 사용법 오류다. 같은 도움말을 내고 종료 코드 2 다.
#[test]
fn 알_수_없는_서브커맨드는_사용법을_출력하고_종료코드_2_다() {
    let root = 임시_루트("unknown-cmd");
    git_저장소로(&root);
    정상_문서(&root);

    let (도움말, _, _) = 실행(&root, &["--help"]);
    let (stdout, stderr, 코드) = 실행(&root, &["이상한명령"]);

    assert_eq!(코드, 2);
    assert_eq!(stdout, "", "사용법은 stderr 로 나간다");
    assert!(stderr.contains("이상한명령"), "{stderr}");
    assert!(
        stderr.contains(&도움말),
        "사용법 오류에도 같은 텍스트여야 한다"
    );
    정리(&root);
}

/// 인자가 부족한 것도 사용법 오류다.
#[test]
fn 인자가_부족하면_사용법을_출력한다() {
    let root = 임시_루트("missing-arg");
    git_저장소로(&root);
    정상_문서(&root);

    let (도움말, _, _) = 실행(&root, &["--help"]);

    // `refs` 는 인자가 필수다.
    let (_, stderr, 코드) = 실행(&root, &["refs"]);
    assert_eq!(코드, 2);
    assert!(stderr.contains(&도움말), "{stderr}");

    // `bless` 는 `--import` 까지 있어야 한다.
    let (_, stderr, 코드) = 실행(&root, &["bless", "docs/a"]);
    assert_eq!(코드, 2);
    assert!(stderr.contains(&도움말), "{stderr}");

    // 인자가 아예 없는 호출도 마찬가지다.
    let (_, stderr, 코드) = 실행(&root, &[]);
    assert_eq!(코드, 2);
    assert!(stderr.contains(&도움말), "{stderr}");
    정리(&root);
}

/// 문서가 하나도 없으면 조용히 성공하지 말고 그렇다고 알린다.
#[test]
fn kang_파일이_0개면_그렇다고_알린다() {
    let root = 임시_루트("empty-project");
    git_저장소로(&root);

    let (stdout, stderr, 코드) = 실행(&root, &["build"]);

    assert_eq!(코드, 0, "문서가 없는 것은 error 가 아니다: {stderr}");
    assert_eq!(stdout, "");
    assert!(stderr.contains(".kang"), "{stderr}");
    assert!(stderr.contains("없습니다"), "{stderr}");
    정리(&root);
}

/// v2 기능은 존재하지 않는 명령처럼 보이면 안 된다 (스펙 6절).
#[test]
fn inspect_는_v2_안내와_함께_종료코드_3_이다() {
    let root = 임시_루트("inspect");
    git_저장소로(&root);
    정상_문서(&root);

    let (stdout, stderr, 코드) = 실행(&root, &["inspect"]);

    assert_eq!(코드, 3);
    assert_eq!(stdout, "");
    assert!(stderr.contains("v2"), "{stderr}");
    정리(&root);
}

/// 환경 오류에는 `--help` 를 내지 않는다. git 저장소가 아닌 것은 명령을 잘못 쓴 게 아니다.
#[test]
fn git_저장소가_아니면_help_대신_git_init_지시만_출력한다() {
    let root = 임시_루트("no-git");

    let (stdout, stderr, 코드) = 실행(&root, &["build"]);

    assert_eq!(코드, 2, "환경 오류는 컴파일 error 와 구분된다: {stderr}");
    assert_eq!(stdout, "");
    assert!(stderr.contains("git init"), "{stderr}");
    assert!(
        !stderr.contains("종료 코드"),
        "사용법을 내면 에이전트가 철자를 의심한다: {stderr}"
    );
    정리(&root);
}

// ---------------------------------------------------------------------------
// show — 평탄화된 완결 뷰 (스펙 6.4)
// ---------------------------------------------------------------------------

/// 스펙 6.4: 문서 뷰는 정의 키워드와 그 키워드를 참조하는 topic 을 담는다.
/// 자기 문서가 정의한 키워드는 최상위에서 펼치므로, 그것을 가리키는 본문 참조는
/// 경로 하나로 줄어든다.
#[test]
fn show_가_정의_키워드와_참조_topic_을_출력한다() {
    let root = 임시_루트("show-doc");
    git_저장소로(&root);
    정상_문서(&root);

    let (stdout, stderr, 코드) = 실행(&root, &["show", "docs/a"]);

    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(
        stdout,
        r#"path: docs/a
keywords:
  - name: 결제
    path: docs/a
    description: 대금을 지불하는 행위
    referencedBy:
      - docs/a#결제의 방법
topics:
  - name: 결제의 방법
    uncoded: false
    topic: |2-
      ## 결제의 방법

      사용자는 `결제` 를 한다.
    references:
      keywords:
        - docs/a.결제
"#
    );
    정리(&root);
}

/// 스펙 6.4: 정의한 예외에는 그것을 커버하는 topic 의 본문이, 커버하는 예외에는
/// 그 예외가 선언된 topic 의 본문이 임베드된다. 두 방향 모두 원본을 열지 않고 읽힌다.
#[test]
fn show_가_예외와_커버_본문을_임베드한다() {
    let root = 임시_루트("show-exception");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\n## 청구서 정책\n\n모든 청구서는 결제에서 나온다.\n\nexception `무료 상품 청구서`\n",
    );
    let 예외_핀 = 핀(
        &root,
        &["docs", "a"],
        SymbolKind::Exception,
        "무료 상품 청구서",
    );
    쓰기(
        &root,
        "docs/c.kang",
        &format!(
            "---\ndescription: C\n---\n\nimport `docs`/`a`!`무료 상품 청구서` as `무료 청구서` rev \"{예외_핀}\"\n\n## 무료결제의 구성요소\n\n무료상품은 청구서 없이 기록만 남긴다.\n\ncover `무료 청구서`\n"
        ),
    );

    // 정의한 쪽 — 예외에 커버 topic 의 주소와 본문이 붙는다.
    let (stdout, stderr, 코드) = 실행(&root, &["show", "docs/a"]);
    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(
        stdout,
        r#"path: docs/a
exceptions:
  - name: 무료 상품 청구서
    pending: false
    coveredBy: docs/c#무료결제의 구성요소
    topic: |2
      ## 무료결제의 구성요소

      무료상품은 청구서 없이 기록만 남긴다.
topics:
  - name: 청구서 정책
    uncoded: false
    topic: |2
      ## 청구서 정책

      모든 청구서는 결제에서 나온다.
"#
    );

    // 커버하는 쪽 — 예외가 선언된 topic 의 본문이 맥락으로 붙는다 (스펙 4.8).
    let (stdout, stderr, 코드) = 실행(&root, &["show", "docs/c"]);
    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(
        stdout,
        r#"path: docs/c
covers:
  - name: 무료 상품 청구서
    path: docs/a
    topic: |2
      ## 청구서 정책

      모든 청구서는 결제에서 나온다.
topics:
  - name: 무료결제의 구성요소
    uncoded: false
    topic: |2
      ## 무료결제의 구성요소

      무료상품은 청구서 없이 기록만 남긴다.
"#
    );
    정리(&root);
}

/// 스펙 6.4: 참조한 topic 은 재귀적으로 임베드된다. 사슬이 길어도 링크를 따라갈
/// 필요가 없어야 `show` 가 `cat` 보다 쓸모 있다.
#[test]
fn show_가_참조_topic_을_재귀적으로_임베드한다() {
    let root = 임시_루트("show-recursive");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/base.kang",
        "---\ndescription: 기초\n---\n\n## 기초 정책\n\n모든 정책의 바탕이다.\n",
    );
    let 기초_핀 = 핀(&root, &["docs", "base"], SymbolKind::Topic, "기초 정책");
    쓰기(
        &root,
        "docs/mid.kang",
        &format!(
            "---\ndescription: 중간\n---\n\nimport `docs`/`base`#`기초 정책` as `기초` rev \"{기초_핀}\"\n\n## 중간 정책\n\n`기초` 를 따른다.\n"
        ),
    );
    let 중간_핀 = 핀(&root, &["docs", "mid"], SymbolKind::Topic, "중간 정책");
    쓰기(
        &root,
        "docs/top.kang",
        &format!(
            "---\ndescription: 꼭대기\n---\n\nimport `docs`/`mid`#`중간 정책` as `중간` rev \"{중간_핀}\"\n\n## 꼭대기 정책\n\n`중간` 을 따른다.\n"
        ),
    );

    let (stdout, stderr, 코드) = 실행(&root, &["show", "docs/top"]);

    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(
        stdout,
        r#"path: docs/top
topics:
  - name: 꼭대기 정책
    uncoded: false
    topic: |2-
      ## 꼭대기 정책

      `중간` 을 따른다.
    references:
      topics:
        - name: docs/mid#중간 정책
          uncoded: false
          topic: |2-
            ## 중간 정책

            `기초` 를 따른다.
          references:
            topics:
              - name: docs/base#기초 정책
                uncoded: false
                topic: |2-
                  ## 기초 정책

                  모든 정책의 바탕이다.
"#
    );
    정리(&root);
}

/// 스펙 6.4: 같은 topic 이 여러 경로로 도달되면 최초 1회만 전개하고 이후에는
/// 경로 참조로 대체한다. 다이아몬드에서 본문이 두 번 나오면 출력이 지수로 커진다.
#[test]
fn 다이아몬드_의존에서_같은_topic_이_한_번만_전개된다() {
    let root = 임시_루트("show-diamond");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/base.kang",
        "---\ndescription: 기초\n---\n\n## 기초 정책\n\n모든 정책의 바탕이다.\n",
    );
    let 기초_핀 = 핀(&root, &["docs", "base"], SymbolKind::Topic, "기초 정책");
    for (파일, 이름, 서술) in [
        ("docs/left.kang", "왼쪽 정책", "왼쪽에서"),
        ("docs/right.kang", "오른쪽 정책", "오른쪽에서"),
    ] {
        쓰기(
            &root,
            파일,
            &format!(
                "---\ndescription: {이름}\n---\n\nimport `docs`/`base`#`기초 정책` as `기초` rev \"{기초_핀}\"\n\n## {이름}\n\n`기초` 를 {서술} 따른다.\n"
            ),
        );
    }
    let 왼쪽_핀 = 핀(&root, &["docs", "left"], SymbolKind::Topic, "왼쪽 정책");
    let 오른쪽_핀 = 핀(&root, &["docs", "right"], SymbolKind::Topic, "오른쪽 정책");
    쓰기(
        &root,
        "docs/top.kang",
        &format!(
            "---\ndescription: 꼭대기\n---\n\nimport `docs`/`left`#`왼쪽 정책` as `왼쪽` rev \"{왼쪽_핀}\"\nimport `docs`/`right`#`오른쪽 정책` as `오른쪽` rev \"{오른쪽_핀}\"\n\n## 꼭대기 정책\n\n`왼쪽` 과 `오른쪽` 을 합친다.\n"
        ),
    );

    let (stdout, stderr, 코드) = 실행(&root, &["show", "docs/top"]);

    assert_eq!(코드, 0, "{stderr}");
    // 두 갈래 모두 기초 정책에 닿지만 본문은 한 번만 나온다.
    assert_eq!(
        stdout.matches("## 기초 정책").count(),
        1,
        "본문이 두 번 전개되었다: {stdout}"
    );
    assert_eq!(
        stdout.matches("모든 정책의 바탕이다.").count(),
        1,
        "본문이 두 번 전개되었다: {stdout}"
    );
    // 두 번째 도달은 경로 문자열 하나로 대체된다.
    assert_eq!(
        stdout.matches("- docs/base#기초 정책\n").count(),
        1,
        "경로 참조가 없다: {stdout}"
    );
    // 왼쪽·오른쪽은 각각 한 번씩 전개된다 — 중복 제거가 과하게 걸리면 안 된다.
    assert_eq!(stdout.matches("## 왼쪽 정책").count(), 1, "{stdout}");
    assert_eq!(stdout.matches("## 오른쪽 정책").count(), 1, "{stdout}");
    정리(&root);
}

/// topic 뷰는 그 topic 하나로 좁힌 뷰다. 문서 뷰와 달리 자기 문서의 키워드를
/// 최상위에 펼치지 않으므로, 참조한 키워드가 참조 자리에서 전개된다.
#[test]
fn show_가_topic_하나로_좁힌_뷰를_출력한다() {
    let root = 임시_루트("show-topic");
    git_저장소로(&root);
    정상_문서(&root);

    let (stdout, stderr, 코드) = 실행(&root, &["show", "docs/a#결제의 방법"]);

    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(
        stdout,
        r#"path: docs/a#결제의 방법
topics:
  - name: 결제의 방법
    uncoded: false
    topic: |2-
      ## 결제의 방법

      사용자는 `결제` 를 한다.
    references:
      keywords:
        - name: 결제
          path: docs/a
          description: 대금을 지불하는 행위
          referencedBy:
            - docs/a#결제의 방법
"#
    );
    정리(&root);
}

/// keyword 의 `#` 상세 topic 은 파싱만 하고 버리지 않는다. 그 topic 의 전체 경로를
/// `detail` 로 담아야 조회한 쪽이 이어서 읽을 수 있다.
#[test]
fn show_가_키워드의_상세_topic_경로를_담는다() {
    let root = 임시_루트("show-detail");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nkeyword `결제`: 대금을 지불하는 행위 #`결제의 상세`\n\n## 결제의 상세\n\n자세한 설명이다.\n",
    );

    let (stdout, stderr, 코드) = 실행(&root, &["show", "docs/a"]);

    assert_eq!(코드, 0, "{stderr}");
    assert!(
        stdout.contains("    detail: docs/a#결제의 상세\n"),
        "상세 topic 경로가 없다: {stdout}"
    );
    정리(&root);
}

/// error 가 있으면 아무것도 출력하지 않는다 (스펙 5·6절). 통과하지 못한 문서는
/// 어떤 CLI 명령으로도 출력되지 않는다.
#[test]
fn show_는_에러가_있으면_아무것도_출력하지_않는다() {
    let root = 임시_루트("show-err");
    git_저장소로(&root);
    에러_문서(&root);

    let (stdout, stderr, 코드) = 실행(&root, &["show", "docs/a"]);

    assert_eq!(코드, 1, "stderr: {stderr}");
    assert_eq!(stdout, "", "error 상태에서 표준 출력은 비어 있어야 한다");
    assert!(stderr.contains("K001"), "{stderr}");
    정리(&root);
}

/// 없는 주소를 빈 출력으로 돌려주면 "그런 문서가 없다" 와 "내용이 없다" 를 구분할 수 없다.
#[test]
fn show_는_없는_문서와_topic_을_거절한다() {
    let root = 임시_루트("show-missing");
    git_저장소로(&root);
    정상_문서(&root);

    let (stdout, stderr, 코드) = 실행(&root, &["show", "docs/zzz"]);
    assert_eq!(코드, 2, "{stderr}");
    assert_eq!(stdout, "");
    assert!(stderr.contains("docs/zzz"), "{stderr}");

    let (stdout, stderr, 코드) = 실행(&root, &["show", "docs/a#없는 토픽"]);
    assert_eq!(코드, 2, "{stderr}");
    assert_eq!(stdout, "");
    assert!(stderr.contains("없는 토픽"), "{stderr}");
    정리(&root);
}

/// 자기 자신을 참조하는 topic 이 무한 재귀가 되면 안 된다. 방문 집합이 유일한
/// 방어선이다 — 같은 파일 안의 참조는 import 간선을 만들지 않아 순환 검사가 보지 않는다.
#[test]
fn show_는_자기_자신을_참조하는_topic_에서_멈춘다() {
    let root = 임시_루트("show-self");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\n## 되돌이 정책\n\n이 정책은 `되돌이 정책` 자신을 가리킨다.\n",
    );

    let (stdout, stderr, 코드) = 실행(&root, &["show", "docs/a#되돌이 정책"]);

    assert_eq!(코드, 0, "{stderr}");
    // 자기 자신은 이미 전개되었으므로 경로 하나로 대체된다.
    assert_eq!(stdout.matches("## 되돌이 정책").count(), 1, "{stdout}");
    assert!(stdout.contains("- docs/a#되돌이 정책\n"), "{stdout}");
    정리(&root);
}

/// `kang show ... | head` 는 에이전트의 관용구다. 읽는 쪽이 먼저 끝나도 패닉하면 안 된다.
#[test]
fn show_는_읽는_쪽이_파이프를_닫아도_패닉하지_않는다() {
    let root = 임시_루트("show-broken-pipe");
    git_저장소로(&root);
    // 한 글자가 3바이트이므로 본문 하나가 90KB 다. 파이프 버퍼(64KB)를 넘긴다.
    let 긴_본문 = "가".repeat(30_000);
    쓰기(
        &root,
        "docs/a.kang",
        &format!("---\ndescription: A\n---\n\n## 긴 정책\n\n{긴_본문}\n"),
    );

    let mut child = Command::new(env!("CARGO_BIN_EXE_kang"))
        .args(["show", "docs/a"])
        .current_dir(&root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("kang 바이너리를 실행할 수 있어야 한다");
    drop(child.stdout.take());
    let 결과 = child
        .wait_with_output()
        .expect("자식을 기다릴 수 있어야 한다");
    let stderr = String::from_utf8_lossy(&결과.stderr);

    assert_eq!(결과.status.code(), Some(0), "stderr: {stderr}");
    assert!(!stderr.contains("panicked"), "{stderr}");
    정리(&root);
}

/// 계층 keyword 참조는 **한 이름**으로 묶여야 한다 (스펙 4.3).
///
/// 파서는 `` `결제수단`.`카드` `` 를 두 조각으로 넣으므로, 조각을 그대로 조회하면
/// 참조가 상위 키워드에 붙고 하위 키워드는 아무도 참조하지 않는 것이 된다.
/// 진단을 내는 층과 **같은 분할 함수**를 쓰는지가 여기서 갈린다 — 빌드가 통과하는
/// 문서에서 조회만 조용히 틀리므로 신호가 없다.
#[test]
fn show_가_계층_키워드_참조를_한_이름으로_묶는다() {
    let root = 임시_루트("show-hierarchy");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nkeyword `결제수단`: 대금을 내는 방법\nkeyword `결제수단`.`카드`: 카드를 사용한 결제\n\n## 카드 결제\n\n`결제수단`.`카드` 로 낸다.\n",
    );

    let (stdout, stderr, 코드) = 실행(&root, &["show", "docs/a"]);

    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(
        stdout,
        r#"path: docs/a
keywords:
  - name: 결제수단
    path: docs/a
    description: 대금을 내는 방법
  - name: 결제수단.카드
    path: docs/a
    description: 카드를 사용한 결제
    referencedBy:
      - docs/a#카드 결제
topics:
  - name: 카드 결제
    uncoded: false
    topic: |2-
      ## 카드 결제

      `결제수단`.`카드` 로 낸다.
    references:
      keywords:
        - docs/a.결제수단.카드
"#
    );
    정리(&root);
}

// ---------------------------------------------------------------------------
// bless
// ---------------------------------------------------------------------------

/// 루트 아래 상대 경로의 파일을 읽는다.
///
/// `bless` 는 kang 에서 유일하게 사용자 파일을 고쳐 쓰는 명령이므로, 검증은
/// 종료 코드가 아니라 **파일 바이트**로 해야 한다.
///
/// # 매개변수
/// - `root`: 프로젝트 루트
/// - `상대`: 루트 기준 상대 경로
///
/// # 반환값
/// 파일 내용 전체
fn 읽기(root: &Path, 상대: &str) -> String {
    fs::read_to_string(root.join(상대)).expect("파일을 읽을 수 있어야 한다")
}

/// 핀이 없는 import 하나를 가진 두 문서짜리 프로젝트를 만든다.
///
/// 스펙 4.8 의 3단계 레시피 1단계 — "핀 없이 쓴다" 상태다.
///
/// # 매개변수
/// - `root`: 프로젝트 루트
fn 핀_없는_프로젝트(root: &Path) {
    쓰기(
        root,
        "docs/base.kang",
        "---\ndescription: 기초\n---\n\n## 기초 정책\n\n모든 정책의 바탕이다.\n",
    );
    쓰기(
        root,
        "docs/top.kang",
        "---\ndescription: 꼭대기\n---\n\nimport `docs`/`base`#`기초 정책` as `기초`\n\n## 꼭대기 정책\n\n`기초` 를 따른다.\n",
    );
}

/// 스펙 4.8: 핀이 없으면 삽입한다. 더미 해시를 손으로 적는 의식이 생기지 않는다.
#[test]
fn bless_가_핀이_없는_import_에_핀을_삽입한다() {
    let root = 임시_루트("bless-insert");
    git_저장소로(&root);
    핀_없는_프로젝트(&root);

    let (stdout, stderr, 코드) = 실행(
        &root,
        &["bless", "docs/top", "--import", "docs/base#기초 정책"],
    );

    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(stdout, "", "bless 는 표준 출력에 데이터를 내지 않는다");
    let 기초_핀 = 핀(&root, &["docs", "base"], SymbolKind::Topic, "기초 정책");
    assert_eq!(
        읽기(&root, "docs/top.kang"),
        format!(
            "---\ndescription: 꼭대기\n---\n\nimport `docs`/`base`#`기초 정책` as `기초` rev \"{기초_핀}\"\n\n## 꼭대기 정책\n\n`기초` 를 따른다.\n"
        )
    );
    정리(&root);
}

/// 스펙 4.8: 핀이 있으면 현재 해시로 갱신한다. 삽입과 치환은 다른 편집이다.
#[test]
fn bless_가_틀린_핀을_현재_해시로_갱신한다() {
    let root = 임시_루트("bless-update");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/base.kang",
        "---\ndescription: 기초\n---\n\n## 기초 정책\n\n모든 정책의 바탕이다.\n",
    );
    쓰기(
        &root,
        "docs/top.kang",
        "---\ndescription: 꼭대기\n---\n\nimport `docs`/`base`#`기초 정책` as `기초` rev \"000000\"\n\n## 꼭대기 정책\n\n`기초` 를 따른다.\n",
    );

    let (_, stderr, 코드) = 실행(
        &root,
        &["bless", "docs/top", "--import", "docs/base#기초 정책"],
    );

    assert_eq!(코드, 0, "{stderr}");
    let 기초_핀 = 핀(&root, &["docs", "base"], SymbolKind::Topic, "기초 정책");
    assert_ne!(
        기초_핀, "000000",
        "픽스처의 옛 핀이 현재 해시와 같으면 안 된다"
    );
    assert_eq!(
        읽기(&root, "docs/top.kang"),
        format!(
            "---\ndescription: 꼭대기\n---\n\nimport `docs`/`base`#`기초 정책` as `기초` rev \"{기초_핀}\"\n\n## 꼭대기 정책\n\n`기초` 를 따른다.\n"
        )
    );
    정리(&root);
}

/// **이 태스크의 핵심 계약.** bless 가 넣은 핀을 `check_revs` 가 곧바로 거부하면
/// 핀을 붙일 방법이 아예 없어진다. fix 를 적용하면 새 진단이 생기면 안 된다 —
/// 스펙 5.1.1 `:261` 의 "그대로 적용 가능한 `fix`" 가 근거다. (이 주석은 `:417` 을 인용하고
/// 있었는데 그 줄은 빈 줄이고 스펙에 그 문장은 없다. 거짓 인용이 V0004 브리프까지 전파됐다.)
#[test]
fn bless_후_build_가_통과한다() {
    let root = 임시_루트("bless-build-ok");
    git_저장소로(&root);
    핀_없는_프로젝트(&root);

    let (_, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(코드, 1, "{stderr}");
    assert!(stderr.contains("K020"), "{stderr}");

    let (_, stderr, 코드) = 실행(
        &root,
        &["bless", "docs/top", "--import", "docs/base#기초 정책"],
    );
    assert_eq!(코드, 0, "{stderr}");

    let (stdout, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(stderr, "", "핀을 붙였으면 진단이 남지 않아야 한다");
    assert_eq!(stdout, "");
    정리(&root);
}

/// 진단의 `[shell] fix` 가 만드는 주소와 `bless` 가 받는 주소가 다르면, 에이전트가
/// 복사해 실행한 명령이 통째로 실패한다. 문자열을 그대로 셸에 넣어 확인한다.
#[test]
fn build_이_낸_fix_명령을_그대로_실행하면_통과한다() {
    let root = 임시_루트("bless-fix-roundtrip");
    git_저장소로(&root);
    핀_없는_프로젝트(&root);

    let (_, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(코드, 1, "{stderr}");
    assert!(
        셸_fix(&stderr)[0].starts_with("kang bless "),
        "렌더된 fix 는 명령으로 시작해야 한다: {stderr}"
    );
    fix_적용(&root, &stderr);

    let (_, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(코드, 0, "{stderr}");
    정리(&root);
}

/// 스펙 6.2: 대상이 바뀌면 `K021` 이 나고, 다시 `bless` 하면 해소된다.
#[test]
fn bless_가_대상이_바뀐_뒤의_핀_불일치를_해소한다() {
    let root = 임시_루트("bless-k021");
    git_저장소로(&root);
    핀_없는_프로젝트(&root);
    실행(
        &root,
        &["bless", "docs/top", "--import", "docs/base#기초 정책"],
    );

    // 대상 본문을 고친다. 참조처가 깨지는 것이 rev 핀의 목적이다.
    쓰기(
        &root,
        "docs/base.kang",
        "---\ndescription: 기초\n---\n\n## 기초 정책\n\n모든 정책의 바탕이며 이제 한 줄이 늘었다.\n",
    );
    let (_, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(코드, 1, "{stderr}");
    assert!(stderr.contains("K021"), "{stderr}");

    let (_, stderr, 코드) = 실행(
        &root,
        &["bless", "docs/top", "--import", "docs/base#기초 정책"],
    );
    assert_eq!(코드, 0, "{stderr}");

    let (_, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(코드, 0, "{stderr}");
    정리(&root);
}

/// 두 번 돌려도 두 번째는 아무것도 바꾸지 않아야 한다. 실패한 bless 를 다시 돌리는
/// 것이 정상 복구 경로이므로 멱등하지 않으면 재실행이 위험해진다.
#[test]
fn bless_는_멱등하다() {
    let root = 임시_루트("bless-idempotent");
    git_저장소로(&root);
    핀_없는_프로젝트(&root);

    실행(
        &root,
        &["bless", "docs/top", "--import", "docs/base#기초 정책"],
    );
    let 첫_번째 = 읽기(&root, "docs/top.kang");
    let 첫_mtime = fs::metadata(root.join("docs/top.kang"))
        .and_then(|메타| 메타.modified())
        .expect("mtime 을 읽을 수 있어야 한다");

    let (_, stderr, 코드) = 실행(
        &root,
        &["bless", "docs/top", "--import", "docs/base#기초 정책"],
    );

    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(읽기(&root, "docs/top.kang"), 첫_번째);
    // 바이트만 보면 "다시 써서 같은 내용이 되었다" 와 구분되지 않는다. 두 번째 bless 는
    // 아예 쓰지 않아야 한다.
    assert_eq!(
        fs::metadata(root.join("docs/top.kang"))
            .and_then(|메타| 메타.modified())
            .expect("mtime 을 읽을 수 있어야 한다"),
        첫_mtime,
        "바꿀 것이 없는데 파일을 다시 썼다"
    );
    정리(&root);
}

/// **원문 보존은 신뢰 경계다.** 핀만 갈아 끼우고 나머지 바이트는 건드리지 않는다 —
/// 줄 끝(`\r\n`), 파일 끝 개행 유무, 들여쓰기, 줄 끝 공백 전부. 문서를 파싱해서
/// 다시 직렬화하면 이 중 하나는 반드시 깨진다.
#[test]
fn bless_가_핀_외의_바이트를_보존한다() {
    let root = 임시_루트("bless-bytes");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/base.kang",
        "---\ndescription: 기초\n---\n\n## 기초 정책\n\n모든 정책의 바탕이다.\n",
    );
    // CRLF 줄 끝 + 들여쓴 import + 줄 끝 공백 + 파일 끝 개행 없음.
    쓰기(
        &root,
        "docs/top.kang",
        "---\r\ndescription: 꼭대기\r\n---\r\n\r\n  import `docs`/`base`#`기초 정책` as `기초`  \r\n\r\n## 꼭대기 정책\r\n\r\n`기초` 를 따른다.",
    );

    let (_, stderr, 코드) = 실행(
        &root,
        &["bless", "docs/top", "--import", "docs/base#기초 정책"],
    );

    assert_eq!(코드, 0, "{stderr}");
    let 기초_핀 = 핀(&root, &["docs", "base"], SymbolKind::Topic, "기초 정책");
    assert_eq!(
        읽기(&root, "docs/top.kang"),
        format!(
            "---\r\ndescription: 꼭대기\r\n---\r\n\r\n  import `docs`/`base`#`기초 정책` as `기초` rev \"{기초_핀}\"  \r\n\r\n## 꼭대기 정책\r\n\r\n`기초` 를 따른다."
        )
    );
    정리(&root);
}

/// 주소는 심볼이다 (ADR-0003). 문서를 고쳐 줄이 밀려도 같은 주소가 같은 import 를
/// 가리켜야 하고, 지정하지 않은 import 는 한 글자도 바뀌면 안 된다.
#[test]
fn bless_가_줄이_밀려도_지정한_import_만_고친다() {
    let root = 임시_루트("bless-shifted");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/base.kang",
        "---\ndescription: 기초\n---\n\nkeyword `결제`: 대금을 지불하는 행위\n\n## 기초 정책\n\n모든 정책의 바탕이다.\n",
    );
    // 두 import 사이에 줄이 끼어 뒤쪽 import 의 줄 번호가 밀린 상태다.
    쓰기(
        &root,
        "docs/top.kang",
        "---\ndescription: 꼭대기\n---\n\nimport `docs`/`base`.`결제` as `결제`\n\nimport `docs`/`base`#`기초 정책` as `기초`\n\n## 꼭대기 정책\n\n`결제` 와 `기초` 를 따른다.\n",
    );

    let (_, stderr, 코드) = 실행(
        &root,
        &["bless", "docs/top", "--import", "docs/base#기초 정책"],
    );

    assert_eq!(코드, 0, "{stderr}");
    let 기초_핀 = 핀(&root, &["docs", "base"], SymbolKind::Topic, "기초 정책");
    assert_eq!(
        읽기(&root, "docs/top.kang"),
        format!(
            "---\ndescription: 꼭대기\n---\n\nimport `docs`/`base`.`결제` as `결제`\n\nimport `docs`/`base`#`기초 정책` as `기초` rev \"{기초_핀}\"\n\n## 꼭대기 정책\n\n`결제` 와 `기초` 를 따른다.\n"
        )
    );
    정리(&root);
}

/// 스펙 6.0: 인자에 백틱을 쓰지 않는다. keyword 는 `.`, topic 은 `#`, exception 은 `!` 다.
/// exception 의 핀은 **그것을 선언한 topic 의 해시**와 같아야 한다 (스펙 4.8).
#[test]
fn bless_가_백틱_없는_세_종류_주소를_받는다() {
    let root = 임시_루트("bless-three-kinds");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/base.kang",
        "---\ndescription: 기초\n---\n\nkeyword `결제`: 대금을 지불하는 행위\n\n## 기초 정책\n\n모든 정책의 바탕이다.\n\nexception `무료 상품`\n",
    );
    쓰기(
        &root,
        "docs/top.kang",
        "---\ndescription: 꼭대기\n---\n\nimport `docs`/`base`.`결제` as `결제`\nimport `docs`/`base`#`기초 정책` as `기초`\nimport `docs`/`base`!`무료 상품` as `무료`\n\n## 꼭대기 정책\n\n`결제` 와 `기초` 를 따르고 `무료` 를 다룬다.\n\ncover `무료`\n",
    );

    // 세 주소를 차례로 축복한다. 셸 인용이 필요한 공백 있는 이름을 포함한다.
    for 주소 in [
        "docs/base.결제",
        "docs/base#기초 정책",
        "docs/base!무료 상품",
    ] {
        let (_, stderr, 코드) = 실행(&root, &["bless", "docs/top", "--import", 주소]);
        assert_eq!(코드, 0, "{주소}: {stderr}");
    }

    let 결제_핀 = 핀(&root, &["docs", "base"], SymbolKind::Keyword, "결제");
    let 기초_핀 = 핀(&root, &["docs", "base"], SymbolKind::Topic, "기초 정책");
    let 무료_핀 = 핀(&root, &["docs", "base"], SymbolKind::Exception, "무료 상품");
    assert_eq!(무료_핀, 기초_핀, "exception 은 선언 topic 의 해시를 쓴다");
    assert_eq!(
        읽기(&root, "docs/top.kang"),
        format!(
            "---\ndescription: 꼭대기\n---\n\nimport `docs`/`base`.`결제` as `결제` rev \"{결제_핀}\"\nimport `docs`/`base`#`기초 정책` as `기초` rev \"{기초_핀}\"\nimport `docs`/`base`!`무료 상품` as `무료` rev \"{무료_핀}\"\n\n## 꼭대기 정책\n\n`결제` 와 `기초` 를 따르고 `무료` 를 다룬다.\n\ncover `무료`\n"
        )
    );

    let (_, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(코드, 0, "{stderr}");
    정리(&root);
}

/// 그 문서에 없는 import 를 조용히 성공으로 돌려주면, 에이전트는 핀을 붙였다고 믿고
/// 같은 error 를 다시 만난다.
///
/// **대상 심볼은 실재한다.** 없는 심볼로 물으면 그쪽 분기가 먼저 걸려 이 규칙이
/// 검증되지 않는다.
#[test]
fn bless_가_그_문서에_없는_import_를_거부한다() {
    let root = 임시_루트("bless-no-import");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/base.kang",
        "---\ndescription: 기초\n---\n\nkeyword `결제`: 대금을 지불하는 행위\n\n## 기초 정책\n\n모든 정책의 바탕이다.\n",
    );
    // `기초 정책` 만 import 한다. `결제` 는 실재하는 심볼이지만 이 문서가 들여오지 않았다.
    쓰기(
        &root,
        "docs/top.kang",
        "---\ndescription: 꼭대기\n---\n\nimport `docs`/`base`#`기초 정책` as `기초`\n\n## 꼭대기 정책\n\n`기초` 를 따른다.\n",
    );
    let 원본 = 읽기(&root, "docs/top.kang");

    let (stdout, stderr, 코드) =
        실행(&root, &["bless", "docs/top", "--import", "docs/base.결제"]);

    assert_eq!(코드, 2, "{stderr}");
    assert_eq!(stdout, "");
    assert!(stderr.contains("docs/base.결제"), "{stderr}");
    assert!(
        stderr.contains("이 import 가 없습니다"),
        "대상 없음이 아니라 import 없음이어야 한다: {stderr}"
    );
    assert_eq!(읽기(&root, "docs/top.kang"), 원본, "거부했으면 쓰지 않는다");
    정리(&root);
}

/// 대상 심볼이 없으면 해시할 것이 없다. 없는 핀을 지어내면 `K021` 이 영원히 남는다.
#[test]
fn bless_가_대상_심볼이_없는_import_를_거부한다() {
    let root = 임시_루트("bless-no-target");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/base.kang",
        "---\ndescription: 기초\n---\n\n## 기초 정책\n\n모든 정책의 바탕이다.\n",
    );
    쓰기(
        &root,
        "docs/top.kang",
        "---\ndescription: 꼭대기\n---\n\nimport `docs`/`base`#`없는 정책` as `없음`\n\n## 꼭대기 정책\n\n`없음` 을 따른다.\n",
    );
    let 원본 = 읽기(&root, "docs/top.kang");

    let (stdout, stderr, 코드) = 실행(
        &root,
        &["bless", "docs/top", "--import", "docs/base#없는 정책"],
    );

    assert_eq!(코드, 2, "{stderr}");
    assert_eq!(stdout, "");
    assert_eq!(읽기(&root, "docs/top.kang"), 원본, "거부했으면 쓰지 않는다");
    정리(&root);
}

/// 없는 문서를 조용히 넘기면 오타 하나가 "핀을 붙였다" 는 거짓 확신이 된다.
#[test]
fn bless_가_없는_문서를_거부한다() {
    let root = 임시_루트("bless-no-doc");
    git_저장소로(&root);
    핀_없는_프로젝트(&root);

    let (stdout, stderr, 코드) = 실행(
        &root,
        &["bless", "docs/zzz", "--import", "docs/base#기초 정책"],
    );

    assert_eq!(코드, 2, "{stderr}");
    assert_eq!(stdout, "");
    assert!(stderr.contains("docs/zzz"), "{stderr}");
    정리(&root);
}

/// 인자의 **모양**이 틀린 것은 사용법 오류다. 도움말이 에이전트의 첫 접점이라는
/// 규약이 이 분기에서만 깨지면 안 된다.
#[test]
fn bless_가_구분자_없는_주소를_사용법_오류로_거부한다() {
    let root = 임시_루트("bless-bad-addr");
    git_저장소로(&root);
    핀_없는_프로젝트(&root);

    let (도움말, _, _) = 실행(&root, &["--help"]);
    let (stdout, stderr, 코드) = 실행(&root, &["bless", "docs/top", "--import", "docs/base"]);

    assert_eq!(코드, 2, "{stderr}");
    assert_eq!(stdout, "");
    assert!(stderr.contains(&도움말), "{stderr}");
    정리(&root);
}

/// **bless 는 error 상태에서 실행되어야 한다.** 핀 없음도 error, 핀 불일치도 error 이므로
/// `compile()` 통과를 요구하면 영원히 실행될 수 없다. 개념 이름을 바꾸는 정상 워크플로
/// (스펙 6.1) 는 여러 문서가 동시에 깨진 상태를 지난다.
#[test]
fn bless_가_다른_진단이_있는_상태에서도_실행된다() {
    let root = 임시_루트("bless-with-errors");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/base.kang",
        "---\ndescription: 기초\n---\n\n## 기초 정책\n\n모든 정책의 바탕이다.\n",
    );
    // `없는 심볼` 이 `K001` 을, 핀 없는 import 가 `K020` 을 낸다.
    쓰기(
        &root,
        "docs/top.kang",
        "---\ndescription: 꼭대기\n---\n\nimport `docs`/`base`#`기초 정책` as `기초`\n\n## 꼭대기 정책\n\n`기초` 와 `없는 심볼` 을 따른다.\n",
    );

    let (_, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(코드, 1, "{stderr}");
    assert!(stderr.contains("K001"), "{stderr}");

    let (_, stderr, 코드) = 실행(
        &root,
        &["bless", "docs/top", "--import", "docs/base#기초 정책"],
    );

    assert_eq!(코드, 0, "{stderr}");
    let 기초_핀 = 핀(&root, &["docs", "base"], SymbolKind::Topic, "기초 정책");
    assert!(
        읽기(&root, "docs/top.kang").contains(&format!("rev \"{기초_핀}\"")),
        "핀이 삽입되지 않았다"
    );
    정리(&root);
}

/// **파싱이 실패하면 고쳐 쓰지 않는다.** 읽지 못한 문서에 수정을 얹으면 사용자는
/// 깨진 프로젝트에 편집까지 더해진 상태를 받는다.
#[test]
fn bless_가_파싱_실패_상태에서는_실행되지_않는다() {
    let root = 임시_루트("bless-parse-fail");
    git_저장소로(&root);
    핀_없는_프로젝트(&root);
    // frontmatter 가 없는 문서는 파싱 자체가 실패한다.
    쓰기(&root, "docs/broken.kang", "## 깨진 문서\n\n내용.\n");
    let 원본 = 읽기(&root, "docs/top.kang");

    let (stdout, stderr, 코드) = 실행(
        &root,
        &["bless", "docs/top", "--import", "docs/base#기초 정책"],
    );

    assert_eq!(코드, 1, "{stderr}");
    assert_eq!(stdout, "");
    assert_eq!(
        읽기(&root, "docs/top.kang"),
        원본,
        "파싱이 실패했으면 한 바이트도 쓰지 않는다"
    );
    정리(&root);
}

/// 진단이 낸 `[shell] fix` 줄을 전부 뽑는다. **접두 `[shell] ` 만 뗀다.**
///
/// 명령이 시작하는 자리를 찾아 앞을 잘라내면 **그 잘라내기가 테스트를 대신한다** —
/// `action` 이 한글 산문으로 시작해 복사해 실행할 수 없는 결함이 통째로 가려진다.
/// 스펙 5.1.1 과 6.1 이 요구하는 것은 명령만 있는 줄이므로, 테스트도 접두만 떼야 한다.
///
/// # 매개변수
/// - `stderr`: `kang build` 의 진단 출력
///
/// # 반환값
/// 렌더된 순서대로의 셸 명령들
fn 셸_fix(stderr: &str) -> Vec<String> {
    stderr
        .lines()
        .filter_map(|줄| 줄.trim_start().strip_prefix("[shell] "))
        .map(str::to_string)
        .collect()
}

/// `[shell] fix` 줄을 전부 셸에 **그대로** 넣어 실행하고 각각이 성공하는지 본다.
///
/// # 매개변수
/// - `root`: 명령을 실행할 프로젝트 루트
/// - `stderr`: `kang build` 의 진단 출력
///
/// # 반환값
/// 실행한 명령의 개수
fn fix_적용(root: &Path, stderr: &str) -> usize {
    let 명령들 = 셸_fix(stderr);
    assert!(!명령들.is_empty(), "진단에 셸 fix 가 있어야 한다: {stderr}");
    // 렌더된 순서대로 하나씩 실행한다. `fixes` 는 순서 있는 목록이다 (스펙 5.1.1).
    for 명령 in &명령들 {
        let 결과 = Command::new("sh")
            .arg("-c")
            .arg(명령)
            .current_dir(root)
            .env("PATH", 바이너리_경로())
            .output()
            .expect("sh 를 실행할 수 있어야 한다");
        assert_eq!(
            결과.status.code(),
            Some(0),
            "렌더된 fix 를 그대로 실행하지 못했다: {명령}\n{}",
            String::from_utf8_lossy(&결과.stderr)
        );
    }
    명령들.len()
}

/// 테스트용 `kang` 바이너리가 있는 디렉토리를 `PATH` 로 만든다.
///
/// 진단이 낸 fix 는 `kang` 이라는 이름만 안다. 셸이 그 이름을 찾을 수 있어야
/// "복사해 그대로 실행" 을 실제로 검증할 수 있다.
///
/// # 반환값
/// 바이너리 디렉토리를 앞에 붙인 `PATH` 값
fn 바이너리_경로() -> String {
    let dir = Path::new(env!("CARGO_BIN_EXE_kang"))
        .parent()
        .expect("바이너리에 상위 디렉토리가 있어야 한다")
        .display()
        .to_string();
    match std::env::var("PATH") {
        Ok(기존) => format!("{dir}:{기존}"),
        Err(_) => dir,
    }
}

/// **좌표는 지금 쓰려는 바이트에서 나와야 한다** (ADR-0003).
///
/// `bless` 는 파일을 두 번 읽는다 — 프로젝트를 세울 때 한 번, 고쳐 쓰기 직전에 한 번.
/// 그 사이에 파일이 바뀌면 첫 읽기의 줄 번호가 낡는다. 낡은 좌표가 우연히 다른
/// `import ` 접두 줄(코드펜스 안의 사용자 산문 등)을 가리키면 **거기에 핀이 박히고**
/// bless 는 성공을 알린다. 되돌릴 곳이 없다.
///
/// 이 창은 바이너리로 결정적으로 재현할 수 없다(두 읽기 사이에 끼어들 수 없다).
/// 그래서 라이브러리를 직접 불러 그 사이를 벌린다.
#[test]
fn bless_가_파싱_이후_바뀐_파일에서_옛_줄_번호를_쓰지_않는다() {
    let root = 임시_루트("bless-toctou");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/base.kang",
        "---\ndescription: 기초\n---\n\n## 기초 정책\n\n바탕이다.\n",
    );
    // 진짜 import 는 10행이다.
    쓰기(
        &root,
        "docs/top.kang",
        "---\ndescription: 꼭대기\n---\n\n\n\n\n\n\nimport `docs`/`base`#`기초 정책` as `기초`\n\n## 꼭대기 정책\n\n`기초` 를 따른다.\n",
    );

    let (project, _) = kang::resolve::load(&root);
    let (table, _) = kang::resolve::SymbolTable::build(&project);

    // 여기서 파일이 바뀐다. 진짜 import 는 5행으로 올라오고, **10행은 코드펜스 안의
    // 사용자 산문**이 된다 — 파서가 정상적으로 무시하는, `import ` 로 시작하지만 선언이
    // 아닌 줄이다. 낡은 좌표 10 이 접두 검사만으로는 이 줄을 통과시킨다.
    // 그 줄은 topic 본문이므로 핀이 박히면 **그 topic 의 해시가 바뀌어** 이 topic 을
    // import 한 하위 문서까지 K021 로 무너진다.
    let 바뀐_원문 = "---\ndescription: 꼭대기\n---\n\nimport `docs`/`base`#`기초 정책` as `기초`\n\n## 꼭대기 정책\n\n```\nimport `docs`/`base`#`기초 정책` as `가짜`\n```\n\n`기초` 를 따른다.\n";
    쓰기(&root, "docs/top.kang", 바뀐_원문);

    let addr = kang::bless::ImportAddress::parse("docs/base#기초 정책")
        .expect("주소를 파싱할 수 있어야 한다");
    let 결과 = kang::bless::bless(
        &project,
        &table,
        &root,
        &DocPath(vec!["docs".to_string(), "top".to_string()]),
        &addr,
    );

    // 바뀐 파일에서도 진짜 import 는 5행에 있다. 거부하든 새 좌표로 고치든,
    // **펜스 안의 사용자 산문은 절대 바뀌어서는 안 된다.**
    let 결과_원문 = 읽기(&root, "docs/top.kang");
    assert!(
        결과_원문.contains("import `docs`/`base`#`기초 정책` as `가짜`\n"),
        "펜스 안의 사용자 산문에 핀이 박혔다: {결과_원문}"
    );
    // 그리고 성공을 알렸다면 진짜 import 가 실제로 핀을 받았어야 한다.
    if 결과.is_ok() {
        let 기초_핀 = 핀(&root, &["docs", "base"], SymbolKind::Topic, "기초 정책");
        assert!(
            결과_원문.contains(&format!("as `기초` rev \"{기초_핀}\"")),
            "성공이라 했는데 진짜 import 는 미핀이다: {결과_원문}"
        );
    }
    정리(&root);
}

/// 파일이 바뀌어 import 가 **다른 줄로 옮겨간** 경우, 거부가 아니라 새 좌표로
/// 올바르게 고쳐야 한다. 거부만 하면 정상 워크플로(문서를 고친 뒤 bless)가 막힌다.
#[test]
fn bless_가_파싱_이후_옮겨간_import_를_새_좌표에서_찾는다() {
    let root = 임시_루트("bless-toctou-moved");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/base.kang",
        "---\ndescription: 기초\n---\n\n## 기초 정책\n\n바탕이다.\n",
    );
    쓰기(
        &root,
        "docs/top.kang",
        "---\ndescription: 꼭대기\n---\n\nimport `docs`/`base`#`기초 정책` as `기초`\n\n## 꼭대기 정책\n\n`기초` 를 따른다.\n",
    );

    let (project, _) = kang::resolve::load(&root);
    let (table, _) = kang::resolve::SymbolTable::build(&project);

    // frontmatter 에 줄이 늘어 import 가 5행 → 6행으로 밀렸다.
    쓰기(
        &root,
        "docs/top.kang",
        "---\ndescription: 꼭대기\ntags: []\n---\n\nimport `docs`/`base`#`기초 정책` as `기초`\n\n## 꼭대기 정책\n\n`기초` 를 따른다.\n",
    );

    let addr = kang::bless::ImportAddress::parse("docs/base#기초 정책")
        .expect("주소를 파싱할 수 있어야 한다");
    let 결과 = kang::bless::bless(
        &project,
        &table,
        &root,
        &DocPath(vec!["docs".to_string(), "top".to_string()]),
        &addr,
    );

    assert!(결과.is_ok(), "{결과:?}");
    let 기초_핀 = 핀(&root, &["docs", "base"], SymbolKind::Topic, "기초 정책");
    assert_eq!(
        읽기(&root, "docs/top.kang"),
        format!(
            "---\ndescription: 꼭대기\ntags: []\n---\n\nimport `docs`/`base`#`기초 정책` as `기초` rev \"{기초_핀}\"\n\n## 꼭대기 정책\n\n`기초` 를 따른다.\n"
        )
    );
    정리(&root);
}

/// **원자성은 모듈 주석이 계약으로 선언한 신뢰 경계다.** 쓰기가 실패하면 원본은
/// 한 바이트도 바뀌지 않아야 한다. 제자리에서 덮어쓰면 이 단언이 깨진다.
///
/// 문서 디렉토리에서 쓰기 권한을 뺏어 임시 파일 생성을 실패시킨다.
#[test]
#[cfg(unix)]
fn bless_는_쓰기가_실패하면_원본을_건드리지_않는다() {
    use std::os::unix::fs::PermissionsExt;

    let root = 임시_루트("bless-atomic");
    git_저장소로(&root);
    핀_없는_프로젝트(&root);
    let 원본 = 읽기(&root, "docs/top.kang");

    let docs = root.join("docs");
    fs::set_permissions(&docs, fs::Permissions::from_mode(0o555))
        .expect("권한을 바꿀 수 있어야 한다");

    // root 로 실행하면 권한 비트가 무시되어 시나리오 자체가 성립하지 않는다.
    if fs::write(docs.join("probe"), "x").is_ok() {
        let _ = fs::remove_file(docs.join("probe"));
        let _ = fs::set_permissions(&docs, fs::Permissions::from_mode(0o755));
        정리(&root);
        return;
    }

    let (stdout, stderr, 코드) = 실행(
        &root,
        &["bless", "docs/top", "--import", "docs/base#기초 정책"],
    );

    let _ = fs::set_permissions(&docs, fs::Permissions::from_mode(0o755));
    assert_eq!(코드, 2, "{stderr}");
    assert_eq!(stdout, "");
    assert_eq!(
        읽기(&root, "docs/top.kang"),
        원본,
        "쓰기에 실패했는데 원본이 바뀌었다 — 제자리 덮어쓰기다"
    );
    정리(&root);
}

/// 스펙 5.1.1 의 정신: 진단은 검증하면 참인 사실만 말해야 한다. 아무 바이트도 바뀌지
/// 않았는데 "갱신했습니다" 라고 하면 거짓이다.
#[test]
fn bless_가_무변경과_갱신을_구분해_알린다() {
    let root = 임시_루트("bless-said-what");
    git_저장소로(&root);
    핀_없는_프로젝트(&root);

    let (_, 첫_stderr, 코드) = 실행(
        &root,
        &["bless", "docs/top", "--import", "docs/base#기초 정책"],
    );
    assert_eq!(코드, 0, "{첫_stderr}");
    assert!(첫_stderr.contains("갱신"), "{첫_stderr}");

    let (_, 둘째_stderr, 코드) = 실행(
        &root,
        &["bless", "docs/top", "--import", "docs/base#기초 정책"],
    );
    assert_eq!(코드, 0, "{둘째_stderr}");
    assert!(
        !둘째_stderr.contains("갱신했습니다"),
        "아무것도 안 바꿨는데 갱신했다고 말한다: {둘째_stderr}"
    );
    assert!(둘째_stderr.contains("이미"), "{둘째_stderr}");
    정리(&root);
}

/// rename 은 임시 파일의 inode 를 문서 자리에 올리므로, 아무것도 하지 않으면 문서의
/// 파일 mode 가 새 파일의 것으로 갈린다. 문서 권한은 사용자가 정한 것이다.
#[test]
#[cfg(unix)]
fn bless_가_문서의_파일_권한을_보존한다() {
    use std::os::unix::fs::PermissionsExt;

    let root = 임시_루트("bless-mode");
    git_저장소로(&root);
    핀_없는_프로젝트(&root);
    let 문서 = root.join("docs/top.kang");
    fs::set_permissions(&문서, fs::Permissions::from_mode(0o600))
        .expect("권한을 바꿀 수 있어야 한다");

    let (_, stderr, 코드) = 실행(
        &root,
        &["bless", "docs/top", "--import", "docs/base#기초 정책"],
    );

    assert_eq!(코드, 0, "{stderr}");
    let mode = fs::metadata(&문서)
        .expect("문서가 있어야 한다")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600, "파일 mode 가 {mode:o} 로 바뀌었다");
    정리(&root);
}

// ---------------------------------------------------------------------------
// 통합 검증 — 스펙 V0001 의 예제 프로젝트로 전 명령을 왕복한다 (Task 12)
// ---------------------------------------------------------------------------

/// **C1 의 회귀 테스트.** 렌더된 `[shell]` 줄에서 접두 `[shell] ` 만 떼고 나머지를
/// 통째로 셸에 넣는다. `action` 이 한글 산문으로 시작하면 `sh` 가 그 낱말을 명령으로
/// 찾아 127 로 죽는다 (스펙 5.1.1 :261·:306, 6.1 :417).
#[test]
fn build_출력을_bless_에_그대로_넘기면_전부_해제된다() {
    let root = 임시_루트("예제-fix-왕복");
    git_저장소로(&root);
    예제_프로젝트(&root);

    let (_, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(코드, 1, "{stderr}");
    // 핀 없는 import 가 넷이므로 fix 도 넷이다. 하나만 돌면 회귀를 놓친다.
    assert_eq!(fix_적용(&root, &stderr), 4, "{stderr}");

    let (stdout, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(stderr, "", "fix 를 적용하면 새 진단이 생기면 안 된다");
    assert_eq!(stdout, "");
    정리(&root);
}

/// **M8 의 특성 고정 테스트.** `참조들` 의 손 파싱을 [`kang::bless::ImportAddress::parse`]
/// 로 갈아 끼우기 **전에** 오늘 통과하는 입력을 못박는다. 구분자를 담은 디렉토리 이름은
/// 마지막 `/` 를 먼저 가르기 때문에 오늘 정확히 동작하며, 그것이 깨지면 안 된다.
#[test]
fn refs_는_구분자가_든_디렉토리_이름을_그대로_받는다() {
    let root = 임시_루트("refs-구분자-디렉토리");
    git_저장소로(&root);
    // 디렉토리 이름에 `.` 과 `#` 이 있다. 문서 이름과 키워드 이름은 구분자가 없다.
    쓰기(
        &root,
        "v1.2/a#b/pay.kang",
        "---\ndescription: 결제 정책\n---\n\nkeyword `결제`: 대금을 지불하는 행위\n\n## 결제의 방법\n\n사용자는 `결제` 를 한다.\n",
    );

    let (stdout, stderr, 코드) = 실행(&root, &["refs", "v1.2/a#b/pay.결제"]);

    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(stdout, "v1.2/a#b/pay#결제의 방법\n");
    정리(&root);
}

/// topic 주소를 `refs` 에 주면 **모양 오류**다. `refs` 가 받는 것은 키워드뿐이다 (스펙 6.5).
#[test]
fn refs_는_topic_주소를_거절하고_사용법을_출력한다() {
    let root = 임시_루트("refs-topic-주소");
    git_저장소로(&root);
    정상_문서(&root);

    let (도움말, _, _) = 실행(&root, &["--help"]);
    let (stdout, stderr, 코드) = 실행(&root, &["refs", "docs/a#결제의 방법"]);

    assert_eq!(코드, 2, "{stderr}");
    assert_eq!(stdout, "");
    assert!(stderr.contains(&도움말), "{stderr}");
    정리(&root);
}

/// `show` 도 **마지막 `/` 뒤**에서 가른다 (스펙 6.0 주소 분할 규칙). 전체 문자열의 첫 `#`
/// 로 가르면 디렉토리 이름의 `#` 에서 갈려, `list`·`refs` 가 받는 문서를 `show` 만 거절한다.
#[test]
fn show_는_구분자가_든_디렉토리_이름을_그대로_받는다() {
    let root = 임시_루트("show-구분자-디렉토리");
    git_저장소로(&root);
    쓰기(
        &root,
        "v1.2/a#b/pay.kang",
        "---\ndescription: 결제 정책\n---\n\nkeyword `결제`: 대금을 지불하는 행위\n\n## 결제의 방법\n\n사용자는 `결제` 를 한다.\n",
    );

    let (문서, stderr, 코드) = 실행(&root, &["show", "v1.2/a#b/pay"]);
    assert_eq!(코드, 0, "{stderr}");
    assert!(문서.starts_with("path: v1.2/a#b/pay\n"), "{문서}");

    let (토픽, stderr, 코드) = 실행(&root, &["show", "v1.2/a#b/pay#결제의 방법"]);
    assert_eq!(코드, 0, "{stderr}");
    assert!(토픽.contains("결제의 방법"), "{토픽}");
    정리(&root);
}

/// 자리가 틀린 import 선언은 **참인 진단 하나**로 잡히고, 그 fix 를 적용하면 빌드가 통과한다.
///
/// 오늘은 그 줄이 산문이 되어 `K001` 이 셋 난다. 그중 둘은 문서 경로 조각(`docs`·`pay`)에
/// 대한 것이고 fix 가 그것을 keyword 로 선언하라고 안내한다 — 그대로 하면 빌드는 통과하되
/// 스펙 4.3 이 선언하지 말라고 못박은 일반 명사가 SoT 에 박힌다. 셋째는 9행이 정확히 그
/// 이름으로 import 하는데도 "import 하지 않았다" 고 말한다.
#[test]
fn 첫_topic_뒤_import_는_진단_하나로_잡히고_옮기면_통과한다() {
    let root = 임시_루트("import-자리");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/pay.kang",
        "---\ndescription: 결제 정책\n---\n\nkeyword `결제`: 대금을 지불하는 행위\n\n## 결제의 방법\n\n사용자는 `결제` 를 한다.\n",
    );
    쓰기(
        &root,
        "docs/card.kang",
        "---\ndescription: 카드 정책\n---\n\nimport `docs`/`pay`.`결제`\n\n## 카드 결제\n\n`결제` 를 카드로 한다.\n",
    );

    // **핀을 하드코딩하지 않는다.** 통과하는 상태를 `bless` 로 먼저 만들고 그 줄을 옮긴다 —
    // 그래야 아래의 진단이 자리 때문이라는 것이 증명되고, 정규화 규칙이 바뀌어도 죽지 않는다.
    let (_, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(코드, 1, "{stderr}");
    assert_eq!(fix_적용(&root, &stderr), 1, "{stderr}");
    let 통과본 = 읽기(&root, "docs/card.kang");
    let import_줄 = 통과본
        .lines()
        .find(|줄| 줄.starts_with("import "))
        .expect("bless 가 핀을 넣은 import 줄이 있어야 한다")
        .to_string();
    let (stdout, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(stdout, "");

    // 같은 줄을 첫 topic **뒤**로 옮긴다. 그 밖의 문법은 전부 합법이며 핀도 그대로다.
    let 옮긴본 = format!(
        "---\ndescription: 카드 정책\n---\n\n## 카드 결제\n\n{import_줄}\n\n`결제` 를 카드로 한다.\n"
    );
    쓰기(&root, "docs/card.kang", &옮긴본);

    let (stdout, stderr, 코드) = 실행(&root, &["build"]);

    assert_eq!(코드, 1, "{stderr}");
    assert_eq!(stdout, "");
    assert_eq!(stderr.matches("error[").count(), 1, "{stderr}");
    assert!(stderr.contains("K114"), "{stderr}");
    // 거짓 진단 셋이 사라져야 한다 — 특히 일반 명사를 선언하라는 안내가 없어야 한다.
    assert!(!stderr.contains("K001"), "{stderr}");
    assert!(!stderr.contains("keyword `docs`"), "{stderr}");

    // fix 가 시킨 대로 그 줄을 다시 파일 최상단으로 옮긴다.
    쓰기(&root, "docs/card.kang", &통과본);

    let (stdout, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(stderr, "", "fix 를 적용하면 새 진단이 생기면 안 된다");
    assert_eq!(stdout, "");
    정리(&root);
}

/// **`list` 가 낸 주소는 `show` 가 받아야 한다.** 도구가 자기 출력을 자기 파서로 거절하면서
/// 처방으로 같은 경로를 다시 주면 결정론적 루프다. 여기서 검증하는 것은 사람이 손으로 옮겨
/// 적은 주소가 아니라 **`list` 의 stdout 그 자체**다.
#[test]
fn list_가_낸_주소를_show_가_그대로_받는다() {
    let root = 임시_루트("list-show-왕복");
    git_저장소로(&root);
    // 디렉토리 이름에는 구분자가 합법이다 (스펙 6.0 주소 분할 규칙).
    쓰기(
        &root,
        "v1.2/a#b/pay.kang",
        "---\ndescription: 결제 정책\n---\n\nkeyword `결제`: 대금을 지불하는 행위\n\n## 결제의 방법\n\n사용자는 `결제` 를 한다.\n",
    );

    let (목록, stderr, 코드) = 실행(&root, &["list"]);
    assert_eq!(코드, 0, "{stderr}");

    // 목록 줄에서 주소만 뗀다 (스펙 6.3 은 `<경로>: <description>` 이다).
    let 주소들: Vec<&str> = 목록
        .lines()
        .map(|줄| 줄.split_once(": ").map_or(줄, |(주소, _)| 주소))
        .collect();
    assert_eq!(주소들, vec!["v1.2/a#b/pay"], "{목록}");

    // 훑은 주소를 하나씩 그대로 `show` 에 넣는다.
    for 주소 in 주소들 {
        let (문서, stderr, 코드) = 실행(&root, &["show", 주소]);
        assert_eq!(
            코드, 0,
            "list 가 낸 주소를 show 가 거절했다: {주소}\n{stderr}"
        );
        assert!(문서.starts_with(&format!("path: {주소}\n")), "{문서}");
    }
    정리(&root);
}

/// 문서 **파일 이름**의 `.`·`#`·`!` 는 CLI 주소로 가리킬 수 없으므로 로드가 거절한다
/// (스펙 6.0 `:414`). 셋 다 `bless`·`refs` 가 막히고, `#` 은 `show` 도 막힌다.
///
/// 거절하지 않으면 `list`·`keywords` 가 **아무 명령도 받지 않는 주소**를 찍는다.
#[test]
fn 문서_파일_이름의_구분자는_로드를_거절한다() {
    let root = 임시_루트("문서이름-구분자");
    git_저장소로(&root);
    // 구분자 셋을 각각 하나씩. 문서 이름 외의 문법은 전부 합법이다.
    for 이름 in ["a#b", "v1.2", "x!y"] {
        쓰기(
            &root,
            &format!("docs/{이름}.kang"),
            "---\ndescription: 결제 정책\n---\n\nkeyword `결제`: 대금을 지불하는 행위\n\n## 결제의 방법\n\n사용자는 `결제` 를 한다.\n",
        );
    }

    let (stdout, stderr, 코드) = 실행(&root, &["build"]);

    assert_eq!(코드, 1, "{stderr}");
    assert_eq!(stdout, "");
    assert_eq!(stderr.matches("K113").count(), 3, "{stderr}");
    // 셋 다 `Project.docs` 에서 빠져야 한다. 남아 있으면 세 문서가 같은 이름을 선언하므로
    // `K012` 가 함께 나고, 그 진단의 fix 는 가리킬 수 없는 주소로 iknow 를 쓰라고 시킨다.
    assert_eq!(stderr.matches("error[").count(), 3, "{stderr}");
    // 파일 이름을 고치라고 말해야 한다 — 그 말을 하지 않으면 처방이 아예 없다.
    assert!(stderr.contains("파일 이름"), "{stderr}");

    // 조회 명령이 쓸 수 없는 주소를 찍으면 안 된다.
    let (목록, _, 코드) = 실행(&root, &["list"]);
    assert_eq!(코드, 1);
    assert_eq!(목록, "", "쓸 수 없는 주소를 찍었다: {목록}");
    정리(&root);
}

/// **처방이 있어야 한다.** 파일 이름에 `.` 이 든 문서의 심볼을 import 하면 오늘은
/// `K020` 의 유일한 fix 인 `bless` 가 exit 2 로 죽어 빌드가 영구히 error 에 머문다
/// (`bless` 에 수동 rev 주입 인자가 없고 스펙 4.8 이 핀을 손으로 계산할 수 없다고 못박는다).
///
/// 파일 이름을 고치는 것이 유일한 해결이므로 진단이 그 말을 해야 하고, 고친 뒤에는
/// `K020` 의 fix 가 실제로 돌아 빌드가 통과해야 한다.
#[test]
fn 문서_이름을_고치면_핀_fix_가_실제로_돈다() {
    let root = 임시_루트("문서이름-봉쇄해제");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/v1.2.kang",
        "---\ndescription: 버전 정책\n---\n\nkeyword `승격`: 다음 단계로 올리는 행위\n\n## 승격의 방법\n\n`승격` 은 이렇게 한다.\n",
    );
    쓰기(
        &root,
        "docs/use.kang",
        "---\ndescription: 사용처\n---\n\nimport `docs`/`v1.2`.`승격`\n\n## 승격 사용\n\n`승격` 을 쓴다.\n",
    );

    let (_, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(코드, 1, "{stderr}");
    assert!(stderr.contains("K113"), "{stderr}");

    // 진단이 시킨 대로 파일 이름을 고치고 그것을 가리키던 import 를 맞춘다.
    fs::rename(root.join("docs/v1.2.kang"), root.join("docs/v12.kang"))
        .expect("파일 이름을 바꿀 수 있어야 한다");
    쓰기(
        &root,
        "docs/use.kang",
        "---\ndescription: 사용처\n---\n\nimport `docs`/`v12`.`승격`\n\n## 승격 사용\n\n`승격` 을 쓴다.\n",
    );

    // 이제 남는 것은 핀뿐이고, 그 fix 는 복사해 실행하면 실제로 돈다.
    let (_, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(코드, 1, "{stderr}");
    assert_eq!(fix_적용(&root, &stderr), 1, "{stderr}");

    let (stdout, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(stderr, "", "fix 를 적용하면 새 진단이 생기면 안 된다");
    assert_eq!(stdout, "");
    정리(&root);
}

/// 심볼 이름의 `/` 는 **빌드를 영구히 봉쇄한다** (스펙 6.0 `:415`). `## 환불/취소` 는
/// 컴파일을 통과하되 그 topic 을 import 한 문서의 `K020` 이 처방한 `kang bless` 가
/// exit 2 로 죽어(주소가 마지막 `/` 뒤에서 갈리므로 이름 안이 잘린다) 다른 처방이 없다.
///
/// 그래서 선언·참조를 읽는 자리에서 거절하고, 이름을 고치면 `K020` 의 fix 가 실제로 돌아
/// 빌드가 통과해야 한다. 봉쇄가 열리는 것을 이 왕복이 증명한다.
#[test]
fn 이름의_슬래시를_빼면_핀_fix_가_실제로_돈다() {
    let root = 임시_루트("이름-슬래시");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/A.kang",
        "---\ndescription: 환불 정책\n---\n\n## 환불/취소\n\n환불과 취소를 다룬다.\n",
    );
    쓰기(
        &root,
        "docs/B.kang",
        "---\ndescription: 하위 정책\n---\n\nimport `docs`/`A`#`환불/취소`\n\n## 하위 정책\n\n`환불/취소` 를 따른다.\n",
    );

    // 선언 한 자리(A 의 헤딩)와 참조 두 자리(B 의 import 줄·본문)가 각각 거절된다.
    let (stdout, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(코드, 1, "{stderr}");
    assert_eq!(stdout, "");
    assert_eq!(stderr.matches("K115").count(), 3, "{stderr}");
    assert_eq!(stderr.matches("error[").count(), 3, "{stderr}");
    // 이름을 고치라고 말해야 한다 — 그 말을 하지 않으면 처방이 아예 없다.
    assert!(stderr.contains('/'), "{stderr}");

    // 진단이 시킨 대로 두 문서에서 `/` 를 뺀다.
    쓰기(
        &root,
        "docs/A.kang",
        "---\ndescription: 환불 정책\n---\n\n## 환불 취소\n\n환불과 취소를 다룬다.\n",
    );
    쓰기(
        &root,
        "docs/B.kang",
        "---\ndescription: 하위 정책\n---\n\nimport `docs`/`A`#`환불 취소`\n\n## 하위 정책\n\n`환불 취소` 를 따른다.\n",
    );

    // 남는 것은 핀뿐이고, 그 fix 는 복사해 실행하면 실제로 돈다.
    let (_, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(코드, 1, "{stderr}");
    assert!(stderr.contains("K020"), "{stderr}");
    assert_eq!(fix_적용(&root, &stderr), 1, "{stderr}");

    let (stdout, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(stderr, "", "fix 를 적용하면 새 진단이 생기면 안 된다");
    assert_eq!(stdout, "");
    정리(&root);
}

/// 스펙 V0001 의 예제 프로젝트는 합법 kang 이어야 한다. 스펙이 든 예제가 자기 컴파일러를
/// 통과하지 못하면 스펙과 구현 중 하나가 틀린 것이다.
#[test]
fn fixture_프로젝트가_build_를_통과한다() {
    let root = 임시_루트("예제-통과");
    git_저장소로(&root);
    예제_프로젝트_통과(&root);

    let (stdout, stderr, 코드) = 실행(&root, &["build"]);

    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(stderr, "", "예제 프로젝트는 진단을 내지 않아야 한다");
    assert_eq!(stdout, "", "build 는 문서를 출력하지 않는다");
    정리(&root);
}

/// 상위 정책이 바뀌면 그것을 import 한 문서가 **전부** 깨진다 (스펙 4.8). 한 곳만 깨지면
/// 나머지가 옛 정책을 전제로 방치된다.
#[test]
fn 상위_문서_수정_후_모든_참조처가_깨진다() {
    let root = 임시_루트("상위-수정");
    git_저장소로(&root);
    예제_프로젝트_통과(&root);

    // `docs/A` 의 `결제` 한 줄 정의를 고친다. `docs/B` 와 `docs/C` 가 둘 다 이것을 import 한다.
    let a = 읽기(&root, "docs/A.kang").replace(
        "keyword `결제`: 사용자가 상품 대금을 지불하는 행위",
        "keyword `결제`: 사용자가 상품 대금을 지불하고 기록을 남기는 행위",
    );
    쓰기(&root, "docs/A.kang", &a);

    let (_, stderr, 코드) = 실행(&root, &["build"]);

    assert_eq!(코드, 1, "{stderr}");
    // 참조처가 둘이므로 진단도 둘이다. 하나만 나면 나머지가 방치된다.
    assert_eq!(stderr.matches("error[K021]").count(), 2, "{stderr}");
    assert!(stderr.contains("docs/B.kang:5"), "{stderr}");
    assert!(stderr.contains("docs/C.kang:5"), "{stderr}");
    정리(&root);
}

/// 정상 워크플로는 "참조처를 고친 뒤 `bless`" 다 (스펙 6.2). 그래서 진단이 알린 줄 번호는
/// bless 시점에 이미 낡아 있다. 주소가 심볼이므로 그래도 맞는 줄이 갱신된다 (ADR-0003).
#[test]
fn 참조처를_먼저_고친_뒤_bless_해도_올바른_핀이_갱신된다() {
    let root = 임시_루트("고친뒤-bless");
    git_저장소로(&root);
    예제_프로젝트_통과(&root);
    let 옛_핀 = 읽기(&root, "docs/C.kang");

    let a = 읽기(&root, "docs/A.kang").replace(
        "keyword `결제`: 사용자가 상품 대금을 지불하는 행위",
        "keyword `결제`: 사용자가 상품 대금을 지불하고 기록을 남기는 행위",
    );
    쓰기(&root, "docs/A.kang", &a);

    // 참조처를 먼저 고친다 — 빈 줄 하나로 import 블록이 한 줄 아래로 밀린다.
    let c = 읽기(&root, "docs/C.kang").replacen("---\n\nimport", "---\n\n\nimport", 1);
    쓰기(&root, "docs/C.kang", &c);
    let (_, stderr, _) = 실행(&root, &["build"]);
    assert!(
        stderr.contains("docs/C.kang:6"),
        "밀린 줄이 진단에 나와야 한다: {stderr}"
    );

    let (_, stderr, 코드) = 실행(&root, &["bless", "docs/C", "--import", "docs/A.결제"]);
    assert_eq!(코드, 0, "{stderr}");

    // 밀린 자리의 import 줄이 갱신되었고, 값도 실제로 달라졌다.
    let 새_본문 = 읽기(&root, "docs/C.kang");
    let 새_줄 = 새_본문
        .lines()
        .nth(5)
        .expect("여섯째 줄이 있어야 한다")
        .to_string();
    assert!(
        새_줄.starts_with("import `docs`/`A`.`결제` rev \""),
        "{새_줄}"
    );
    assert!(
        !옛_핀.contains(&새_줄),
        "핀이 실제로 바뀌어야 한다: {새_줄}"
    );
    // 나머지 참조처(`docs/B`)는 여전히 깨진 채여야 한다 — 일괄 해제 수단은 없다 (스펙 6.2).
    let (_, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(코드, 1, "{stderr}");
    assert_eq!(stderr.matches("error[K021]").count(), 1, "{stderr}");
    assert!(stderr.contains("docs/B.kang:5"), "{stderr}");
    정리(&root);
}

/// exception 의 해시 입력은 **그것을 선언한 topic 의 본문**이다 (스펙 4.8). 맥락이 바뀌면
/// 그 예외를 커버하는 문서가 깨져야 한다.
#[test]
fn exception_선언_topic_을_바꾸면_커버_문서가_깨진다() {
    let root = 임시_루트("예외-맥락");
    git_저장소로(&root);
    예제_프로젝트_통과(&root);

    // 예외 이름도 `cover` 줄도 그대로 두고, 예외를 선언한 topic 의 **본문만** 고친다.
    let b = 읽기(&root, "docs/B.kang").replace(
        "`결제` 를 하기 위한 수단 중",
        "`결제` 를 하기 위한 여러 수단 중",
    );
    쓰기(&root, "docs/B.kang", &b);

    let (_, stderr, 코드) = 실행(&root, &["build"]);

    assert_eq!(코드, 1, "{stderr}");
    assert!(stderr.contains("error[K021]"), "{stderr}");
    // 깨지는 것은 예외를 import 한 줄이다 (`docs/C` 의 셋째 import).
    assert!(stderr.contains("`카드 수단 무료 결제`"), "{stderr}");
    assert!(stderr.contains("docs/C.kang:7"), "{stderr}");
    정리(&root);
}

/// 순환은 **체인 전체**를 출력해야 한다 (스펙 5.1). 한 문서만 가리키면 어디를 끊을지
/// 판단할 수 없다.
#[test]
fn 순환_import_를_만들면_체인이_출력된다() {
    let root = 임시_루트("순환");
    git_저장소로(&root);
    예제_프로젝트_통과(&root);

    // `docs/A` 가 `docs/C` 의 topic 을 import 하면 A → C → B → A 가 닫힌다.
    let a = 읽기(&root, "docs/A.kang")
        .replacen(
            "---\n\nkeyword",
            "---\n\nimport `docs`/`C`#`무료결제의 구성요소`\n\nkeyword",
            1,
        )
        .replace(
            "`결제` 는 `청구서` 와 `결제 내역` 으로 이루어져 있다.",
            "`결제` 는 `청구서` 와 `결제 내역` 으로 이루어져 있다. `무료결제의 구성요소` 도 참고한다.",
        );
    쓰기(&root, "docs/A.kang", &a);

    let (_, stderr, 코드) = 실행(&root, &["build"]);

    assert_eq!(코드, 1, "{stderr}");
    assert!(stderr.contains("error[K040]"), "{stderr}");
    // 체인은 닫히는 지점까지 전부 나온다.
    assert!(
        stderr.contains("docs/A → docs/C → docs/B → docs/A"),
        "{stderr}"
    );
    // 체인에 든 문서의 import 줄이 모두 위치로 나온다.
    for 자리 in ["docs/A.kang:5", "docs/C.kang:6", "docs/B.kang:5"] {
        assert!(stderr.contains(자리), "{자리} 가 없다: {stderr}");
    }
    정리(&root);
}

/// `show` 는 YAML 을 낸다 (스펙 6.4). 파서를 통과하지 못하면 소비자가 읽을 수 없다.
///
/// **의존성을 늘리지 않는다.** `python3` 과 `pyyaml` 이 있으면 반드시 돌고, 없으면
/// 건너뛴다. 이미터 자체의 단위 검증은 `tests/yaml.rs` 에 있다.
///
/// `KANG_REQUIRE_YAML` 이 켜진 환경에서는 건너뛰기가 실패다. CI 가 이 변수를 켜므로
/// 파서 없는 이미지로 갈아타는 순간 조용히 빠지지 않고 터진다.
#[test]
fn show_출력이_유효한_yaml_이다() {
    let root = 임시_루트("show-yaml");
    git_저장소로(&root);
    예제_프로젝트_통과(&root);

    // ponytail: python3 이나 pyyaml 이 없는 로컬 환경에서는 이 게이트가 빠진다. YAML
    // 파서를 의존성으로 들이지 않기 위한 대가이며, CI 는 아래 KANG_REQUIRE_YAML 로
    // 그 빠짐을 실패로 바꾼다.
    let 파서_있음 = Command::new("python3")
        .args(["-c", "import yaml"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|상태| 상태.success());
    if !파서_있음 {
        // KANG_REQUIRE_YAML 이 켜졌다면 건너뛰기가 곧 실패다. cargo test 는 통과한
        // 테스트의 출력을 삼키므로, eprintln 만으로는 CI 화면에 아무것도 남지 않는다.
        assert!(
            std::env::var_os("KANG_REQUIRE_YAML").is_none(),
            "KANG_REQUIRE_YAML 이 켜졌는데 python3 또는 pyyaml 이 없다"
        );
        eprintln!("python3 또는 pyyaml 이 없어 YAML 검증을 건너뛴다");
        정리(&root);
        return;
    }

    // 문서 주소와 topic 주소 양쪽을 본다. topic 조회가 다른 코드 경로다.
    for 주소 in [
        "docs/A",
        "docs/B",
        "docs/C",
        "docs/B#카드 결제",
        "docs/C#무료결제의 구성요소",
    ] {
        let (stdout, stderr, 코드) = 실행(&root, &["show", 주소]);
        assert_eq!(코드, 0, "{주소}: {stderr}");

        let 결과 = Command::new("python3")
            .args([
                "-c",
                "import sys,yaml; yaml.safe_load(sys.argv[1]) or sys.exit('빈 문서')",
                "--",
                &stdout,
            ])
            .output()
            .expect("python3 을 실행할 수 있어야 한다");
        assert!(
            결과.status.success(),
            "{주소} 의 출력이 YAML 이 아니다:\n{stdout}\n{}",
            String::from_utf8_lossy(&결과.stderr)
        );
    }
    정리(&root);
}

/// 통과하지 못한 문서는 **어떤** CLI 명령으로도 출력되지 않는다 (스펙 5절).
/// 조회 명령 넷이 각자 손으로 반복한 별개 match 이므로 넷을 모두 본다.
#[test]
fn error_상태에서는_어떤_조회도_출력되지_않는다() {
    let root = 임시_루트("에러-조회");
    git_저장소로(&root);
    예제_프로젝트_통과(&root);
    // 통과하던 프로젝트를 깬다. 깨진 것은 `docs/A` 지만 어떤 조회도 막혀야 한다.
    let a = 읽기(&root, "docs/A.kang").replace(
        "keyword `결제`: 사용자가 상품 대금을 지불하는 행위",
        "keyword `결제`: 사용자가 상품 대금을 지불하고 기록을 남기는 행위",
    );
    쓰기(&root, "docs/A.kang", &a);

    for 인자 in [
        vec!["list"],
        vec!["keywords"],
        vec!["refs", "docs/A.결제"],
        vec!["show", "docs/B"],
        vec!["show", "docs/C#무료결제의 구성요소"],
    ] {
        let (stdout, stderr, 코드) = 실행(&root, &인자);
        assert_eq!(코드, 1, "{인자:?}: {stderr}");
        assert_eq!(stdout, "", "{인자:?} 가 출력했다");
        assert!(stderr.contains("error[K021]"), "{인자:?}: {stderr}");
    }
    정리(&root);
}

/// 스펙 5.1.1 이 예시로 못박은 세 진단(`K001`·`K012`·`K021`)이 **바이너리 출력에서**
/// 세 요소를 갖추는지 본다 — 관련 위치 전부, 왜 문제인지 한 문장, 그대로 적용 가능한 fix.
///
/// 단위 테스트는 진단을 손으로 만들어 [`kang::check::report`] 만 보므로, 진단 함수가
/// `compile()` 에 연결되지 않았거나 fix 가 산문으로 오염된 것은 여기서만 잡힌다.
#[test]
fn 진단_3종의_구조가_스펙_5_1_1_과_일치한다() {
    let root = 임시_루트("진단-3종");
    git_저장소로(&root);
    // `epoch` 를 두 문서가 선언(K012), `docs/c` 가 `승격` 을 참조만 함(K001),
    // `docs/d` 가 틀린 핀으로 import(K021).
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: 기준선\n---\n\nkeyword `승격`: 후보를 기준선으로 올리는 일\nkeyword `epoch`: 기준선의 세대\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: 초안\n---\n\nkeyword `epoch`: 초안의 세대\n",
    );
    쓰기(
        &root,
        "docs/c.kang",
        "---\ndescription: 회귀 정책\n---\n\n## 회귀의 기준\n\n`승격` 이후에는 회귀를 막는다.\n",
    );
    쓰기(
        &root,
        "docs/d.kang",
        "---\ndescription: 청구\n---\n\nimport `docs`/`a`.`승격` rev \"000000\"\n\n## 청구의 기준\n\n`승격` 을 따른다.\n",
    );

    let (_, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(코드, 1, "{stderr}");

    // 블록 하나가 진단 하나다. 머리글 줄이 블록의 시작이다 — 블록 **안에도** 빈 줄이
    // 있으므로 빈 줄로 가르면 안 된다.
    let mut 블록: Vec<String> = Vec::new();
    for 줄 in stderr.lines() {
        if 줄.starts_with("error[") || 줄.starts_with("warning[") {
            블록.push(String::new());
        }
        if let Some(현재) = 블록.last_mut() {
            현재.push_str(줄);
            현재.push('\n');
        }
    }
    for 코드이름 in ["K001", "K012", "K021"] {
        let 블록 = 블록
            .iter()
            .find(|블록| 블록.starts_with(&format!("error[{코드이름}]: ")))
            .unwrap_or_else(|| panic!("{코드이름} 블록이 없다: {stderr}"));
        // 왜 문제인지 **한 문장** — 머리글 줄이 곧 그것이다. 마침표만 보면 한 낱말도
        // 통과하므로 길이 하한을 함께 둔다. 규칙을 모르는 에이전트가 판단할 수 있어야 한다.
        let 머리글 = 블록.lines().next().expect("머리글 줄이 있어야 한다");
        assert!(머리글.ends_with('.'), "{머리글}");
        assert!(머리글.chars().count() > 40, "머리글이 너무 짧다: {머리글}");

        // **관련 위치 전부.** 존재만 보면 note 를 빈 문자열로 바꿔도 통과한다.
        // `  <문서>.kang:<줄>` + 공백 + 비어 있지 않은 note 를 줄마다 확인한다.
        let 위치_줄: Vec<&str> = 블록.lines().filter(|줄| 줄.contains(".kang:")).collect();
        assert!(!위치_줄.is_empty(), "{블록}");
        for 줄 in &위치_줄 {
            let (자리, note) = 줄
                .trim_start()
                .split_once("  ")
                .unwrap_or_else(|| panic!("위치와 note 를 가르는 공백이 없다: {줄}"));
            assert!(
                자리
                    .rsplit_once(':')
                    .is_some_and(|(_, 번호)| 번호.parse::<usize>().is_ok_and(|번호| 번호 > 0)),
                "줄 번호가 아니다: {줄}"
            );
            assert!(!note.trim().is_empty(), "note 가 비었다: {줄}");
        }

        // 그대로 적용 가능한 fix.
        assert!(블록.contains("\n  fix:\n"), "{블록}");
    }

    // `K012` 는 다중 위치 진단이다. 한 곳만 보여주면 나머지를 찾아 헤맨다.
    let k012 = 블록
        .iter()
        .find(|블록| 블록.starts_with("error[K012]: "))
        .expect("K012 블록이 있어야 한다")
        .as_str();
    assert!(k012.contains("docs/a.kang:6"), "{k012}");
    assert!(k012.contains("docs/b.kang:5"), "{k012}");

    // 문법이 갈린다 — `[edit]` 는 문서 문법(백틱), `[shell]` 은 CLI 문법(백틱 금지·인용).
    for 줄 in stderr.lines().filter(|줄| 줄.contains("[shell]")) {
        assert!(!줄.contains('`'), "셸 fix 에 백틱이 있다: {줄}");
    }
    // 그리고 `[shell]` 은 명령만이다 — 렌더된 줄을 그대로 실행할 수 있어야 한다.
    // 개수를 못박는다. 빈 목록을 돌면 이 검사가 조용히 사라진다 (`K001`·`K021` 이 하나씩).
    let 명령들 = 셸_fix(&stderr);
    assert_eq!(명령들.len(), 2, "{stderr}");
    for 명령 in 명령들 {
        assert!(명령.starts_with("kang "), "명령으로 시작해야 한다: {명령}");
    }
    정리(&root);
}

/// `parse_document` 는 `lines()` 로, `bless` 의 되쓰기는 `split_inclusive('\n')` 로 줄을
/// 가른다. **두 색인이 어긋나면 엉뚱한 줄에 핀이 박힌다.**
///
/// BOM 과 CRLF 를 함께 준다. BOM 은 로더가 벗기지만 되쓰기는 원문을 쓰므로 조각 0 에
/// 남고, CRLF 는 `lines()` 가 `\r` 를 떼지만 `split_inclusive` 는 남긴다 — 두 규칙이
/// 한 파일에서 동시에 걸리는 경우다.
#[test]
fn bom_과_crlf_문서에서도_지정한_줄에만_핀이_박힌다() {
    let root = 임시_루트("bom-crlf");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/base.kang",
        "---\ndescription: 기초\n---\n\n## 기초 정책\n\n기초를 적는다.\n",
    );
    // `\u{feff}` 는 UTF-8 로 EF BB BF 세 바이트다.
    let 원본 = "\u{feff}---\r\ndescription: 꼭대기\r\n---\r\n\r\nimport `docs`/`base`#`기초 정책` as `기초`\r\n\r\n## 꼭대기 정책\r\n\r\n`기초` 를 따른다.\r\n";
    쓰기(&root, "docs/top.kang", 원본);

    let (_, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(코드, 1, "{stderr}");
    // 진단이 가리키는 줄은 BOM 을 벗긴 뒤의 색인이다.
    assert!(stderr.contains("docs/top.kang:5"), "{stderr}");
    fix_적용(&root, &stderr);

    let 결과 = 읽기(&root, "docs/top.kang");
    // 핀은 다섯째 줄에만 들어가고, 삽입 위치는 `\r` **앞**이다.
    let 새_줄 = 결과
        .split_inclusive('\n')
        .nth(4)
        .expect("다섯째 줄이 있어야 한다");
    assert!(
        새_줄.starts_with("import `docs`/`base`#`기초 정책` as `기초` rev \"")
            && 새_줄.ends_with("\"\r\n"),
        "{새_줄:?}"
    );
    // **나머지 줄은 한 바이트도 바뀌지 않았다.** 몇 줄만 골라 보면 나머지가 망가져도
    // 통과하므로, 원본의 다섯째 줄만 갈아 끼운 기대값과 파일 전체를 비교한다.
    // BOM 도 여기서 함께 지켜진다 — 원본의 첫 바이트가 기대값에 들어 있다.
    let 기대 = 원본.replace("import `docs`/`base`#`기초 정책` as `기초`\r\n", 새_줄);
    assert_eq!(결과, 기대);

    let (_, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(코드, 0, "{stderr}");
    정리(&root);
}

/// 비 UTF-8 문서는 `K051` 이 위에서 막으므로 줄 색인 문제에 도달하지 않는다.
/// `bless` 도 같은 로더를 거치므로 **파일을 한 바이트도 건드리지 않는다.**
#[test]
fn 비_utf8_문서는_bless_가_손대지_않는다() {
    let root = 임시_루트("비-utf8");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/base.kang",
        "---\ndescription: 기초\n---\n\n## 기초 정책\n\n기초를 적는다.\n",
    );
    // UTF-8 이 아닌 바이트열을 직접 쓴다. `쓰기` 는 &str 만 받으므로 여기서만 예외다.
    let 원본: Vec<u8> = b"---\ndescription: \xB1\xE2\xC3\xCA\n---\n\nimport `docs`/`base`#`\xB1\xE2\xC3\xCA \xC1\xA4\xC3\xA5`\n".to_vec();
    fs::create_dir_all(root.join("docs")).expect("디렉토리를 만들 수 있어야 한다");
    fs::write(root.join("docs/top.kang"), &원본).expect("파일을 쓸 수 있어야 한다");

    let (_, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(코드, 1, "{stderr}");
    assert!(stderr.contains("K051"), "{stderr}");

    let (_, stderr, 코드) = 실행(
        &root,
        &["bless", "docs/top", "--import", "docs/base#기초 정책"],
    );
    assert_eq!(코드, 1, "{stderr}");
    assert_eq!(
        fs::read(root.join("docs/top.kang")).expect("파일이 있어야 한다"),
        원본,
        "bless 가 비 UTF-8 문서를 건드렸다"
    );
    정리(&root);
}

/// **알려진 천장의 특성 고정** (`check.rs` 의 `이름_분할` 마커). 같은 줄의 백틱 조각들은
/// 원문의 `.` 을 보지 않고 스코프만 보고 이어 붙는다. 그래서 `` `수단` 과 `카드` `` 처럼
/// 따로 언급한 두 이름이 계층 keyword `수단`.`카드` 로 병합되고, 선언되지 않은 `카드`
/// 참조에 나야 할 `K001` 이 **나지 않는다.**
///
/// ponytail: 도그푸딩 코퍼스(`plans/`·`docs/` 2035 줄, 백틱 조각 2개 이상인 줄 216, 원문이
/// 점으로 이은 계층 이름 17종)에서 실제 충돌은 0 건이라 v1 에서 올리지 않는다. 올릴 조건은
/// 실제 코퍼스에서 충돌 1 건이 관측되는 것이며, 고치려면 파서가 원문 인접성을 실어야 해
/// `ast.rs`·`parse.rs` 변경이 필요하다. 그 변경이 오면 이 테스트가 뒤집힌다.
#[test]
fn 병합_천장은_같은_줄의_두_이름을_한_이름으로_읽는다() {
    let root = 임시_루트("병합-천장");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/x.kang",
        "---\ndescription: X\n---\n\nkeyword `수단`: 결제를 실행하는 방법\nkeyword `수단`.`카드`: 카드 수단\n\n## 정책\n\n`수단` 과 `카드` 를 함께 쓴다.\n",
    );

    // 병합되므로 통과한다. `카드` 단독 선언이 없으니 원래는 K001 이 나야 한다.
    let (_, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(코드, 0, "{stderr}");

    // 병합된 이름으로 조회되고, 상위 조각 단독으로는 조회되지 않는다.
    let (합쳐진, _, _) = 실행(&root, &["refs", "docs/x.수단.카드"]);
    assert_eq!(합쳐진, "docs/x#정책\n");
    let (상위, _, _) = 실행(&root, &["refs", "docs/x.수단"]);
    assert_eq!(상위, "", "상위 조각은 소비되어 남지 않는다");
    정리(&root);
}

/// **C1 의 회귀 테스트를 `resolve.rs` 진단까지 넓힌다.** `check.rs` 진단만 덮으면
/// `K050`·`K051` 의 `[shell]` fix 가 산문으로 시작하는 갭이 다시 열린다.
///
/// `K050` 은 저장소 밖 첫 실행에서 바로 나므로 에이전트의 T0 접점이다.
#[test]
fn k050_의_fix_를_그대로_실행하면_저장소가_생긴다() {
    let root = 임시_루트("k050-fix-왕복");
    // git 저장소로 만들지 않는다. 그것이 `K050` 의 조건이다.

    let (_, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(코드, 2, "{stderr}");
    assert!(stderr.contains("K050"), "{stderr}");
    assert_eq!(fix_적용(&root, &stderr), 1, "{stderr}");

    // 저장소가 생겼으므로 루트 탐색이 통과한다. 문서는 아직 없다.
    let (_, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(코드, 0, "{stderr}");
    assert!(!stderr.contains("K050"), "{stderr}");
    정리(&root);
}

/// `K051`(UTF-8 아님) 의 fix 도 그대로 실행할 수 있어야 한다.
///
/// 이 진단의 처방은 **인코딩을 확인한 뒤 변환**인데 변환 명령의 `-f` 인자는 확인
/// 결과에 달려 있어 진단 시점에 알 수 없다. 그래서 `[shell]` 로 낼 수 있는 것은
/// 확인 명령 하나뿐이고, 변환 방법은 `message` 가 말한다.
#[test]
fn k051_의_fix_를_그대로_실행하면_인코딩을_확인한다() {
    let root = 임시_루트("k051-fix-왕복");
    git_저장소로(&root);
    fs::create_dir_all(root.join("docs")).expect("디렉토리를 만들 수 있어야 한다");
    // EUC-KR 바이트열. UTF-8 로 디코딩되지 않는다.
    fs::write(
        root.join("docs/a.kang"),
        b"---\ndescription: \xB1\xE2\xC3\xCA\n---\n",
    )
    .expect("파일을 쓸 수 있어야 한다");

    let (_, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(코드, 1, "{stderr}");
    assert!(stderr.contains("K051"), "{stderr}");
    // 적용 가능한 명령은 하나다. 자리를 채워야 도는 템플릿은 fix 가 아니다.
    assert_eq!(fix_적용(&root, &stderr), 1, "{stderr}");
    // 변환 방법은 message 가 말한다 — 진단이 그것을 잃으면 안 된다.
    assert!(stderr.contains("iconv"), "{stderr}");
    정리(&root);
}

/// **J1 의 결론.** 컴파일러가 대상 문서를 아는 진단은 `bless` 를 짝지어 **1왕복**에
/// 닫아야 한다. `K034` 는 cover 줄이 있는 문서를 알면서(`check.rs` 의 `c.doc`) 짝짓지
/// 않아 `K001` 과 같은 상황에서 2왕복이었다.
///
/// `K030`·`K031` 은 어느 topic 이 정책인지 사람이 정하므로 문서를 모른다 — 거기서
/// bless 를 짝지으면 진단이 문서 이름을 지어낸다. 그래서 그 둘은 2왕복이 정본이다.
#[test]
fn k034_의_fix_를_한_왕복에_적용하면_통과한다() {
    let root = 임시_루트("k034-1왕복");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\n## 정책\n\n본문이다.\n\nexception `무료 상품`\n",
    );
    // cover 는 있으나 import 가 없다.
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: B\n---\n\n## 처리\n\n하나다.\n\ncover `무료 상품`\n",
    );

    let (_, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(코드, 1, "{stderr}");
    assert!(stderr.contains("error[K034]"), "{stderr}");
    // 진단이 지시한 import 줄을 그대로 넣는다 — `[edit]` 은 산문이므로 손으로 적용한다.
    let 줄 = "import `docs`/`a`!`무료 상품`";
    assert!(
        stderr.contains(&format!("import 블록에 다음 줄을 추가하세요: {줄}")),
        "{stderr}"
    );
    쓰기(
        &root,
        "docs/b.kang",
        &format!("---\ndescription: B\n---\n\n{줄}\n\n## 처리\n\n하나다.\n\ncover `무료 상품`\n"),
    );
    // 그리고 같은 진단이 낸 셸 fix 를 그대로 실행한다.
    assert_eq!(fix_적용(&root, &stderr), 1, "{stderr}");

    // 한 왕복으로 끝나야 한다. `K020` 이 새로 뜨면 2왕복이다 (스펙 V0001 5.1.1).
    let (_, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(stderr, "", "fix 를 적용하면 새 진단이 생기면 안 된다");
    정리(&root);
}

// ---------------------------------------------------------------------------
// init — 에이전트 진입점과 스킬 (스펙 6.1)
// ---------------------------------------------------------------------------

/// `init` 이 만드는 네 산출물. 스펙 6.1 의 표와 같은 순서다.
const 산출물: [&str; 4] = [
    ".claude/skills/kang/SKILL.md",
    "AGENTS.md",
    "CLAUDE.md",
    "docs/example.kang",
];

/// 하나라도 빠지면 에이전트가 kang 의 존재를 모른 채 `.kang` 을 `cat` 한다.
/// 만든 것을 이름으로 말해야 한다 — "만들었습니다" 만으로는 검증할 수 없다.
#[test]
fn 네_파일을_생성한다() {
    let root = 임시_루트("init-four");
    git_저장소로(&root);

    let (stdout, stderr, 코드) = 실행(&root, &["init"]);

    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(stdout, "", "init 은 파일을 만들 뿐 데이터를 내지 않는다");
    // 네 산출물이 전부 실재하고, 비어 있지 않고, 이름으로 보고되어야 한다.
    for 상대 in 산출물 {
        assert!(root.join(상대).exists(), "{상대} 가 없다: {stderr}");
        assert!(!읽기(&root, 상대).trim().is_empty(), "{상대} 가 비어 있다");
        assert!(
            stderr.contains(상대),
            "{상대} 를 만들었다고 말하지 않았다: {stderr}"
        );
    }
    정리(&root);
}

/// 스킬이 비어 있으면 파일만 생기고 원칙은 성립하지 않는다.
/// 스펙 6.1 의 다섯 케이스와 명령 목록이 전부 있어야 한다.
#[test]
fn 생성된_skill_md_가_비어있지_않다() {
    let root = 임시_루트("init-skill");
    git_저장소로(&root);

    let (_, stderr, 코드) = 실행(&root, &["init"]);
    assert_eq!(코드, 0, "{stderr}");

    let 스킬 = 읽기(&root, ".claude/skills/kang/SKILL.md");
    // Claude 는 frontmatter 로 스킬을 찾는다. 없으면 파일이 있어도 로드되지 않는다.
    assert!(스킬.starts_with("---\n"), "frontmatter 가 없다: {스킬}");
    assert!(스킬.contains("name: kang"), "{스킬}");
    // 다섯 케이스 (스펙 6.1 스킬 내용).
    for 케이스 in [
        "조회할 때",
        "쓸 때",
        "실패했을 때",
        "이름을 바꿀 때",
        "코드를 고칠 때",
    ] {
        assert!(스킬.contains(케이스), "케이스 {케이스} 가 없다: {스킬}");
    }
    // 케이스가 지시하는 명령이 전부 있어야 실행할 수 있다.
    for 명령 in [
        "kang build",
        "kang keywords",
        "kang refs",
        "kang show",
        "kang bless",
    ] {
        assert!(스킬.contains(명령), "{명령} 이 없다: {스킬}");
    }
    // 스킬 내용의 유일한 사본이므로 나머지 둘은 가리키기만 한다.
    for 상대 in ["AGENTS.md", "CLAUDE.md"] {
        let 내용 = 읽기(&root, 상대);
        assert!(
            내용.contains(".claude/skills/kang/SKILL.md"),
            "{상대} 가 스킬을 가리키지 않는다: {내용}"
        );
        assert!(
            !내용.contains("조회할 때"),
            "{상대} 가 스킬 내용을 복제했다: {내용}"
        );
    }
    정리(&root);
}

/// **사용자의 `.kang` 초안에 템플릿을 덧붙이면 안 된다.**
///
/// 스펙 6.1 은 "섹션만 덧붙인다" 를 세 마크다운 진입점에만 배정하고 첫 `.kang` 은
/// "템플릿" 으로 구분한다. marker 기제를 `.kang` 에 재사용하면 `description:` 이 없는
/// 초안에 템플릿이 그대로 append 되어 두 번째 `---`/`description:` 이 문서 중간에 박히고,
/// 그러면서 "만들었습니다" 라고 말한다.
///
/// 파싱에 실패하는 초안이 그 경우다 — 예제 판정이 `docs.is_empty()` 하나뿐이면
/// 파싱에 실패한 문서만 있는 프로젝트가 "문서가 없다" 로 읽힌다.
#[test]
fn init_은_파싱에_실패하는_예제_초안을_한_바이트도_바꾸지_않는다() {
    let root = 임시_루트("init-초안-보존");
    git_저장소로(&root);
    // frontmatter 가 없으므로 `K101` 로 파싱에 실패하고, `description:` 줄도 없다.
    let 초안 = "## 결제 초안\n\n아직 frontmatter 를 안 썼다.\n\n`결제` 를 정리할 예정.\n";
    쓰기(&root, "docs/example.kang", 초안);

    let (stdout, stderr, 코드) = 실행(&root, &["init"]);

    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(stdout, "");
    // 한 바이트도 바뀌지 않았다.
    assert_eq!(읽기(&root, "docs/example.kang"), 초안, "{stderr}");
    // 만들지 않은 것을 만들었다고 하면 검증하면 거짓이다.
    assert!(stderr.contains("docs/example.kang"), "{stderr}");
    assert!(stderr.contains("건너"), "{stderr}");
    // 나머지 셋은 만들어져야 한다.
    for 상대 in [".claude/skills/kang/SKILL.md", "AGENTS.md", "CLAUDE.md"] {
        assert!(root.join(상대).exists(), "{상대} 가 없다: {stderr}");
    }
    정리(&root);
}

/// 기존 `CLAUDE.md` 를 덮어쓰면 그 프로젝트의 지침이 사라진다.
#[test]
fn 기존_claude_md_를_덮어쓰지_않고_섹션만_덧붙인다() {
    let root = 임시_루트("init-append");
    git_저장소로(&root);
    // **개행으로 끝나지 않는 파일**이다. 그냥 이어 붙이면 마지막 줄에 달라붙는다.
    쓰기(&root, "CLAUDE.md", "# 내 프로젝트\n\n기존 지침을 지키세요.");

    let (_, stderr, 코드) = 실행(&root, &["init"]);

    assert_eq!(코드, 0, "{stderr}");
    let 내용 = 읽기(&root, "CLAUDE.md");
    assert!(내용.contains("# 내 프로젝트"), "{내용}");
    assert!(내용.contains("기존 지침을 지키세요."), "{내용}");
    assert!(내용.contains("kang"), "kang 안내가 없다: {내용}");
    // 마지막 줄에 달라붙지 않았는지 본다.
    assert!(
        !내용.contains("기존 지침을 지키세요.이"),
        "개행 없이 붙었다: {내용}"
    );
    for 줄 in 내용.lines() {
        assert!(
            !(줄.contains("기존 지침") && 줄.contains("kang")),
            "한 줄에 뭉쳤다: {내용}"
        );
    }
    정리(&root);
}

/// 두 번 실행해도 같은 상태여야 한다. 그리고 건너뛴 것을 건너뛰었다고 말해야 한다.
#[test]
fn 이미_kang_섹션이_있으면_건너뛴다() {
    let root = 임시_루트("init-twice");
    git_저장소로(&root);

    let (_, 첫_stderr, 첫_코드) = 실행(&root, &["init"]);
    assert_eq!(첫_코드, 0, "{첫_stderr}");
    let 첫_내용: Vec<String> = 산출물.iter().map(|상대| 읽기(&root, 상대)).collect();

    let (stdout, stderr, 코드) = 실행(&root, &["init"]);

    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(stdout, "");
    // 파일이 바이트 단위로 같아야 한다 — 섹션이 두 번 붙으면 여기서 걸린다.
    for (상대, 앞) in 산출물.iter().zip(&첫_내용) {
        assert_eq!(&읽기(&root, 상대), 앞, "{상대} 가 바뀌었다");
    }
    // "만들었습니다" 라고 하면서 건너뛰면 검증하면 거짓이다.
    assert!(
        stderr.contains("건너"),
        "건너뛴 것을 말하지 않았다: {stderr}"
    );
    for 상대 in 산출물 {
        assert!(
            stderr.contains(상대),
            "{상대} 의 처리를 말하지 않았다: {stderr}"
        );
    }
    정리(&root);
}

/// 이미 kang 을 쓰는 프로젝트에 예제 템플릿을 더하면 쓰레기가 생긴다.
#[test]
fn 이미_kang_파일이_있으면_예제를_만들지_않는다() {
    let root = 임시_루트("init-has-kang");
    git_저장소로(&root);
    정상_문서(&root);

    let (_, stderr, 코드) = 실행(&root, &["init"]);

    assert_eq!(코드, 0, "{stderr}");
    assert!(
        !root.join("docs/example.kang").exists(),
        "예제를 만들었다: {stderr}"
    );
    // 만들지 않았다는 사실도 말해야 한다.
    assert!(stderr.contains("docs/example.kang"), "{stderr}");
    assert!(stderr.contains("건너"), "{stderr}");
    // 나머지 셋은 만들어져야 한다.
    for 상대 in [".claude/skills/kang/SKILL.md", "AGENTS.md", "CLAUDE.md"] {
        assert!(root.join(상대).exists(), "{상대} 가 없다: {stderr}");
    }
    정리(&root);
}

/// 도구가 만든 문서가 도구를 통과하지 못하면 그것이 첫 경험이다.
#[test]
fn init_직후_build_가_통과한다() {
    let root = 임시_루트("init-build");
    git_저장소로(&root);

    let (_, init_stderr, init_코드) = 실행(&root, &["init"]);
    assert_eq!(init_코드, 0, "{init_stderr}");

    let (stdout, stderr, 코드) = 실행(&root, &["build"]);

    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(stdout, "");
    assert_eq!(stderr, "", "생성한 문서에 진단이 붙으면 안 된다");
    정리(&root);
}

/// TTHW 측정 기준 — 세 명령이다. `init` 은 git 을 요구하지 않지만 `build` 는 요구한다.
#[test]
fn git_init_후_kang_init_과_build_세_명령으로_통과한다() {
    let root = 임시_루트("init-tthw");

    // 1. git init
    git_저장소로(&root);
    // 2. kang init
    let (_, init_stderr, init_코드) = 실행(&root, &["init"]);
    assert_eq!(init_코드, 0, "{init_stderr}");
    // 3. kang build
    let (stdout, stderr, 코드) = 실행(&root, &["build"]);

    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(stdout, "");
    assert_eq!(stderr, "", "세 명령 사이에 손으로 고칠 것이 없어야 한다");
    정리(&root);
}

/// 갓 만든 디렉토리에서 종료 코드 2 로 죽으면 T0 벽이 된다.
#[test]
fn git_저장소가_아니어도_init_이_성공하고_git_init_을_안내한다() {
    let root = 임시_루트("init-no-git");

    let (stdout, stderr, 코드) = 실행(&root, &["init"]);

    assert_eq!(코드, 0, "{stderr}");
    assert_eq!(stdout, "");
    // 현재 디렉토리를 루트로 삼는다.
    for 상대 in 산출물 {
        assert!(root.join(상대).exists(), "{상대} 가 없다: {stderr}");
    }
    assert!(stderr.contains("git init"), "안내가 없다: {stderr}");
    // 환경 오류가 아니므로 사용법을 내지 않는다 (스펙 6절).
    assert!(
        !stderr.contains("종료 코드:"),
        "사용법을 내면 에이전트가 철자를 의심한다: {stderr}"
    );
    정리(&root);
}

/// 다른 도구의 섹션을 건드리면 그 도구가 깨진다. 그리고 marker 를 **산문 안에 인용한**
/// 파일은 아직 kang 안내를 갖지 않았으므로 덧붙여야 한다.
#[test]
fn 다른_도구_섹션이_있는_claude_md_에_kang_섹션만_덧붙인다() {
    let root = 임시_루트("init-other-tool");
    git_저장소로(&root);
    // CLAUDE.md 의 marker 를 **산문 안에 인용한** 줄이 들어 있다. 인용은 지시가 아니므로
    // 실제 안내는 덧붙어야 한다.
    쓰기(
        &root,
        "CLAUDE.md",
        "# 규칙\n\n## 다른 도구\n\n여기에 \"이 프로젝트의 문서는 kang 으로 유지보수된다\" 를 적으라고 적혀 있을 뿐입니다.\n\n## 배포\n\n배포 절차입니다.\n",
    );
    쓰기(
        &root,
        "AGENTS.md",
        "## 다른 도구\n\n다른 도구의 지침입니다.\n",
    );

    let (_, stderr, 코드) = 실행(&root, &["init"]);
    assert_eq!(코드, 0, "{stderr}");

    // 다른 도구의 섹션은 그대로다.
    for 상대 in ["CLAUDE.md", "AGENTS.md"] {
        let 내용 = 읽기(&root, 상대);
        assert!(내용.contains("## 다른 도구"), "{상대}: {내용}");
        assert!(
            내용.contains(".claude/skills/kang/SKILL.md"),
            "{상대} 에 kang 안내가 없다: {내용}"
        );
    }
    // 인용은 지시가 아니다 — 산문 인용이 있어도 실제 안내는 덧붙고 인용은 남는다.
    let claude = 읽기(&root, "CLAUDE.md");
    assert!(claude.contains("## 배포"), "배포 절이 사라졌다: {claude}");
    assert!(claude.contains("적혀 있을 뿐입니다."), "{claude}");

    // 다시 실행해도 한 번만 있어야 한다.
    let (_, stderr, 코드) = 실행(&root, &["init"]);
    assert_eq!(코드, 0, "{stderr}");
    for 상대 in ["CLAUDE.md", "AGENTS.md"] {
        let 내용 = 읽기(&root, 상대);
        assert_eq!(
            내용.matches(".claude/skills/kang/SKILL.md").count(),
            1,
            "{상대} 에 두 번 붙었다: {내용}"
        );
    }
    정리(&root);
}

// ─── kang index (V0004 Task 3) ────────────────────────────────────────────────

/// 세 종류와 계층 keyword 를 한 문서에 모아 둔 픽스처를 만든다.
///
/// # 매개변수
/// - `root`: 프로젝트 루트
fn 인덱스_픽스처(root: &Path) {
    쓰기(
        root,
        "docs/a.kang",
        "---\ndescription: 결제 기반\n---\n\n\
         keyword `결제`: 대금을 지불하는 행위\n\n\
         keyword `결제수단`: 대금을 내는 방법\n\n\
         keyword `결제수단`.`카드`: 카드로 내는 방법\n\n\
         ## 결제의 기본\n\n\
         `결제` 는 `결제수단`.`카드` 로 이뤄진다.\n\n\
         exception `해외 결제`\n\n\
         ## 해외 결제의 처리\n\n\
         `해외 결제` 는 여기서 다룬다.\n\n\
         cover `해외 결제`\n",
    );
}

/// 인덱스 한 줄을 `종류`·`rev`·`주소` 로 가른다.
///
/// 주소가 마지막이므로 이름에 탭이 있어도 온전히 남는다.
///
/// # 매개변수
/// - `줄`: 인덱스 한 줄
///
/// # 반환값
/// `(종류, rev, 주소)`
fn 인덱스_줄(줄: &str) -> (String, String, String) {
    let mut 조각 = 줄.splitn(3, '\t');
    (
        조각.next().expect("종류가 있어야 한다").to_string(),
        조각.next().expect("rev 가 있어야 한다").to_string(),
        조각.next().expect("주소가 있어야 한다").to_string(),
    )
}

#[test]
fn 인덱스가_세_종류와_계층을_모두_낸다() {
    let root = 임시_루트("인덱스_세_종류");
    git_저장소로(&root);
    인덱스_픽스처(&root);

    let (stdout, stderr, 코드) = 실행(&root, &["index", ".kang/index.tsv"]);
    assert_eq!(코드, 0, "정상 프로젝트의 인덱스는 성공해야 한다: {stderr}");
    assert_eq!(stdout, "", "인덱스는 파일로 쓰고 stdout 은 비운다");

    let 내용 = fs::read_to_string(root.join(".kang/index.tsv")).expect("인덱스 파일이 있어야 한다");
    let 줄들: Vec<(String, String, String)> = 내용.lines().map(인덱스_줄).collect();

    let 주소들: Vec<&str> = 줄들.iter().map(|(_, _, a)| a.as_str()).collect();
    for 기대 in [
        "docs/a.결제",
        "docs/a.결제수단",
        "docs/a.결제수단.카드",
        "docs/a#결제의 기본",
        "docs/a#해외 결제의 처리",
        "docs/a!해외 결제",
    ] {
        assert!(
            주소들.contains(&기대),
            "{기대} 가 인덱스에 있어야 한다: {주소들:?}"
        );
    }

    // 종류가 주소의 구분자와 일치해야 한다 — 소비자가 둘을 교차 검증한다.
    for (종류, rev, 주소) in &줄들 {
        assert_eq!(rev.len(), 6, "rev 는 6자리다: {rev}");
        assert!(
            rev.chars().all(|c| c.is_ascii_hexdigit()),
            "rev 는 hex 다: {rev}"
        );
        let 기대_종류 = if 주소.contains('#') {
            "topic"
        } else if 주소.contains('!') {
            "exception"
        } else {
            "keyword"
        };
        assert_eq!(종류, 기대_종류, "{주소} 의 종류가 어긋난다");
    }
    정리(&root);
}

#[test]
fn 인덱스가_낸_주소를_다른_명령이_받는다() {
    let root = 임시_루트("인덱스_주소_왕복");
    git_저장소로(&root);
    인덱스_픽스처(&root);
    실행(&root, &["index", ".kang/index.tsv"]);

    let 내용 = fs::read_to_string(root.join(".kang/index.tsv")).expect("인덱스가 있어야 한다");
    // 인덱스가 낸 모든 주소를 그것을 받는 명령에 넣는다. 하나라도 거절되면
    // 매크로가 인덱스에서 읽은 주소로 bless 명령을 만들 수 없다.
    for 줄 in 내용.lines() {
        let (종류, _, 주소) = 인덱스_줄(줄);
        match 종류.as_str() {
            // keyword 주소는 refs 가 받는다.
            "keyword" => {
                let (_, stderr, 코드) = 실행(&root, &["refs", &주소]);
                assert_eq!(코드, 0, "refs 가 {주소} 를 받아야 한다: {stderr}");
            }
            // topic 주소는 show 가 받는다.
            "topic" => {
                let (_, stderr, 코드) = 실행(&root, &["show", &주소]);
                assert_eq!(코드, 0, "show 가 {주소} 를 받아야 한다: {stderr}");
            }
            // exception 주소는 조회 명령이 없다. bless 의 주소 파서가 받는지만 본다.
            //
            // 종료 코드로는 가릴 수 없다 — bless 는 "주소 형식이 틀렸다" 와 "그 문서에
            // 이 import 가 없다" 에 **같은 2** 를 쓴다. 그래서 메시지로 가른다: 주소가
            // 파싱되지 않으면 사용법과 함께 "심볼 주소가 아닙니다" 가 나온다.
            "exception" => {
                let (_, stderr, _) = 실행(&root, &["bless", "docs/a", "--import", &주소]);
                assert!(
                    !stderr.contains("심볼 주소가 아닙니다"),
                    "bless 가 {주소} 를 주소로 읽어야 한다: {stderr}"
                );
                assert!(
                    stderr.contains("이 import 가 없습니다"),
                    "주소는 읽혔고 그 문서에 해당 import 가 없는 상태여야 한다: {stderr}"
                );
            }
            다른 => panic!("모르는 종류: {다른}"),
        }
    }
    정리(&root);
}

#[test]
fn 인덱스의_핀이_build_가_요구하는_것과_같다() {
    let root = 임시_루트("인덱스_핀_일치");
    git_저장소로(&root);
    인덱스_픽스처(&root);
    실행(&root, &["index", ".kang/index.tsv"]);

    let 내용 = fs::read_to_string(root.join(".kang/index.tsv")).expect("인덱스가 있어야 한다");
    let 결제_핀 = 내용
        .lines()
        .map(인덱스_줄)
        .find(|(_, _, 주소)| 주소 == "docs/a.결제")
        .map(|(_, rev, _)| rev)
        .expect("docs/a.결제 가 인덱스에 있어야 한다");

    // 인덱스가 낸 핀을 손으로 적어 import 한다. build 가 통과하면 인덱스의 핀과
    // check_revs 가 비교하는 핀이 같은 것이다 — 갈리면 매크로가 거짓을 검증한다.
    쓰기(
        &root,
        "docs/b.kang",
        &format!(
            "---\ndescription: 소비\n---\n\n\
             import `docs`/`a`.`결제` rev \"{결제_핀}\"\n\n\
             ## 결제를 쓰는 곳\n\n`결제` 를 쓴다.\n"
        ),
    );
    let (_, stderr, 코드) = 실행(&root, &["build"]);
    assert_eq!(
        코드, 0,
        "인덱스의 핀으로 import 하면 통과해야 한다: {stderr}"
    );
    정리(&root);
}

#[test]
fn error_상태에서는_인덱스를_쓰지_않는다() {
    let root = 임시_루트("인덱스_error_상태");
    git_저장소로(&root);
    // 미해결 심볼 하나로 error 를 만든다.
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: 깨진 문서\n---\n\n## 정책\n\n`없는 개념` 을 쓴다.\n",
    );

    let (stdout, stderr, 코드) = 실행(&root, &["index", ".kang/index.tsv"]);
    assert_eq!(코드, 1, "error 가 있으면 종료 코드 1 이다: {stderr}");
    assert_eq!(stdout, "", "error 상태에서 stdout 은 비어야 한다");
    assert!(stderr.contains("K001"), "진단이 나와야 한다: {stderr}");
    assert!(
        !root.join(".kang/index.tsv").exists(),
        "깨진 프로젝트의 인덱스를 쓰면 매크로가 거짓을 검증한다"
    );
    정리(&root);
}

#[test]
fn 이름에_탭이_있어도_주소가_온전하다() {
    let root = 임시_루트("인덱스_이름에_탭");
    git_저장소로(&root);
    // 탭이 든 이름은 오늘 합법이고 show·refs 가 받는다. 인덱스가 그것을 깨뜨리면
    // 소비자가 필드를 넷으로 세어 조용히 오독한다 — 주소를 마지막에 두어 막는다.
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: 탭\n---\n\nkeyword `앞\t뒤`: 탭이 든 이름\n\n## 탭 쓰는 곳\n\n`앞\t뒤` 를 쓴다.\n",
    );
    실행(&root, &["index", ".kang/index.tsv"]);

    let 내용 = fs::read_to_string(root.join(".kang/index.tsv")).expect("인덱스가 있어야 한다");
    let 탭_줄 = 내용
        .lines()
        .find(|줄| 줄.contains("앞\t뒤"))
        .expect("탭이 든 이름이 인덱스에 있어야 한다");
    let (종류, rev, 주소) = 인덱스_줄(탭_줄);
    assert_eq!(종류, "keyword");
    assert_eq!(rev.len(), 6);
    assert_eq!(주소, "docs/a.앞\t뒤", "주소가 탭까지 온전해야 한다");

    // 그 주소를 refs 가 그대로 받는다.
    let (_, stderr, 코드) = 실행(&root, &["refs", &주소]);
    assert_eq!(코드, 0, "탭이 든 주소도 refs 가 받아야 한다: {stderr}");
    정리(&root);
}

#[test]
fn 인덱스는_쓰지_못하면_옛_인덱스를_지키며_시끄럽게_실패한다() {
    let root = 임시_루트("인덱스_쓰기_실패");
    git_저장소로(&root);
    인덱스_픽스처(&root);

    // 먼저 성공한 인덱스를 만들어 둔다. **이것이 원자성에 하중을 싣는 자리다** —
    // 읽기 전용 디렉토리에서 원자적 쓰기는 임시 파일을 못 만들어 실패하지만, 제자리
    // 쓰기로 바꾸면 기존 파일을 덮어써 **성공한다**. 옛 인덱스의 바이트를 단언하지
    // 않으면 "실패가 시끄럽다" 만 재고 "원자적이다" 는 재지 못한다.
    실행(&root, &["index", ".kang/index.tsv"]);
    let 옛_인덱스 = fs::read(root.join(".kang/index.tsv")).expect("첫 인덱스는 성공해야 한다");
    assert!(
        !옛_인덱스.is_empty(),
        "첫 인덱스가 비어 있으면 시험이 성립하지 않는다"
    );

    // 임시 파일을 만들 수 없게 대상 디렉토리를 읽기 전용으로 만든다.
    // tests/check.rs 가 이미 쓰는 방식이며 임시 파일 이름에 의존하지 않는다.
    let 잠금 = root.join(".kang");
    let 원래 = fs::metadata(&잠금).expect("메타데이터").permissions();
    let mut 읽기전용 = 원래.clone();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        읽기전용.set_mode(0o555);
    }
    fs::set_permissions(&잠금, 읽기전용).expect("권한을 바꿀 수 있어야 한다");

    let (stdout, stderr, 코드) = 실행(&root, &["index", ".kang/index.tsv"]);
    fs::set_permissions(&잠금, 원래).expect("권한을 되돌릴 수 있어야 한다");

    assert_eq!(코드, 2, "쓰기 실패는 환경 오류다: {stderr}");
    assert_eq!(stdout, "", "실패했으면 stdout 은 비어야 한다");

    // 원자성의 실체: 실패한 재작성이 옛 인덱스를 건드리지 않았다.
    let 지금 = fs::read(root.join(".kang/index.tsv")).expect("옛 인덱스가 남아야 한다");
    assert_eq!(
        지금, 옛_인덱스,
        "실패한 쓰기가 옛 인덱스를 바꿨다 — 원자적이지 않다"
    );

    let 잔여: Vec<String> = fs::read_dir(root.join(".kang"))
        .expect("디렉토리를 읽을 수 있어야 한다")
        .map(|e| e.expect("항목").file_name().to_string_lossy().into_owned())
        .filter(|이름| 이름 != "index.tsv")
        .collect();
    assert!(잔여.is_empty(), "임시 파일이 남으면 안 된다: {잔여:?}");
    정리(&root);
}
