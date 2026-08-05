// `kang` 바이너리를 실제로 실행해 서브커맨드 디스패치·출력·종료 코드를 검증하는 통합 테스트.
//
// 통합 테스트 크레이트는 서로 격리되므로 `tests/check.rs` 의 임시 저장소 헬퍼를 그대로
// 가져다 쓸 수 없다. 격리 전략(프로세스 id + 테스트 이름)만 복제하고, 이 파일이 쓰지 않는
// 헬퍼(문서경로·위치들·코드들 등)는 옮기지 않는다.
//
// **라이브러리를 부르지 않고 바이너리를 부른다.** 진단 함수가 `compile()` 에 연결되지
// 않았을 때 그것을 잡는 것이 이 파일의 목적이므로, 단위 호출로 대신하면 의미가 없다.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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

/// 통과하지 못한 문서는 어떤 CLI 명령으로도 출력되지 않는다 (스펙 5절).
#[test]
fn 에러가_있으면_list_가_아무것도_출력하지_않는다() {
    let root = 임시_루트("list-err");
    git_저장소로(&root);
    에러_문서(&root);

    let (stdout, stderr, 코드) = 실행(&root, &["list"]);

    assert_eq!(코드, 1, "stderr: {stderr}");
    assert_eq!(stdout, "", "error 가 있으면 문서를 한 줄도 내지 않는다");
    assert!(stderr.contains("K001"), "{stderr}");
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
    // 종료 코드 네 가지.
    assert!(stdout.contains("종료 코드"), "{stdout}");
    for 코드값 in ["0", "1", "2", "3"] {
        assert!(stdout.contains(코드값), "{stdout}");
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
