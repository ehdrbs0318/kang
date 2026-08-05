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
    for 명령 in ["kang init", "kang bless", "kang inspect"] {
        assert!(
            미구현_절.contains(명령),
            "{명령} 이 미구현으로 표시되지 않았다: {stdout}"
        );
    }
    // 구현된 명령이 미구현 목록에 남아 있으면 에이전트가 쓸 수 있는 명령을 쓰지 않는다.
    assert!(
        !미구현_절.contains("kang show <"),
        "kang show 가 아직 미구현으로 표시되어 있다: {stdout}"
    );
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
