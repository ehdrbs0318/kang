// `resolve::find_root` / `resolve::load` / `resolve::SymbolTable` 과
// `check::check_cycles` / `check::check_symbols` / `check::report` 를 검증하는 통합 테스트.
//
// 이 파일의 테스트는 **실제 파일 시스템**을 쓴다. 각 테스트는 자기만의 임시 디렉토리를
// 만들고 그 안에서만 움직이므로 `cargo test` 의 기본 병렬 실행에서도 서로를 밟지 않는다.
//
// 진단 검사는 **문자 단위가 아니라 구조 일치**를 본다 — `code` 값, `locations` 개수와
// 각 `note` 유무, `fixes` 의 종류·순서, 셸 명령의 인용 여부다. 스펙 5.1.1 예시에 박힌
// 경로와 줄 번호까지 맞추면 스펙의 오타를 고칠 때마다 테스트가 깨진다.
use kang::ast::{Diagnostic, DocPath, Fix, FixKind, Location, Severity, SymbolKind, SymbolRef};
use kang::check;
use kang::resolve;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// 테스트 하나가 독점하는 임시 디렉토리를 만든다.
///
/// 경로에 프로세스 id 와 테스트 이름을 함께 넣는다. 테스트 이름이 같은 실행 안의
/// 병렬 실행을, 프로세스 id 가 동시에 두 번 돌리는 경우를 각각 막는다.
///
/// **패닉으로 남은 디렉토리는 다음 실행이 회수하지 못한다** — 프로세스 id 가 매번
/// 달라 같은 경로를 다시 만나지 않기 때문이다. 그래도 만들기 전에 지우는 것은
/// 운영체제가 id 를 재사용했을 때 남의 찌꺼기 위에 쌓지 않기 위해서다.
/// 남는 것은 `$TMPDIR` 의 작은 파일 몇 개이고 운영체제가 회수하므로,
/// 실행 간 격리를 포기하는 것보다 이쪽을 감수한다.
///
/// # 매개변수
/// - `이름`: 테스트를 구분하는 이름
///
/// # 반환값
/// 갓 만들어진 빈 디렉토리 경로
fn 임시_루트(이름: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kang-check-{}-{}", std::process::id(), 이름));
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

/// 문자열 조각들로 [`DocPath`] 를 만든다.
///
/// # 매개변수
/// - `조각들`: 경로 조각들
///
/// # 반환값
/// 만들어진 [`DocPath`]
fn 문서경로(조각들: &[&str]) -> DocPath {
    DocPath(조각들.iter().map(|조각| 조각.to_string()).collect())
}

/// 프로젝트를 읽어 순환 검사만 돌린다.
///
/// 로드 단계에서 진단이 나오면 픽스처가 잘못된 것이므로 여기서 잡는다 —
/// 그것을 그대로 두면 순환 진단이 없는 이유를 픽스처 오타에서 찾게 된다.
///
/// # 매개변수
/// - `root`: 프로젝트 루트
///
/// # 반환값
/// [`check::check_cycles`] 가 낸 진단들
fn 순환_검사(root: &Path) -> Vec<Diagnostic> {
    let (project, 로드_진단) = resolve::load(root);
    assert!(로드_진단.is_empty(), "{로드_진단:?}");
    check::check_cycles(&project)
}

/// 진단이 가리키는 위치를 `(문서 경로, 줄 번호)` 목록으로 펼친다.
/// 순환 체인의 **순서와 길이**를 그대로 단언하기 위한 것이다.
///
/// # 매개변수
/// - `diagnostic`: 펼칠 진단
///
/// # 반환값
/// 위치마다 문서 경로 문자열과 줄 번호를 담은 목록
fn 위치들(diagnostic: &Diagnostic) -> Vec<(String, usize)> {
    diagnostic
        .locations
        .iter()
        .map(|location| (location.doc.to_string(), location.line))
        .collect()
}

/// git 저장소 루트가 곧 프로젝트 루트여야 한다 (스펙 3절).
#[test]
fn git_루트를_프로젝트_루트로_찾는다() {
    let root = 임시_루트("git-root");
    git_저장소로(&root);

    let 찾은 = resolve::find_root(&root).expect("git 저장소 안이므로 루트를 찾아야 한다");

    assert_eq!(찾은, root);
    정리(&root);
}

/// 어느 하위 디렉토리에서 실행해도 루트와 문서 경로가 같아야 한다 (스펙 3절).
#[test]
fn 하위_디렉토리에서_실행해도_docpath_가_같다() {
    let root = 임시_루트("subdir");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/details/payment.kang",
        "---\ndescription: 결제\n---\n",
    );

    let 하위 = root.join("docs/details");
    let 하위에서 = resolve::find_root(&하위).expect("하위 디렉토리에서도 루트를 찾아야 한다");
    let 루트에서 = resolve::find_root(&root).expect("루트에서도 루트를 찾아야 한다");

    assert_eq!(하위에서, 루트에서);
    let (project, diagnostics) = resolve::load(&하위에서);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(
        project
            .docs
            .contains_key(&문서경로(&["docs", "details", "payment"]))
    );
    정리(&root);
}

/// worktree 와 submodule 의 `.git` 은 디렉토리가 아니라 **파일**이다.
/// 종류를 물으면 그 저장소들이 통째로 `K050` 을 맞는다 — 합법 입력 거부다.
#[test]
fn 파일_형태의_git_도_저장소로_인정한다() {
    let root = 임시_루트("git-file");
    fs::write(root.join(".git"), "gitdir: /어딘가/.git/worktrees/x\n")
        .expect("파일을 쓸 수 있어야 한다");

    let 찾은 = resolve::find_root(&root).expect("`.git` 파일도 저장소 표시다");

    assert_eq!(찾은, root);
    정리(&root);
}

/// 상대 경로의 마지막 조상은 빈 경로이고, 빈 경로의 `.git` 은 **프로세스 cwd** 기준으로
/// 해석된다. 걸러내지 않으면 cwd 가 저장소일 때 빈 루트를 `Ok` 로 돌려준다.
#[test]
fn 상대_경로로는_빈_루트를_돌려주지_않는다() {
    // cargo 는 통합 테스트의 cwd 를 패키지 루트로 두고, 그곳은 git 저장소다.
    let 결과 = resolve::find_root(Path::new("존재하지-않는-상대/경로"));

    let diagnostic = 결과.expect_err("빈 루트를 성공으로 돌려주면 안 된다");
    assert_eq!(diagnostic.code, "K050");
}

/// git 저장소가 아니면 그 사실을 진단으로 알려야 한다 (스펙 3절).
#[test]
fn git_저장소가_아니면_진단을_낸다() {
    let root = 임시_루트("no-git");

    let diagnostic = resolve::find_root(&root).expect_err("git 저장소가 아니므로 진단이어야 한다");

    assert_eq!(diagnostic.code, "K050");
    assert_eq!(diagnostic.severity, Severity::Error);
    assert!(!diagnostic.locations.is_empty());
    assert!(!diagnostic.locations[0].note.is_empty());
    assert!(!diagnostic.fixes.is_empty());
    정리(&root);
}

/// 루트 아래 어느 깊이의 `.kang` 파일이든 전부 읽어야 한다.
#[test]
fn 하위_디렉토리의_kang_파일을_전부_읽는다() {
    let root = 임시_루트("recurse");
    git_저장소로(&root);
    쓰기(&root, "a.kang", "---\ndescription: 뿌리\n---\n");
    쓰기(&root, "docs/b.kang", "---\ndescription: 한 층\n---\n");
    쓰기(
        &root,
        "docs/details/c.kang",
        "---\ndescription: 두 층\n---\n",
    );

    let (project, diagnostics) = resolve::load(&root);

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(project.docs.len(), 3);
    assert!(project.docs.contains_key(&문서경로(&["a"])));
    assert!(project.docs.contains_key(&문서경로(&["docs", "b"])));
    assert!(
        project
            .docs
            .contains_key(&문서경로(&["docs", "details", "c"]))
    );
    정리(&root);
}

/// `.kang` 이 아닌 파일은 읽지도 진단하지도 않아야 한다.
#[test]
fn kang_이_아닌_파일은_무시한다() {
    let root = 임시_루트("ext");
    git_저장소로(&root);
    쓰기(&root, "readme.md", "# 이건 kang 문서가 아니다");
    쓰기(&root, "docs/notes.txt", "frontmatter 도 없다");
    쓰기(&root, "docs/kang", "확장자가 아니라 이름이 kang 이다");
    쓰기(&root, "docs/a.kang", "---\ndescription: 진짜 문서\n---\n");

    let (project, diagnostics) = resolve::load(&root);

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(project.docs.len(), 1);
    assert!(project.docs.contains_key(&문서경로(&["docs", "a"])));
    정리(&root);
}

/// 자기 파일이 선언한 keyword·topic·exception 이 스코프에 있어야 한다.
#[test]
fn 자기_파일의_심볼을_스코프에서_찾는다() {
    let root = 임시_루트("own-scope");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: 결제 정책\n---\n\nkeyword `결제`: 대금을 지불하는 행위\n\n## 결제의 방법\n\n사용자는 `결제` 를 한다.\n\nexception `무료 상품`\n",
    );

    let (project, 로드_진단) = resolve::load(&root);
    assert!(로드_진단.is_empty(), "{로드_진단:?}");
    let (table, 진단) = resolve::SymbolTable::build(&project);
    assert!(진단.is_empty(), "{진단:?}");

    let scope = table.scope(&문서경로(&["docs", "a"]));
    let 결제 = *scope
        .get("결제")
        .expect("자기 keyword 가 스코프에 있어야 한다");
    let 방법 = *scope
        .get("결제의 방법")
        .expect("자기 topic 이 스코프에 있어야 한다");
    let 무료 = *scope
        .get("무료 상품")
        .expect("자기 exception 이 스코프에 있어야 한다");

    assert_eq!(table.owner(결제), &문서경로(&["docs", "a"]));
    assert_eq!(table.owner(방법), &문서경로(&["docs", "a"]));
    assert_eq!(table.owner(무료), &문서경로(&["docs", "a"]));
    정리(&root);
}

/// import 한 alias 로 남의 심볼을 스코프에서 찾을 수 있어야 한다 (스펙 4.7).
#[test]
fn import_한_alias_를_스코프에서_찾는다() {
    let root = 임시_루트("alias-scope");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: 결제 정책\n---\n\nkeyword `결제`: 대금을 지불하는 행위\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: 청구 정책\n---\n\nimport `docs`/`a`.`결제` as `A 결제`\n\n## 청구의 방법\n\n`A 결제` 뒤에 청구한다.\n",
    );

    let (project, 로드_진단) = resolve::load(&root);
    assert!(로드_진단.is_empty(), "{로드_진단:?}");
    let (table, 진단) = resolve::SymbolTable::build(&project);
    assert!(진단.is_empty(), "{진단:?}");

    let scope = table.scope(&문서경로(&["docs", "b"]));
    let 별칭 = *scope.get("A 결제").expect("alias 가 스코프에 있어야 한다");

    // alias 는 이름만 바꿀 뿐 owner 는 선언한 파일 그대로다.
    assert_eq!(table.owner(별칭), &문서경로(&["docs", "a"]));
    assert_eq!(table.hash_source(별칭), "대금을 지불하는 행위");
    // 정본 이름은 남의 스코프로 새지 않는다.
    assert!(!scope.contains_key("결제"));
    정리(&root);
}

/// 서로 다른 파일이 선언한 같은 이름을 `by_name` 이 모아야 한다 (스펙 5.1).
#[test]
fn 같은_이름_심볼을_by_name_으로_모은다() {
    let root = 임시_루트("by-name");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nkeyword `금액`: 청구되는 원화 액수\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: B\n---\n\nkeyword `금액`: 환불되는 원화 액수\n",
    );
    쓰기(
        &root,
        "docs/c.kang",
        "---\ndescription: C\n---\n\nkeyword `금액`: 적립되는 원화 액수\n",
    );

    let (project, 로드_진단) = resolve::load(&root);
    assert!(로드_진단.is_empty(), "{로드_진단:?}");
    let (table, _) = resolve::SymbolTable::build(&project);

    let 같은_이름 = table.by_name("금액");

    assert_eq!(같은_이름.len(), 3);
    let mut owners: Vec<String> = 같은_이름
        .iter()
        .map(|&id| table.owner(id).to_string())
        .collect();
    owners.sort();
    assert_eq!(owners, vec!["docs/a", "docs/b", "docs/c"]);
    정리(&root);
}

/// 문서가 하나도 없는 프로젝트는 문서도 진단도 없어야 한다.
#[test]
fn 빈_프로젝트는_문서도_진단도_없다() {
    let root = 임시_루트("empty");
    git_저장소로(&root);

    let (project, diagnostics) = resolve::load(&root);

    assert!(project.docs.is_empty());
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    정리(&root);
}

/// `.git` 같은 숨은 디렉토리는 순회하지 않아야 한다.
/// 저장소 내부를 훑는 것은 낭비이고, 그 안의 파일은 사용자의 문서가 아니다.
#[test]
fn 숨은_디렉토리는_순회하지_않는다() {
    let root = 임시_루트("hidden");
    git_저장소로(&root);
    쓰기(&root, ".git/x.kang", "이건 문서가 아니다");
    쓰기(&root, ".hidden/y.kang", "이것도 문서가 아니다");
    쓰기(&root, "docs/a.kang", "---\ndescription: 진짜 문서\n---\n");

    let (project, diagnostics) = resolve::load(&root);

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(project.docs.len(), 1);
    assert!(project.docs.contains_key(&문서경로(&["docs", "a"])));
    정리(&root);
}

/// BOM 은 디코딩 아티팩트지 문법 요소가 아니므로 로더가 벗겨야 한다.
/// 벗기지 않으면 frontmatter 여는 `---` 을 못 알아보고 `K101` 오진이 난다.
#[test]
fn bom_이_있어도_frontmatter_를_읽는다() {
    let root = 임시_루트("bom");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "\u{feff}---\ndescription: 결제 정책\n---\n",
    );

    let (project, diagnostics) = resolve::load(&root);

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(
        project.docs[&문서경로(&["docs", "a"])].description,
        "결제 정책"
    );
    정리(&root);
}

/// 읽기 권한이 없는 파일은 그 사실을 진단으로 알리고 나머지 파일은 계속 읽어야 한다.
/// 조용히 넘기면 사용자는 문서가 없는 것과 구분할 수 없다.
#[test]
#[cfg(unix)]
fn 읽을_수_없는_파일은_진단으로_바꾸고_계속한다() {
    use std::os::unix::fs::PermissionsExt;

    let root = 임시_루트("no-perm");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/locked.kang",
        "---\ndescription: 잠긴 문서\n---\n",
    );
    쓰기(
        &root,
        "docs/open.kang",
        "---\ndescription: 열린 문서\n---\n",
    );
    let 잠긴 = root.join("docs/locked.kang");
    fs::set_permissions(&잠긴, fs::Permissions::from_mode(0o000))
        .expect("권한을 바꿀 수 있어야 한다");

    // root 로 실행하면 권한 비트가 무시되어 이 시나리오 자체가 성립하지 않는다.
    // 읽히지 않는 것을 확인한 뒤에만 진단을 요구한다.
    if fs::read(&잠긴).is_ok() {
        정리(&root);
        return;
    }

    let (project, diagnostics) = resolve::load(&root);

    assert_eq!(project.docs.len(), 1);
    assert!(project.docs.contains_key(&문서경로(&["docs", "open"])));
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K051");
    assert_eq!(diagnostics[0].severity, Severity::Error);
    assert_eq!(
        diagnostics[0].locations[0].doc,
        문서경로(&["docs", "locked"])
    );
    assert!(!diagnostics[0].locations[0].note.is_empty());
    assert!(!diagnostics[0].fixes.is_empty());

    let _ = fs::set_permissions(&잠긴, fs::Permissions::from_mode(0o644));
    정리(&root);
}

/// 셸 fix 는 경로를 인용해야 한다 (스펙 5.1.1·6.0·6.1).
/// 인용하지 않으면 공백이 든 경로에서 셸이 인자를 쪼개, 원인을 진단하는 대신
/// **새로운 잘못된 사실**을 준다. 에이전트에게 인용 판단을 맡기면 틀린다.
#[test]
#[cfg(unix)]
fn 셸_fix_는_공백이_든_경로를_인용한다() {
    use std::os::unix::fs::PermissionsExt;

    // 경로에 공백을 넣는 것이 이 테스트의 전부다.
    let root = 임시_루트("quote test");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/locked.kang",
        "---\ndescription: 잠긴 문서\n---\n",
    );
    let 잠긴 = root.join("docs/locked.kang");
    fs::set_permissions(&잠긴, fs::Permissions::from_mode(0o000))
        .expect("권한을 바꿀 수 있어야 한다");

    if fs::read(&잠긴).is_ok() {
        정리(&root);
        return;
    }

    let (_, diagnostics) = resolve::load(&root);

    assert_eq!(diagnostics.len(), 1);
    let fix = &diagnostics[0].fixes[0];
    assert_eq!(fix.kind, FixKind::Shell);
    // 인용된 경로가 통째로 들어 있어야 한다.
    assert!(
        fix.action.contains(&format!("'{}'", 잠긴.display())),
        "인용되지 않았다: {}",
        fix.action
    );

    let _ = fs::set_permissions(&잠긴, fs::Permissions::from_mode(0o644));
    정리(&root);
}

/// 읽을 수 없는 디렉토리는 진단으로 알리고 형제 문서는 계속 읽어야 한다.
/// 진단 순서도 실행마다 같아야 한다 — `수집` 은 `files.sort()` 이전에 돈다.
#[test]
#[cfg(unix)]
fn 읽을_수_없는_디렉토리는_진단으로_바꾸고_계속한다() {
    use std::os::unix::fs::PermissionsExt;

    let root = 임시_루트("locked-dir");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/open.kang",
        "---\ndescription: 열린 문서\n---\n",
    );
    let 가 = root.join("docs/a-locked");
    let 나 = root.join("docs/b-locked");
    fs::create_dir_all(&가).expect("디렉토리를 만들 수 있어야 한다");
    fs::create_dir_all(&나).expect("디렉토리를 만들 수 있어야 한다");
    fs::set_permissions(&가, fs::Permissions::from_mode(0o000))
        .expect("권한을 바꿀 수 있어야 한다");
    fs::set_permissions(&나, fs::Permissions::from_mode(0o000))
        .expect("권한을 바꿀 수 있어야 한다");

    // root 로 실행하면 권한 비트가 무시되어 이 시나리오가 성립하지 않는다.
    if fs::read_dir(&가).is_ok() {
        let _ = fs::set_permissions(&가, fs::Permissions::from_mode(0o755));
        let _ = fs::set_permissions(&나, fs::Permissions::from_mode(0o755));
        정리(&root);
        return;
    }

    let (project, diagnostics) = resolve::load(&root);

    // 형제 문서는 살아 있다.
    assert_eq!(project.docs.len(), 1);
    assert!(project.docs.contains_key(&문서경로(&["docs", "open"])));
    assert_eq!(diagnostics.len(), 2);
    assert!(diagnostics.iter().all(|d| d.code == "K051"));
    // 디렉토리는 문서가 아니므로 전체 경로를 조각 **하나**에 담는다.
    assert_eq!(diagnostics[0].locations[0].doc.0.len(), 1);
    assert!(diagnostics[0].locations[0].doc.0[0].ends_with("a-locked"));
    // 정렬되어 실행마다 같은 순서다.
    assert!(diagnostics[0].locations[0].doc.0[0] < diagnostics[1].locations[0].doc.0[0]);
    assert_eq!(diagnostics[0].fixes[0].kind, FixKind::Shell);

    let _ = fs::set_permissions(&가, fs::Permissions::from_mode(0o755));
    let _ = fs::set_permissions(&나, fs::Permissions::from_mode(0o755));
    정리(&root);
}

/// UTF-8 이 아닌 파일은 그 파일에 대한 진단으로 바꾸고 나머지 파일은 계속 읽어야 한다.
#[test]
fn utf8_이_아닌_파일은_진단으로_바꾸고_계속한다() {
    let root = 임시_루트("bad-utf8");
    git_저장소로(&root);
    // 0xff 는 UTF-8 어느 자리에도 올 수 없는 바이트다.
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("docs/bad.kang"), [0xffu8, 0xfe, 0xfd]).unwrap();
    쓰기(
        &root,
        "docs/good.kang",
        "---\ndescription: 멀쩡한 문서\n---\n",
    );

    let (project, diagnostics) = resolve::load(&root);

    // 깨진 파일 하나가 나머지를 죽이지 않는다.
    assert_eq!(project.docs.len(), 1);
    assert!(project.docs.contains_key(&문서경로(&["docs", "good"])));
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K051");
    assert_eq!(diagnostics[0].severity, Severity::Error);
    assert_eq!(diagnostics[0].locations[0].doc, 문서경로(&["docs", "bad"]));
    assert!(!diagnostics[0].locations[0].note.is_empty());
    // 텍스트로 열 수 없는 파일이므로 편집이 아니라 셸 변환이 fix 다.
    // 형제 진단인 읽기 실패와 같은 종류여야 한 코드가 한 모양을 갖는다.
    assert_eq!(diagnostics[0].fixes[0].kind, FixKind::Shell);
    assert_eq!(diagnostics[0].fixes[0].doc, None);
    assert!(
        diagnostics[0].fixes[0]
            .action
            .contains(&format!("'{}'", root.join("docs/bad.kang").display()))
    );
    정리(&root);
}

/// 파싱에 실패한 문서는 진단만 남기고 프로젝트에서 빠져야 한다 (스펙 5절).
#[test]
fn 파싱에_실패한_문서는_프로젝트에서_빠진다() {
    let root = 임시_루트("parse-fail");
    git_저장소로(&root);
    쓰기(&root, "docs/bad.kang", "frontmatter 가 없다\n");
    쓰기(
        &root,
        "docs/good.kang",
        "---\ndescription: 멀쩡한 문서\n---\n",
    );

    let (project, diagnostics) = resolve::load(&root);

    assert_eq!(project.docs.len(), 1);
    assert!(project.docs.contains_key(&문서경로(&["docs", "good"])));
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "K101");
    assert_eq!(diagnostics[0].locations[0].doc, 문서경로(&["docs", "bad"]));
    정리(&root);
}

/// 해시 입력은 세 종류 모두에 있고, exception 은 자기를 선언한 topic 의 본문이다 (스펙 4.8).
#[test]
fn hash_source_는_세_종류_모두에_값이_있다() {
    let root = 임시_루트("hash-source");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: 결제 정책\n---\n\nkeyword `결제`: 대금을 지불하는 행위\n\n## 결제의 방법\n\n사용자는 `결제` 를 한다.\n\nexception `무료 상품`\nexception `해외 결제` pending\n",
    );

    let (project, 로드_진단) = resolve::load(&root);
    assert!(로드_진단.is_empty(), "{로드_진단:?}");
    let (table, _) = resolve::SymbolTable::build(&project);
    let scope = table.scope(&문서경로(&["docs", "a"]));

    // keyword 의 해시 입력은 한 줄 정의 텍스트다.
    assert_eq!(table.hash_source(scope["결제"]), "대금을 지불하는 행위");
    // topic 의 해시 입력은 헤딩을 포함한 본문이며 선언 줄은 빠진다.
    let 본문 = table.hash_source(scope["결제의 방법"]);
    assert!(본문.starts_with("## 결제의 방법"));
    assert!(본문.contains("사용자는 `결제` 를 한다."));
    assert!(!본문.contains("exception"));
    // exception 의 해시 입력은 그것을 선언한 topic 의 본문이다.
    // 한 topic 의 예외 여럿은 전부 같은 해시를 갖는다.
    assert_eq!(table.hash_source(scope["무료 상품"]), 본문);
    assert_eq!(table.hash_source(scope["해외 결제"]), 본문);
    정리(&root);
}

/// 이름 충돌 판정은 계층 전체 경로 기준이다 —
/// `결제`.`상태` 와 `구독`.`상태` 는 서로 다른 이름이다 (스펙 4.3).
#[test]
fn 계층이_다른_같은_말단_이름은_충돌이_아니다() {
    let root = 임시_루트("hierarchy");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nkeyword `결제`: 대금을 지불하는 행위\nkeyword `결제`.`상태`: 결제의 진행 단계\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: B\n---\n\nkeyword `구독`: 반복 결제 계약\nkeyword `구독`.`상태`: 구독의 진행 단계\n",
    );

    let (project, 로드_진단) = resolve::load(&root);
    assert!(로드_진단.is_empty(), "{로드_진단:?}");
    let (table, 진단) = resolve::SymbolTable::build(&project);

    assert!(진단.is_empty(), "{진단:?}");
    assert_eq!(table.by_name("결제.상태").len(), 1);
    assert_eq!(table.by_name("구독.상태").len(), 1);
    // 말단 이름만으로는 어느 쪽도 잡히지 않는다.
    assert!(table.by_name("상태").is_empty());
    정리(&root);
}

/// 한 문서가 같은 로컬 이름을 두 번 쓰면 그 이름의 참조가 어느 쪽인지 정할 수 없다.
#[test]
fn 한_문서가_같은_이름을_두_번_선언하면_진단을_낸다() {
    let root = 임시_루트("dup-name");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nkeyword `금액`: 청구되는 원화 액수\nkeyword `금액`: 환불되는 원화 액수\n",
    );

    let (project, 로드_진단) = resolve::load(&root);
    assert!(로드_진단.is_empty(), "{로드_진단:?}");
    let (_, 진단) = resolve::SymbolTable::build(&project);

    assert_eq!(진단.len(), 1);
    assert_eq!(진단[0].code, "K052");
    assert_eq!(진단[0].severity, Severity::Error);
    // 두 선언 줄을 모두 가리켜야 사용자가 어디를 고칠지 안다.
    assert_eq!(진단[0].locations.len(), 2);
    assert_eq!(진단[0].locations[0].line, 5);
    assert_eq!(진단[0].locations[1].line, 6);
    assert!(!진단[0].fixes.is_empty());
    정리(&root);
}

/// import 로 들어온 이름과 자기 선언이 겹쳐도 같은 진단이다.
/// 진단이 가리키는 줄은 **파일에 나타난 순서**여야 한다 — 컴파일러가 자기 선언을
/// 먼저 도는 것은 내부 사정이고, 스펙 4.7 은 import 를 파일 최상단에 두라고 한다.
#[test]
fn import_와_선언이_같은_이름이면_진단을_낸다() {
    let root = 임시_루트("import-vs-own");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nkeyword `금액`: 청구되는 원화 액수\n",
    );
    // 5행에 import, 7행에 keyword — 스펙이 권하는 흔한 배치다.
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: B\n---\n\nimport `docs`/`a`.`금액`\n\nkeyword `금액`: 환불되는 원화 액수\n",
    );

    let (project, 로드_진단) = resolve::load(&root);
    assert!(로드_진단.is_empty(), "{로드_진단:?}");
    let (_, 진단) = resolve::SymbolTable::build(&project);

    assert_eq!(진단.len(), 1);
    assert_eq!(진단[0].code, "K052");
    assert_eq!(진단[0].locations.len(), 2);
    // 5 → 7 이지 7 → 5 가 아니다.
    assert_eq!(진단[0].locations[0].line, 5);
    assert_eq!(진단[0].locations[1].line, 7);
    정리(&root);
}

/// 두 import 가 같은 별칭을 쓰면 뒤엣것이 앞엣것을 조용히 덮으면 안 된다.
#[test]
fn 같은_별칭을_두_번_쓰면_진단을_낸다() {
    let root = 임시_루트("dup-alias");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nkeyword `결제`: 대금을 지불하는 행위\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: B\n---\n\nkeyword `청구`: 대금을 청구하는 행위\n",
    );
    쓰기(
        &root,
        "docs/c.kang",
        "---\ndescription: C\n---\n\nimport `docs`/`a`.`결제` as `공통`\nimport `docs`/`b`.`청구` as `공통`\n",
    );

    let (project, 로드_진단) = resolve::load(&root);
    assert!(로드_진단.is_empty(), "{로드_진단:?}");
    let (table, 진단) = resolve::SymbolTable::build(&project);

    assert_eq!(진단.len(), 1);
    assert_eq!(진단[0].code, "K052");
    assert_eq!(진단[0].locations[0].line, 5);
    assert_eq!(진단[0].locations[1].line, 6);
    // 먼저 묶인 쪽이 남는다. 뒤엣것이 덮지 않는다.
    let scope = table.scope(&문서경로(&["docs", "c"]));
    assert_eq!(table.owner(scope["공통"]), &문서경로(&["docs", "a"]));
    정리(&root);
}

/// 서로 다른 파일이 같은 이름을 선언하는 것은 이 층의 진단이 아니다 —
/// `iknow` 로 합법이 되므로 상호성 검사를 하는 층이 판정한다 (스펙 4.4).
#[test]
fn 다른_파일의_같은_이름은_진단하지_않는다() {
    let root = 임시_루트("cross-file-name");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nkeyword `금액`: 청구되는 원화 액수 // iknow `docs`/`b`.`금액`\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: B\n---\n\nkeyword `금액`: 환불되는 원화 액수 // iknow `docs`/`a`.`금액`\n",
    );

    let (project, 로드_진단) = resolve::load(&root);
    assert!(로드_진단.is_empty(), "{로드_진단:?}");
    let (table, 진단) = resolve::SymbolTable::build(&project);

    assert!(진단.is_empty(), "{진단:?}");
    assert_eq!(table.by_name("금액").len(), 2);
    정리(&root);
}

/// 심볼 참조를 전역 식별자로 해석해야 한다.
#[test]
fn 심볼_참조를_전역_식별자로_해석한다() {
    let root = 임시_루트("resolve");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nkeyword `결제`.`상태`: 결제의 진행 단계\n\n## 결제의 방법\n\n본문이다.\n\nexception `무료 상품`\n",
    );

    let (project, 로드_진단) = resolve::load(&root);
    assert!(로드_진단.is_empty(), "{로드_진단:?}");
    let (table, _) = resolve::SymbolTable::build(&project);

    let 계층_keyword = SymbolRef {
        doc: 문서경로(&["docs", "a"]),
        kind: SymbolKind::Keyword,
        name: vec!["결제".to_string(), "상태".to_string()],
    };
    let topic = SymbolRef {
        doc: 문서경로(&["docs", "a"]),
        kind: SymbolKind::Topic,
        name: vec!["결제의 방법".to_string()],
    };
    let 예외 = SymbolRef {
        doc: 문서경로(&["docs", "a"]),
        kind: SymbolKind::Exception,
        name: vec!["무료 상품".to_string()],
    };
    // 종류가 다르면 같은 자리를 가리키지 않는다.
    let 없는_종류 = SymbolRef {
        doc: 문서경로(&["docs", "a"]),
        kind: SymbolKind::Topic,
        name: vec!["결제.상태".to_string()],
    };
    let 없는_문서 = SymbolRef {
        doc: 문서경로(&["docs", "z"]),
        kind: SymbolKind::Topic,
        name: vec!["결제의 방법".to_string()],
    };

    assert_eq!(
        table.resolve(&계층_keyword),
        Some(table.scope(&문서경로(&["docs", "a"]))["결제.상태"])
    );
    assert!(table.resolve(&topic).is_some());
    assert!(table.resolve(&예외).is_some());
    assert!(table.resolve(&없는_종류).is_none());
    assert!(table.resolve(&없는_문서).is_none());
    정리(&root);
}

/// alias 가 없는 import 는 대상의 정본 이름으로 스코프에 들어간다 (스펙 4.7).
#[test]
fn alias_가_없는_import_는_정본_이름으로_들어간다() {
    let root = 임시_루트("no-alias");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nkeyword `결제`.`상태`: 결제의 진행 단계\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: B\n---\n\nimport `docs`/`a`.`결제`.`상태`\n\n## B 의 정책\n\n본문이다.\n",
    );

    let (project, 로드_진단) = resolve::load(&root);
    assert!(로드_진단.is_empty(), "{로드_진단:?}");
    let (table, 진단) = resolve::SymbolTable::build(&project);
    assert!(진단.is_empty(), "{진단:?}");

    let scope = table.scope(&문서경로(&["docs", "b"]));
    let id = *scope
        .get("결제.상태")
        .expect("alias 가 없으면 정본 이름으로 들어가야 한다");

    assert_eq!(table.owner(id), &문서경로(&["docs", "a"]));
    정리(&root);
}

/// `A → B → A` 는 순환이다 (스펙 5.1·5.3).
#[test]
fn 직접_순환을_검출한다() {
    let root = 임시_루트("cycle-two");
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

    let 진단 = 순환_검사(&root);

    assert_eq!(진단.len(), 1, "{진단:?}");
    assert_eq!(진단[0].code, "K040");
    assert_eq!(진단[0].severity, Severity::Error);
    // 체인은 두 문서를 지나 시작점으로 돌아온다. 위치는 각 파일의 import 줄이다.
    assert_eq!(
        위치들(&진단[0]),
        vec![("docs/a".to_string(), 5), ("docs/b".to_string(), 5)]
    );
    assert!(
        진단[0].message.contains("docs/a → docs/b → docs/a"),
        "{}",
        진단[0].message
    );
    // note 는 그 줄이 실제로 import 하는 대상을 말해야 한다.
    assert!(진단[0].locations[0].note.contains("docs/b"), "{진단:?}");
    assert!(진단[0].locations[1].note.contains("docs/a"), "{진단:?}");
    // 순환을 닫는 문서가 고칠 대상이다.
    assert!(!진단[0].fixes.is_empty());
    assert_eq!(진단[0].fixes[0].kind, FixKind::Edit);
    assert_eq!(
        진단[0].fixes[0].doc.as_ref(),
        Some(&문서경로(&["docs", "b"]))
    );
    // Edit 수정은 문서 문법이므로 대상 경로를 백틱으로 적는다 (스펙 5.1.1) —
    // 에이전트가 파일에서 찾을 텍스트가 `` import `docs`/`a`.`결제` `` 이기 때문이다.
    assert!(
        진단[0].fixes[0].action.contains("`docs`/`a`"),
        "{}",
        진단[0].fixes[0].action
    );
    // 스펙 5.1 표가 요구하는 "공통 개념 추출 안내".
    assert!(
        진단[0].fixes[0].action.contains("상위 문서"),
        "{}",
        진단[0].fixes[0].action
    );
    정리(&root);
}

/// 순환 발견 시 체인 전체를 출력해야 한다 (스펙 5.1: "순환 체인 전체를 출력").
#[test]
fn 삼단계_순환의_체인_전체를_출력한다() {
    let root = 임시_루트("cycle-three");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nimport `docs`/`b`.`청구서`\n\nkeyword `결제`: 대금을 지불하는 행위\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: B\n---\n\nimport `docs`/`c`.`영수증`\n\nkeyword `청구서`: 청구 내역을 담은 문서\n",
    );
    쓰기(
        &root,
        "docs/c.kang",
        "---\ndescription: C\n---\n\nimport `docs`/`a`.`결제`\n\nkeyword `영수증`: 지불을 증명하는 문서\n",
    );

    let 진단 = 순환_검사(&root);

    assert_eq!(진단.len(), 1, "{진단:?}");
    assert_eq!(
        위치들(&진단[0]),
        vec![
            ("docs/a".to_string(), 5),
            ("docs/b".to_string(), 5),
            ("docs/c".to_string(), 5)
        ]
    );
    assert!(
        진단[0]
            .message
            .contains("docs/a → docs/b → docs/c → docs/a"),
        "{}",
        진단[0].message
    );
    정리(&root);
}

/// `iknow` 는 import 간선을 만들지 않으므로 상호 명시가 순환이 아니다 (스펙 4.4).
/// keyword·topic·exception 세 자리 전부를 한 번에 본다.
#[test]
fn iknow_상호_명시는_순환이_아니다() {
    let root = 임시_루트("iknow-mutual");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nkeyword `금액`: 청구되는 원화 액수 // iknow `docs`/`b`.`금액`\n\n## 정산 절차 // iknow `docs`/`b`#`정산 절차`\n\n정산은 `금액` 기준이다.\n\nexception `무료 상품` // iknow `docs`/`b`!`무료 상품`\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: B\n---\n\nkeyword `금액`: 환불되는 원화 액수 // iknow `docs`/`a`.`금액`\n\n## 정산 절차 // iknow `docs`/`a`#`정산 절차`\n\n환불도 `금액` 기준이다.\n\nexception `무료 상품` // iknow `docs`/`a`!`무료 상품`\n",
    );

    let 진단 = 순환_검사(&root);

    assert!(진단.is_empty(), "{진단:?}");
    정리(&root);
}

/// 길이 1 사이클도 순환이다. 자기 파일을 import 하는 것은 자기 자신에 의존하는 것이다.
#[test]
fn 자기_파일_import_는_순환이다() {
    let root = 임시_루트("cycle-self");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nimport `docs`/`a`.`결제`\n\nkeyword `결제`: 대금을 지불하는 행위\n",
    );

    let 진단 = 순환_검사(&root);

    assert_eq!(진단.len(), 1, "{진단:?}");
    assert_eq!(진단[0].code, "K040");
    assert_eq!(위치들(&진단[0]), vec![("docs/a".to_string(), 5)]);
    assert!(
        진단[0].message.contains("docs/a → docs/a"),
        "{}",
        진단[0].message
    );
    // 문서가 하나뿐이므로 '서로가 서로의 전제' 는 가리킬 상대가 없는 거짓이다.
    assert!(
        진단[0].message.contains("자기 자신을 전제로"),
        "{}",
        진단[0].message
    );
    assert!(
        !진단[0].message.contains("서로가 서로의"),
        "{}",
        진단[0].message
    );
    // 자기 문서의 심볼은 import 없이 쓸 수 있으므로 fix 는 줄을 지우라고 말한다.
    assert_eq!(진단[0].fixes[0].kind, FixKind::Edit);
    assert_eq!(
        진단[0].fixes[0].doc.as_ref(),
        Some(&문서경로(&["docs", "a"]))
    );
    assert!(진단[0].fixes[0].action.contains("자기"), "{진단:?}");
    정리(&root);
}

/// 깊은 사슬은 순환이 아니다. 같은 문서를 여러 갈래로 지나도 되돌아오지 않으면 DAG 다.
#[test]
fn 순환이_없으면_진단이_없다() {
    let root = 임시_루트("no-cycle");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nimport `docs`/`b`.`청구서`\nimport `docs`/`d`.`정산`\n\nkeyword `결제`: 대금을 지불하는 행위\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: B\n---\n\nimport `docs`/`c`.`영수증`\n\nkeyword `청구서`: 청구 내역을 담은 문서\n",
    );
    쓰기(
        &root,
        "docs/c.kang",
        "---\ndescription: C\n---\n\nimport `docs`/`d`.`정산`\n\nkeyword `영수증`: 지불을 증명하는 문서\n",
    );
    쓰기(
        &root,
        "docs/d.kang",
        "---\ndescription: D\n---\n\nkeyword `정산`: 주고받을 금액을 확정하는 일\n",
    );

    let 진단 = 순환_검사(&root);

    assert!(진단.is_empty(), "{진단:?}");
    정리(&root);
}

/// 문서가 하나도 없으면 볼 간선이 없다.
#[test]
fn 문서가_없으면_순환_진단도_없다() {
    let root = 임시_루트("cycle-empty");
    git_저장소로(&root);

    let 진단 = 순환_검사(&root);

    assert!(진단.is_empty(), "{진단:?}");
    정리(&root);
}

/// 겹치는 순환은 **되돌아오는 간선마다 하나씩** 보고한다.
/// `A↔B` 와 `B↔C` 는 간선 두 개로 닫히므로 진단도 둘이다 — 같은 순환의 중복이 아니다.
#[test]
fn 겹치는_순환을_각각_보고한다() {
    let root = 임시_루트("cycle-overlap");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nimport `docs`/`b`.`청구서`\n\nkeyword `결제`: 대금을 지불하는 행위\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: B\n---\n\nimport `docs`/`a`.`결제`\nimport `docs`/`c`.`영수증`\n\nkeyword `청구서`: 청구 내역을 담은 문서\n",
    );
    쓰기(
        &root,
        "docs/c.kang",
        "---\ndescription: C\n---\n\nimport `docs`/`b`.`청구서`\n\nkeyword `영수증`: 지불을 증명하는 문서\n",
    );

    let 진단 = 순환_검사(&root);

    assert_eq!(진단.len(), 2, "{진단:?}");
    assert_eq!(
        위치들(&진단[0]),
        vec![("docs/a".to_string(), 5), ("docs/b".to_string(), 5)]
    );
    // docs/b 의 두 번째 import 줄이 docs/c 로 가는 간선이므로 줄 번호가 6 이어야 한다.
    assert_eq!(
        위치들(&진단[1]),
        vec![("docs/b".to_string(), 6), ("docs/c".to_string(), 5)]
    );
    assert!(
        진단[0].message.contains("docs/a → docs/b → docs/a"),
        "{}",
        진단[0].message
    );
    assert!(
        진단[1].message.contains("docs/b → docs/c → docs/b"),
        "{}",
        진단[1].message
    );
    정리(&root);
}

/// 같은 문서를 두 번 import 하면 간선이 둘이지만 순환은 하나다.
/// 체인은 **파일에서 먼저 나온 import 줄**을 가리킨다.
#[test]
fn 같은_대상을_두_번_import_해도_한_번만_보고한다() {
    let root = 임시_루트("cycle-dup-edge");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nimport `docs`/`b`.`청구서`\nimport `docs`/`b`.`영수증`\n\nkeyword `결제`: 대금을 지불하는 행위\n\nkeyword `정산`: 주고받을 금액을 확정하는 일\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: B\n---\n\nimport `docs`/`a`.`결제`\nimport `docs`/`a`.`정산`\n\nkeyword `청구서`: 청구 내역을 담은 문서\n\nkeyword `영수증`: 지불을 증명하는 문서\n",
    );

    let (project, 로드_진단) = resolve::load(&root);
    assert!(로드_진단.is_empty(), "{로드_진단:?}");
    // 이 픽스처는 **합법 입력**이어야 한다. 같은 심볼을 두 번 import 하면 로컬 이름이
    // 두 번 묶여 `K052` 로 거부되므로, 중복 제거가 진짜 필요한 경우는 한 문서의
    // **서로 다른 심볼 둘**을 들여오는 이쪽이다.
    let (_, 이름_진단) = resolve::SymbolTable::build(&project);
    assert!(이름_진단.is_empty(), "{이름_진단:?}");

    let 진단 = check::check_cycles(&project);

    assert_eq!(진단.len(), 1, "{진단:?}");
    assert_eq!(
        위치들(&진단[0]),
        vec![("docs/a".to_string(), 5), ("docs/b".to_string(), 5)]
    );
    정리(&root);
}

/// 대상 문서가 없는 import 는 간선이 아니다. 미해결 심볼 진단은 다른 층의 몫이므로
/// 순환 검사는 **아무 진단도 내지 않는다.**
#[test]
fn 해석되지_않는_import_는_간선이_아니다() {
    let root = 임시_루트("cycle-unresolved");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nimport `docs`/`없는문서`.`청구서`\n\nkeyword `결제`: 대금을 지불하는 행위\n",
    );

    let 진단 = 순환_검사(&root);

    assert!(진단.is_empty(), "{진단:?}");
    정리(&root);
}

/// 진단 순서와 체인은 실행마다 같아야 한다. 골든 파일로 고정할 수 없으면 회귀를 볼 수 없다.
/// `load` 를 두 번 불러 서로 다른 `HashMap` 두 개에서 같은 결과가 나오는지 본다.
#[test]
fn 순환_보고는_결정적이다() {
    let root = 임시_루트("cycle-deterministic");
    git_저장소로(&root);
    // 서로 얽힌 순환을 여럿 두어 시작점 선택이 결과를 바꾸는지 드러낸다.
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nimport `docs`/`b`.`청구서`\nimport `docs`/`c`.`영수증`\n\nkeyword `결제`: 대금을 지불하는 행위\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: B\n---\n\nimport `docs`/`c`.`영수증`\n\nkeyword `청구서`: 청구 내역을 담은 문서\n",
    );
    쓰기(
        &root,
        "docs/c.kang",
        "---\ndescription: C\n---\n\nimport `docs`/`a`.`결제`\nimport `docs`/`b`.`청구서`\n\nkeyword `영수증`: 지불을 증명하는 문서\n",
    );

    let 첫번째 = 순환_검사(&root);
    let 두번째 = 순환_검사(&root);

    // **기대값을 적는다.** 두 실행의 자기 일치만 보면 시작점 정렬이 사라져도 우연히
    // 같은 순서가 나올 때 통과한다. `docs/a` 에서 시작하면 진단이 둘이고, `docs/b` 나
    // `docs/c` 에서 시작하면 셋이므로 정렬 삭제가 개수로 확정적으로 드러난다.
    assert_eq!(첫번째.len(), 2, "{첫번째:?}");
    assert_eq!(
        위치들(&첫번째[0]),
        vec![
            ("docs/a".to_string(), 5),
            ("docs/b".to_string(), 5),
            ("docs/c".to_string(), 5)
        ]
    );
    // 두 번째 순환은 docs/c 의 둘째 import 줄(6)이 docs/b 로 되돌아가며 닫힌다.
    assert_eq!(
        위치들(&첫번째[1]),
        vec![("docs/b".to_string(), 5), ("docs/c".to_string(), 6)]
    );

    let 요약 = |진단: &[Diagnostic]| -> Vec<(String, Vec<(String, usize)>)> {
        진단
            .iter()
            .map(|d| (d.message.clone(), 위치들(d)))
            .collect()
    };
    assert_eq!(요약(&첫번째), 요약(&두번째));
    정리(&root);
}

/// 사전순 첫 문서가 순환 밖일 수 있다. 시작점을 진입 차수 0 인 문서로 좁히는 최적화는
/// `docs/b`↔`docs/c` 처럼 **순환만으로 이루어진 부분 그래프**를 통째로 놓친다.
#[test]
fn 시작점이_순환_밖이어도_검출한다() {
    let root = 임시_루트("cycle-outside-first-doc");
    git_저장소로(&root);
    // docs/a 는 import 가 없어 사전순 첫 시작점이면서 순환과 무관하다.
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nkeyword `결제`: 대금을 지불하는 행위\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: B\n---\n\nimport `docs`/`c`.`영수증`\n\nkeyword `청구서`: 청구 내역을 담은 문서\n",
    );
    쓰기(
        &root,
        "docs/c.kang",
        "---\ndescription: C\n---\n\nimport `docs`/`b`.`청구서`\n\nkeyword `영수증`: 지불을 증명하는 문서\n",
    );

    let 진단 = 순환_검사(&root);

    assert_eq!(진단.len(), 1, "{진단:?}");
    assert_eq!(
        위치들(&진단[0]),
        vec![("docs/b".to_string(), 5), ("docs/c".to_string(), 5)]
    );
    assert!(
        진단[0].message.contains("docs/b → docs/c → docs/b"),
        "{}",
        진단[0].message
    );
    정리(&root);
}

/// 프로젝트를 읽어 심볼 규칙만 돌린다.
///
/// 로드와 심볼 테이블 단계에서 진단이 나오면 픽스처가 잘못된 것이므로 여기서 잡는다 —
/// 그것을 그대로 두면 심볼 진단이 없는 이유를 픽스처 오타에서 찾게 된다.
///
/// # 매개변수
/// - `root`: 프로젝트 루트
///
/// # 반환값
/// [`check::check_symbols`] 가 낸 진단들
fn 심볼_검사(root: &Path) -> Vec<Diagnostic> {
    let (project, 로드_진단) = resolve::load(root);
    assert!(로드_진단.is_empty(), "{로드_진단:?}");
    let (table, 테이블_진단) = resolve::SymbolTable::build(&project);
    assert!(테이블_진단.is_empty(), "{테이블_진단:?}");
    check::check_symbols(&project, &table)
}

/// 진단 코드만 뽑는다. 어떤 규칙이 몇 개 울렸는지를 한 줄로 단언하기 위한 것이다.
///
/// # 매개변수
/// - `diagnostics`: 펼칠 진단들
///
/// # 반환값
/// 진단 코드 목록
fn 코드들(diagnostics: &[Diagnostic]) -> Vec<&str> {
    diagnostics.iter().map(|d| d.code).collect()
}

/// 수정의 종류만 순서대로 뽑는다.
///
/// # 매개변수
/// - `diagnostic`: 펼칠 진단
///
/// # 반환값
/// `fixes` 의 종류 목록
fn 수정_종류(diagnostic: &Diagnostic) -> Vec<&FixKind> {
    diagnostic.fixes.iter().map(|fix| &fix.kind).collect()
}

/// 스펙 5.1: 본문의 백틱 심볼이 선언·import 되지 않았으면 error 다.
#[test]
fn 미선언_백틱_심볼은_에러다() {
    let root = 임시_루트("unresolved-symbol");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/core/baseline.kang",
        "---\ndescription: 기준선\n---\n\nkeyword `승격`: 후보를 기준선으로 올리는 일\n",
    );
    쓰기(
        &root,
        "docs/policy/regression.kang",
        "---\ndescription: 회귀 정책\n---\n\n## 회귀의 기준\n\n`승격` 이후에는 회귀를 막는다.\n",
    );

    let 진단 = 심볼_검사(&root);

    assert_eq!(코드들(&진단), vec!["K001"], "{진단:?}");
    assert_eq!(
        위치들(&진단[0]),
        vec![("docs/policy/regression".to_string(), 7)]
    );
    assert!(진단[0].message.contains("승격"), "{}", 진단[0].message);
    정리(&root);
}

/// 스펙 5.1: import 대상 **파일**이 없으면 error 다.
#[test]
fn 없는_파일을_import_하면_에러다() {
    let root = 임시_루트("import-missing-doc");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: 청구\n---\n\nimport `docs`/`nope`.`결제` as `A 결제`\n",
    );

    let 진단 = 심볼_검사(&root);

    assert_eq!(코드들(&진단), vec!["K002"], "{진단:?}");
    assert_eq!(위치들(&진단[0]), vec![("docs/b".to_string(), 5)]);
    // 대상 문서가 없다는 것이 사실이므로 진단이 문서를 지목해야 한다.
    assert!(진단[0].message.contains("docs/nope"), "{}", 진단[0].message);
    정리(&root);
}

/// 스펙 5.1: 파일은 있는데 그 안에 **심볼**이 없으면 error 다.
#[test]
fn 없는_심볼을_import_하면_에러다() {
    let root = 임시_루트("import-missing-symbol");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: 결제 정책\n---\n\nkeyword `결제`: 대금을 지불하는 행위\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: 청구\n---\n\nimport `docs`/`a`.`없는것` as `X`\n",
    );

    let 진단 = 심볼_검사(&root);

    assert_eq!(코드들(&진단), vec!["K002"], "{진단:?}");
    assert_eq!(위치들(&진단[0]), vec![("docs/b".to_string(), 5)]);
    assert!(진단[0].message.contains("없는것"), "{}", 진단[0].message);
    정리(&root);
}

/// 스펙 5.1: import 했으나 어떤 topic 에서도 쓰지 않으면 error 다.
#[test]
fn 사용하지_않는_import_는_에러다() {
    let root = 임시_루트("unused-import");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: 결제 정책\n---\n\nkeyword `결제`: 대금을 지불하는 행위\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: 청구\n---\n\nimport `docs`/`a`.`결제` as `A 결제`\n\n## 청구의 방법\n\n청구서를 발행한다.\n",
    );

    let 진단 = 심볼_검사(&root);

    assert_eq!(코드들(&진단), vec!["K003"], "{진단:?}");
    assert_eq!(위치들(&진단[0]), vec![("docs/b".to_string(), 5)]);
    정리(&root);
}

/// 스펙 4.7: 한 심볼에 두 개 이상의 이름을 붙이는 것은 error 다.
#[test]
fn 한_심볼에_두_alias_는_에러다() {
    let root = 임시_루트("two-alias");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: 결제 정책\n---\n\nkeyword `결제`: 대금을 지불하는 행위\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: 청구\n---\n\nimport `docs`/`a`.`결제` as `X`\nimport `docs`/`a`.`결제` as `Y`\n\n## 청구의 방법\n\n`X` 와 `Y` 는 같다.\n",
    );

    let 진단 = 심볼_검사(&root);

    assert_eq!(코드들(&진단), vec!["K004"], "{진단:?}");
    // 두 import 줄이 모두 관련 위치다.
    assert_eq!(
        위치들(&진단[0]),
        vec![("docs/b".to_string(), 5), ("docs/b".to_string(), 6)]
    );
    정리(&root);
}

/// 스펙 5.1: 서로 다른 파일이 같은 이름을 선언하고 iknow 가 없으면 error 다.
#[test]
fn 이름_충돌에_iknow_가_없으면_에러다() {
    let root = 임시_루트("collision-no-iknow");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/core/lease.kang",
        "---\ndescription: 임차\n---\n\nkeyword `epoch`: 임차 기간의 단위\n",
    );
    쓰기(
        &root,
        "docs/core/draft.kang",
        "---\ndescription: 초안\n---\n\nkeyword `epoch`: 초안의 세대\n",
    );

    let 진단 = 심볼_검사(&root);

    // 진단 단위는 **이름 하나**다. 두 파일이 각자 진단을 받으면 같은 사실이 두 번 나온다.
    assert_eq!(코드들(&진단), vec!["K012"], "{진단:?}");
    assert_eq!(진단[0].locations.len(), 2, "{진단:?}");
    // 양쪽 모두 상대를 명시해야 하므로 수정도 둘이다.
    assert_eq!(수정_종류(&진단[0]), vec![&FixKind::Edit, &FixKind::Edit]);
    정리(&root);
}

/// 스펙 4.4: 상호 명시여야 한다. 한쪽만 iknow 하면 나머지 한쪽이 error 다.
#[test]
fn iknow_가_한쪽에만_있으면_에러다() {
    let root = 임시_루트("iknow-one-sided");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nkeyword `epoch`: A 의 세대 // iknow `docs`/`b`.`epoch`\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: B\n---\n\nkeyword `epoch`: B 의 세대\n",
    );

    let 진단 = 심볼_검사(&root);

    assert_eq!(코드들(&진단), vec!["K012"], "{진단:?}");
    // 위치는 여전히 선언 둘 전부지만, 고칠 곳은 빠뜨린 한쪽뿐이다.
    assert_eq!(진단[0].locations.len(), 2, "{진단:?}");
    assert_eq!(진단[0].fixes.len(), 1, "{진단:?}");
    assert_eq!(
        진단[0].fixes[0].doc.as_ref().map(DocPath::to_string),
        Some("docs/b".to_string())
    );
    정리(&root);
}

/// 스펙 4.4: N개 파일이 같은 이름을 선언하면 각자 나머지 N-1개를 전부 명시해야 한다.
#[test]
fn 세개_파일_충돌은_각자_나머지_2개를_명시해야_한다() {
    let root = 임시_루트("iknow-three");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nkeyword `epoch`: A // iknow `docs`/`b`.`epoch`, `docs`/`c`.`epoch`\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: B\n---\n\nkeyword `epoch`: B // iknow `docs`/`a`.`epoch`, `docs`/`c`.`epoch`\n",
    );
    // c 는 a 만 알고 b 를 빠뜨렸다.
    쓰기(
        &root,
        "docs/c.kang",
        "---\ndescription: C\n---\n\nkeyword `epoch`: C // iknow `docs`/`a`.`epoch`\n",
    );

    let 진단 = 심볼_검사(&root);

    assert_eq!(코드들(&진단), vec!["K012"], "{진단:?}");
    assert_eq!(진단[0].locations.len(), 3, "{진단:?}");
    assert_eq!(진단[0].fixes.len(), 1, "{진단:?}");
    assert_eq!(
        진단[0].fixes[0].doc.as_ref().map(DocPath::to_string),
        Some("docs/c".to_string())
    );
    // 누락된 파일 경로를 명시해야 한다 (스펙 4.4). 이미 명시한 a 를 다시 요구하면 거짓이다.
    assert!(
        진단[0].fixes[0].action.contains("`b`"),
        "{}",
        진단[0].fixes[0].action
    );
    assert!(
        !진단[0].fixes[0].action.contains("`a`"),
        "{}",
        진단[0].fixes[0].action
    );
    정리(&root);
}

/// 스펙 4.4: iknow 대상 파일이나 심볼이 실재하지 않으면 error 다.
#[test]
fn iknow_대상이_없으면_에러다() {
    let root = 임시_루트("iknow-missing-target");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nkeyword `epoch`: A 의 세대 // iknow `docs`/`zzz`.`epoch`\n",
    );

    let 진단 = 심볼_검사(&root);

    assert_eq!(코드들(&진단), vec!["K010"], "{진단:?}");
    assert_eq!(위치들(&진단[0]), vec![("docs/a".to_string(), 5)]);
    정리(&root);
}

/// 스펙 4.3: 전체 경로가 같을 때만 충돌이다. 말단 이름이 같은 것은 충돌이 아니다.
#[test]
fn 계층이_다르면_같은_말단_이름도_충돌이_아니다() {
    let root = 임시_루트("hierarchy-no-collision");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: 결제\n---\n\nkeyword `결제`: 대금을 지불하는 행위\nkeyword `결제`.`상태`: 결제가 놓인 단계\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: 구독\n---\n\nkeyword `구독`: 주기적으로 대금을 내는 계약\nkeyword `구독`.`상태`: 구독이 놓인 단계\n",
    );

    let 진단 = 심볼_검사(&root);

    assert!(진단.is_empty(), "{진단:?}");
    정리(&root);
}

/// **합법 문서를 거부하지 않는다.** 본문의 `` `결제수단`.`카드` `` 는 백틱 쌍 두 개로
/// 파싱되지만 스코프 키는 `"결제수단.카드"` 하나다. 인접 조각을 합치지 않으면 이
/// 합법 문서가 미해결 심볼 error 를 받는다.
#[test]
fn 계층_참조는_인접_조각을_합쳐_해석한다() {
    let root = 임시_루트("hierarchy-ref-merge");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: 결제\n---\n\nkeyword `결제수단`: 대금을 내는 방법\nkeyword `결제수단`.`카드`: 카드를 사용한 결제\n\n## 결제의 방법\n\n사용자는 `결제수단`.`카드` 로 결제한다.\n",
    );

    let 진단 = 심볼_검사(&root);

    assert!(진단.is_empty(), "{진단:?}");
    정리(&root);
}

/// **합법 문서를 거부하지 않는다.** 상위 키워드를 import 하고 하위를 이 문서에서
/// 선언하는 것은 스펙 4.3 이 명시한 형태다. 계층 참조가 상위 조각을 삼켜 버리면
/// 그 import 가 미사용으로 오인된다.
#[test]
fn 계층_참조는_상위_import_를_사용으로_친다() {
    let root = 임시_루트("hierarchy-parent-import");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: 결제\n---\n\nkeyword `결제수단`: 대금을 내는 방법\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: 카드\n---\n\nimport `docs`/`a`.`결제수단`\n\nkeyword `결제수단`.`카드`: 카드를 사용한 결제\n\n## 카드 결제\n\n사용자는 `결제수단`.`카드` 로 결제한다.\n",
    );

    let 진단 = 심볼_검사(&root);

    assert!(진단.is_empty(), "{진단:?}");
    정리(&root);
}

/// 스펙 4.2 는 "본문과 **선언부**의 모든 백틱은 심볼 참조" 다.
/// keyword 정의 안의 참조도 사용으로 쳐야 한다 — 아니면 합법 문서가 거부된다.
#[test]
fn keyword_정의의_참조도_사용으로_친다() {
    let root = 임시_루트("keyword-def-usage");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: 결제\n---\n\nkeyword `결제`: 대금을 지불하는 행위\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: 청구\n---\n\nimport `docs`/`a`.`결제` as `A 결제`\n\nkeyword `청구서`: `A 결제` 로 생겨나는 문서\n",
    );

    let 진단 = 심볼_검사(&root);

    assert!(진단.is_empty(), "{진단:?}");
    정리(&root);
}

/// 스펙 5.1.1: "관련 위치 전부." 한 곳만 보여주면 나머지를 찾아 헤맨다.
#[test]
fn 진단이_관련_위치를_전부_담는다() {
    let root = 임시_루트("all-locations");
    git_저장소로(&root);
    // 세 파일이 같은 이름을 선언하고 아무도 iknow 하지 않는다.
    for 이름 in ["a", "b", "c"] {
        쓰기(
            &root,
            &format!("docs/{이름}.kang"),
            &format!("---\ndescription: {이름}\n---\n\nkeyword `epoch`: {이름} 의 세대\n"),
        );
    }

    let 진단 = 심볼_검사(&root);

    assert_eq!(코드들(&진단), vec!["K012"], "{진단:?}");
    assert_eq!(
        위치들(&진단[0]),
        vec![
            ("docs/a".to_string(), 5),
            ("docs/b".to_string(), 5),
            ("docs/c".to_string(), 5)
        ]
    );
    // 각 위치는 "그 위치가 왜 관련되는지" 를 말해야 한다.
    for location in &진단[0].locations {
        assert!(!location.note.is_empty(), "{location:?}");
    }
    정리(&root);
}

/// 스펙 5.1.1: 셸 명령을 담는 fix 는 인용까지 포함한다. 공백이 든 경로가 쪼개지면
/// 에이전트가 그대로 실행했을 때 엉뚱한 인자를 받는다.
#[test]
fn 셸_명령_fix_는_인용되어_출력된다() {
    let root = 임시_루트("shell-quoting");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/core policy/base.kang",
        "---\ndescription: 기준선\n---\n\nkeyword `승격`: 후보를 기준선으로 올리는 일\n",
    );
    쓰기(
        &root,
        "docs/regression.kang",
        "---\ndescription: 회귀\n---\n\n## 회귀의 기준\n\n`승격` 이후에는 회귀를 막는다.\n",
    );

    let 진단 = 심볼_검사(&root);

    assert_eq!(코드들(&진단), vec!["K001"], "{진단:?}");
    let 셸 = 진단[0]
        .fixes
        .iter()
        .find(|fix| fix.kind == FixKind::Shell)
        .expect("미해결 심볼 진단은 bless 셸 수정을 갖는다");
    assert!(
        셸.action.contains("'docs/core policy/base.승격'"),
        "{}",
        셸.action
    );
    정리(&root);
}

/// 스펙 5.1.1 의 첫 예시와 **구조**가 같아야 한다 —
/// 코드 `K001`, 참조 자리 하나, `[edit]` 다음 `[shell]`.
#[test]
fn 미해결_심볼_진단의_구조가_스펙_5_1_1_과_일치한다() {
    let root = 임시_루트("k001-shape");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/core/baseline.kang",
        "---\ndescription: 기준선\n---\n\nkeyword `승격`: 후보를 기준선으로 올리는 일\n",
    );
    쓰기(
        &root,
        "docs/policy/regression.kang",
        "---\ndescription: 회귀 정책\n---\n\n## 회귀의 기준\n\n`승격` 이후에는 회귀를 막는다.\n",
    );

    let 진단 = 심볼_검사(&root);

    assert_eq!(코드들(&진단), vec!["K001"], "{진단:?}");
    assert_eq!(진단[0].severity, Severity::Error);
    assert_eq!(진단[0].locations.len(), 1);
    assert!(!진단[0].locations[0].note.is_empty());
    // 먼저 import 를 써야 bless 가 붙일 자리가 생긴다 — 순서가 뒤집히면 적용되지 않는다.
    assert_eq!(수정_종류(&진단[0]), vec![&FixKind::Edit, &FixKind::Shell]);
    assert_eq!(
        진단[0].fixes[0].doc.as_ref().map(DocPath::to_string),
        Some("docs/policy/regression".to_string())
    );
    assert_eq!(진단[0].fixes[1].doc, None);
    // 같은 이름을 선언한 문서를 알려 주어야 한다 (스펙 5.1.1).
    assert!(
        진단[0].message.contains("docs/core/baseline"),
        "{}",
        진단[0].message
    );

    let 출력 = check::report(&진단);
    assert!(출력.starts_with("error[K001]: "), "{출력}");
    assert!(출력.contains("docs/policy/regression.kang:7"), "{출력}");
    assert!(출력.contains("[edit]"), "{출력}");
    assert!(출력.contains("[shell]"), "{출력}");
    정리(&root);
}

/// 스펙 5.1.1 의 두 번째 예시와 **구조**가 같아야 한다 —
/// 코드 `K012`, 선언 자리 둘, 문서 편집 수정 둘, 셸 수정 없음.
#[test]
fn iknow_누락_진단의_구조가_스펙_5_1_1_과_일치한다() {
    let root = 임시_루트("k012-shape");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/core/lease.kang",
        "---\ndescription: 임차\n---\n\nkeyword `epoch`: 임차 기간의 단위\n",
    );
    쓰기(
        &root,
        "docs/core/draft.kang",
        "---\ndescription: 초안\n---\n\nkeyword `epoch`: 초안의 세대\n",
    );

    let 진단 = 심볼_검사(&root);

    assert_eq!(코드들(&진단), vec!["K012"], "{진단:?}");
    assert_eq!(진단[0].severity, Severity::Error);
    assert_eq!(진단[0].locations.len(), 2);
    for location in &진단[0].locations {
        assert!(!location.note.is_empty(), "{location:?}");
    }
    assert_eq!(수정_종류(&진단[0]), vec![&FixKind::Edit, &FixKind::Edit]);
    // 각 수정은 상대 문서를 iknow 하라고 말한다.
    assert!(
        진단[0].fixes[0].action.contains("`lease`"),
        "{}",
        진단[0].fixes[0].action
    );
    assert!(
        진단[0].fixes[1].action.contains("`draft`"),
        "{}",
        진단[0].fixes[1].action
    );

    let 출력 = check::report(&진단);
    assert!(출력.starts_with("error[K012]: "), "{출력}");
    assert!(출력.contains("docs/core/draft.kang:5"), "{출력}");
    assert!(출력.contains("docs/core/lease.kang:5"), "{출력}");
    assert!(!출력.contains("[shell]"), "{출력}");
    정리(&root);
}

/// 스펙 5.1.1 의 세 번째 예시와 **구조**가 같아야 한다.
///
/// `rev` 핀 규칙 자체는 Task 8 의 몫이므로 여기서는 진단을 손으로 만들어
/// [`check::report`] 가 그 모양을 스펙대로 찍는지만 본다.
#[test]
fn rev_불일치_진단의_구조가_스펙_5_1_1_과_일치한다() {
    let diagnostic = Diagnostic {
        severity: Severity::Error,
        code: "K021",
        message: "rev 핀이 대상의 현재 내용과 다름 — docs/core/payment#결제의 방법".to_string(),
        locations: vec![Location {
            doc: 문서경로(&["docs", "billing", "invoice"]),
            line: 3,
            note: "핀 a3f9c1, 현재 7b21e0".to_string(),
        }],
        fixes: vec![Fix {
            kind: FixKind::Shell,
            doc: None,
            action: "대상을 다시 읽고 핀을 갱신하세요: kang bless 'docs/billing/invoice' --import 'docs/core/payment#결제의 방법'".to_string(),
        }],
    };

    let 출력 = check::report(std::slice::from_ref(&diagnostic));

    assert!(출력.starts_with("error[K021]: "), "{출력}");
    // 위치는 문서 파일 이름과 줄 번호로 찍는다.
    assert!(출력.contains("docs/billing/invoice.kang:3"), "{출력}");
    assert!(출력.contains("핀 a3f9c1, 현재 7b21e0"), "{출력}");
    assert!(출력.contains("[shell]"), "{출력}");
    assert!(출력.contains("'docs/core/payment#결제의 방법'"), "{출력}");
    // 셸 수정은 CLI 문법이므로 백틱이 없어야 한다 (스펙 6.0).
    let 셸_줄 = 출력
        .lines()
        .find(|line| line.contains("[shell]"))
        .expect("셸 수정 줄이 있어야 한다");
    assert!(!셸_줄.contains('`'), "{셸_줄}");
}

/// 스펙 5.1.1: `[edit]` 는 문서 문법(백틱), `[shell]` 은 CLI 문법(백틱 금지·인용)이다.
/// 두 문법을 섞으면 삽입된 줄이 4.2 를 위반하거나 셸에서 명령 치환으로 터진다.
#[test]
fn edit_fix_는_문서_문법으로_shell_fix_는_cli_문법으로_출력된다() {
    let root = 임시_루트("fix-syntax");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/core/baseline.kang",
        "---\ndescription: 기준선\n---\n\nkeyword `승격`: 후보를 기준선으로 올리는 일\n",
    );
    쓰기(
        &root,
        "docs/policy/regression.kang",
        "---\ndescription: 회귀 정책\n---\n\n## 회귀의 기준\n\n`승격` 이후에는 회귀를 막는다.\n",
    );

    let 진단 = 심볼_검사(&root);

    assert_eq!(코드들(&진단), vec!["K001"], "{진단:?}");
    let 편집 = &진단[0].fixes[0];
    let 셸 = &진단[0].fixes[1];
    // 문서 문법: 경로 조각마다 백틱을 두르고 `/` 로 잇는다.
    assert_eq!(편집.kind, FixKind::Edit);
    assert!(
        편집.action.contains("`docs`/`core`/`baseline`.`승격`"),
        "{}",
        편집.action
    );
    assert!(!편집.action.contains('\''), "{}", 편집.action);
    // CLI 문법: 백틱을 쓰지 않고 인용한다.
    assert_eq!(셸.kind, FixKind::Shell);
    assert!(!셸.action.contains('`'), "{}", 셸.action);
    assert!(
        셸.action.contains("'docs/core/baseline.승격'"),
        "{}",
        셸.action
    );
    정리(&root);
}

/// 스펙 5.1.1: `fix` 는 **순서 있는 목록**이며 앞에서부터 적용한다.
/// import 를 먼저 써야 `bless` 가 핀을 붙일 줄이 생긴다.
#[test]
fn fixes_는_적용_순서대로_나온다() {
    let root = 임시_루트("fix-order");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: 기준선\n---\n\nkeyword `승격`: 후보를 기준선으로 올리는 일\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: 회귀\n---\n\n## 회귀의 기준\n\n`승격` 이후에는 회귀를 막는다.\n",
    );

    let 진단 = 심볼_검사(&root);

    assert_eq!(코드들(&진단), vec!["K001"], "{진단:?}");
    assert_eq!(수정_종류(&진단[0]), vec![&FixKind::Edit, &FixKind::Shell]);

    let 출력 = check::report(&진단);
    let 편집_자리 = 출력.find("[edit]").expect("문서 편집 수정이 있어야 한다");
    let 셸_자리 = 출력.find("[shell]").expect("셸 수정이 있어야 한다");
    assert!(편집_자리 < 셸_자리, "{출력}");
    정리(&root);
}

/// `Location::line` 은 1-based 이므로 `0` 은 "가리킬 줄이 없음" 규약이다
/// (Task 4 가 `K050`·`K051` 에 쓴다). 그때 `doc` 은 문서 주소가 아니라 표시용 경로이므로
/// `.kang` 을 붙이거나 `:0` 을 찍으면 진단이 없는 파일 이름과 없는 줄을 지어낸다.
#[test]
fn 줄이_없는_진단은_줄_번호도_확장자도_찍지_않는다() {
    let diagnostic = Diagnostic {
        severity: Severity::Error,
        code: "K050",
        message: "kang 프로젝트 루트를 찾지 못했습니다.".to_string(),
        locations: vec![Location {
            doc: 문서경로(&["/tmp/여기에는 git 이 없다"]),
            line: 0,
            note: "이 디렉토리에서 위로 올라가며 .git 을 찾았지만 없었습니다.".to_string(),
        }],
        fixes: vec![Fix {
            kind: FixKind::Shell,
            doc: None,
            action: "git init 을 실행하세요.".to_string(),
        }],
    };

    let 출력 = check::report(std::slice::from_ref(&diagnostic));

    assert!(출력.contains("/tmp/여기에는 git 이 없다"), "{출력}");
    assert!(!출력.contains(":0"), "{출력}");
    assert!(!출력.contains(".kang"), "{출력}");
}

/// **합법 문서를 거부하지 않는다.** 스펙 4.6·4.7 의 정본 예시가 exception 을 alias 로
/// import 한 뒤 `cover` 로만 쓴다. `cover` 대상을 사용으로 세지 않으면 그 문서가
/// 미사용 import error 를 받는다.
#[test]
fn cover_로만_쓰는_import_도_사용으로_친다() {
    let root = 임시_루트("cover-usage");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: 청구\n---\n\n## 청구서와 결제의 관계\n\n모든 청구서는 결제로 생겨난다.\n\nexception `무료 상품에 대한 청구서`\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: 무료상품\n---\n\nimport `docs`/`a`!`무료 상품에 대한 청구서` as `무료상품 청구서 예외`\n\n## 무료상품 결제일 때 청구서\n\n무료상품은 0원 기록만 남긴다.\n\ncover `무료상품 청구서 예외`\n",
    );

    let 진단 = 심볼_검사(&root);

    assert!(진단.is_empty(), "{진단:?}");
    정리(&root);
}

/// 문서가 하나도 없는 프로젝트에는 검사할 심볼도 없다. 규칙이 빈 입력에서 터지면
/// 갓 만든 저장소에서 `kang build` 가 죽는다.
#[test]
fn 빈_프로젝트에는_심볼_진단이_없다() {
    let root = 임시_루트("empty-project");
    git_저장소로(&root);

    let 진단 = 심볼_검사(&root);

    assert!(진단.is_empty(), "{진단:?}");
    assert_eq!(check::report(&진단), "");
    정리(&root);
}

/// 스펙 4.2: "본문과 **선언부**의 모든 백틱은 심볼 참조다." keyword 의 상세 마커도
/// 선언부의 백틱이므로 대상이 스코프에 없으면 미해결 심볼이다.
/// 이것을 검사하지 않으면 `` #`없는 상세` `` 가 아무 진단 없이 통과한다.
#[test]
fn 없는_상세_대상은_미해결이다() {
    let root = 임시_루트("detail-missing");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: 결제\n---\n\nkeyword `결제`: 대금을 지불하는 행위 #`없는 상세`\n",
    );

    let 진단 = 심볼_검사(&root);

    assert_eq!(코드들(&진단), vec!["K001"], "{진단:?}");
    // 가리킬 자리는 그 keyword 선언 줄이다.
    assert_eq!(위치들(&진단[0]), vec![("docs/a".to_string(), 5)]);
    assert!(!진단[0].locations[0].note.is_empty(), "{진단:?}");
    assert!(진단[0].message.contains("없는 상세"), "{}", 진단[0].message);
    정리(&root);
}

/// **합법 문서를 거부하지 않는다.** 스펙 4.3:91 은 상위 키워드가 "같은 파일에서
/// 정의했거나 import 한 것" 이어야 한다고 한다. 상세 마커도 같은 두 자리를 가리킬 수
/// 있어야 하고, 그 둘이 정확히 스코프의 내용이다. 여기서는 **같은 파일** 쪽을 본다.
///
/// 종류는 보지 않는다 — 스펙이 "상세 대상은 반드시 topic" 을 명시하지 않았으므로
/// 종류를 강제하면 그것이 합법 입력 거부가 된다.
#[test]
fn 같은_파일_topic_을_상세로_쓰면_통과한다() {
    let root = 임시_루트("detail-same-file");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: 결제\n---\n\nkeyword `결제`: 대금을 지불하는 행위 #`결제의 상세`\n\n## 결제의 상세\n\n결제를 자세히 설명한다.\n",
    );

    let 진단 = 심볼_검사(&root);

    assert!(진단.is_empty(), "{진단:?}");
    정리(&root);
}

/// **합법 문서를 거부하지 않는다.** 스코프의 나머지 절반 — import 한 alias 다.
/// 상세로만 쓰는 import 가 미사용으로 오인되어도 안 되므로 `K003` 도 함께 막는다.
#[test]
fn import_한_alias_를_상세로_쓰면_통과한다() {
    let root = 임시_루트("detail-import-alias");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: 상세\n---\n\n## 결제의 상세\n\n결제를 자세히 설명한다.\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: 결제\n---\n\nimport `docs`/`a`#`결제의 상세` as `A 결제 상세`\n\nkeyword `결제`: 대금을 지불하는 행위 #`A 결제 상세`\n",
    );

    let 진단 = 심볼_검사(&root);

    assert!(진단.is_empty(), "{진단:?}");
    정리(&root);
}

/// **합법 문서를 거부하지 않는다.** 어떤 이름이 최상위이면서 동시에 다른 이름의
/// 하위일 수 있다 (스펙 4.3). `` `주문` 은 `상태`.`전이` 를 따른다 `` 에서 왼쪽부터
/// 최장 접두를 확정하고 조각을 소비하면 `주문`.`상태` 를 잡아 `전이` 가 고아가 된다.
/// 조각 전부가 해석되는 분할이 있으면 그것을 택해야 한다.
#[test]
fn 계층_이름이_서로의_접두사여도_전부_해석한다() {
    let root = 임시_루트("segmentation-backtrack");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: 주문\n---\n\nkeyword `주문`: 손님이 상품을 사겠다는 의사표시\nkeyword `주문`.`상태`: 주문이 놓인 단계\nkeyword `상태`: 어떤 것이 놓인 단계\nkeyword `상태`.`전이`: 상태가 바뀌는 일\n\n## 주문의 흐름\n\n`주문` 은 `상태`.`전이` 를 따른다.\n",
    );

    let 진단 = 심볼_검사(&root);

    assert!(진단.is_empty(), "{진단:?}");
    정리(&root);
}

/// **합법 문서를 거부하지 않는다.** 스펙 4.3:91 은 하위 키워드를 선언하려면 상위를
/// "같은 파일에서 정의했거나 import" 하라고 **의무로** 못박는다. 그 import 를 미사용이라
/// 하면 처방("이 import 줄을 지우세요")대로 고친 문서가 위법이 된다.
/// 본문이 상위 이름을 백틱으로 언급하지 않아도 하위 선언 자체가 상위를 쓴다.
#[test]
fn 상위_import_는_본문_언급이_없어도_하위_선언이_쓴다() {
    let root = 임시_루트("parent-import-by-decl");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/x.kang",
        "---\ndescription: 결제\n---\n\nkeyword `결제수단`: 대금을 내는 방법\n",
    );
    쓰기(
        &root,
        "docs/d.kang",
        "---\ndescription: 카드\n---\n\nimport `docs`/`x`.`결제수단`\n\nkeyword `결제수단`.`카드`: 카드를 사용한 결제\n",
    );

    let 진단 = 심볼_검사(&root);

    assert!(진단.is_empty(), "{진단:?}");
    정리(&root);
}

/// 계층 참조에 조각으로 묻어 들어간다고 해서 **무관한** import 가 사용으로 세어지면
/// 안 된다. `` `결제수단`.`카드` `` 는 심볼 하나를 가리키지 `카드` 를 따로 쓰지 않는다.
#[test]
fn 무관한_import_는_계층_조각에_묻어_통과하지_않는다() {
    let root = 임시_루트("fragment-leak");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/x.kang",
        "---\ndescription: 결제\n---\n\nkeyword `결제수단`: 대금을 내는 방법\n",
    );
    쓰기(
        &root,
        "docs/y.kang",
        "---\ndescription: 카드\n---\n\nkeyword `카드`: 플라스틱 지불 수단\n",
    );
    쓰기(
        &root,
        "docs/d.kang",
        "---\ndescription: 카드 결제\n---\n\nimport `docs`/`x`.`결제수단`\nimport `docs`/`y`.`카드`\n\nkeyword `결제수단`.`카드`: 카드를 사용한 결제\n\n## 카드 결제\n\n사용자는 `결제수단`.`카드` 로 결제한다.\n",
    );

    let 진단 = 심볼_검사(&root);

    // 상위 `결제수단` 은 하위 선언이 쓰지만, `카드` 는 아무도 쓰지 않는다.
    assert_eq!(코드들(&진단), vec!["K003"], "{진단:?}");
    assert!(진단[0].message.contains("카드"), "{}", 진단[0].message);
    정리(&root);
}

/// `K012` 의 수정은 **상대 심볼의 종류**로 주소를 적어야 한다. 자기 종류로 적으면
/// 구분 기호가 뒤바뀌어(`.` ↔ `#`) 그대로 적용한 문서가 `K010` 을 맞는다.
#[test]
fn iknow_수정은_상대_심볼의_종류로_주소를_적는다() {
    let root = 임시_루트("iknow-fix-kind");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nkeyword `epoch`: 임차 기간의 단위\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: B\n---\n\n## epoch\n\n세대를 뜻하는 서술 단위.\n",
    );

    let 진단 = 심볼_검사(&root);

    assert_eq!(코드들(&진단), vec!["K012"], "{진단:?}");
    assert_eq!(진단[0].fixes.len(), 2, "{진단:?}");
    // a 를 고치는 수정은 b 의 topic 을 `#` 로 가리켜야 한다.
    assert_eq!(
        진단[0].fixes[0].doc.as_ref().map(DocPath::to_string),
        Some("docs/a".to_string())
    );
    assert!(
        진단[0].fixes[0].action.contains("`docs`/`b`#`epoch`"),
        "{}",
        진단[0].fixes[0].action
    );
    // b 를 고치는 수정은 a 의 keyword 를 `.` 로 가리켜야 한다.
    assert!(
        진단[0].fixes[1].action.contains("`docs`/`a`.`epoch`"),
        "{}",
        진단[0].fixes[1].action
    );
    정리(&root);
}

/// 위 수정을 그대로 적용한 문서는 진단이 없어야 한다. 종류를 잘못 적으면 여기서
/// `K010` 이 난다 — 수정이 스스로 새 진단을 만드는 것이 `K012` 의 최악 실패다.
#[test]
fn 종류가_다른_이름_충돌도_iknow_로_합법이_된다() {
    let root = 임시_루트("iknow-cross-kind-legal");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nkeyword `epoch`: 임차 기간의 단위 // iknow `docs`/`b`#`epoch`\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: B\n---\n\n## epoch // iknow `docs`/`a`.`epoch`\n\n세대를 뜻하는 서술 단위.\n",
    );

    let 진단 = 심볼_검사(&root);

    assert!(진단.is_empty(), "{진단:?}");
    정리(&root);
}

/// 상호 명시가 **완전하면** 진단이 없다. 세 문서가 서로를 전부 알고 있는 것은
/// 스펙 4.4 가 정의한 합법 형태이고, 이 층에서 직접 단언해 두지 않으면
/// "무조건 진단" 회귀가 수정 개수 단언만으로 간접적으로 걸리게 된다.
#[test]
fn 상호_명시가_완전하면_진단이_없다() {
    let root = 임시_루트("iknow-complete");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nkeyword `epoch`: A // iknow `docs`/`b`.`epoch`, `docs`/`c`.`epoch`\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: B\n---\n\nkeyword `epoch`: B // iknow `docs`/`a`.`epoch`, `docs`/`c`.`epoch`\n",
    );
    쓰기(
        &root,
        "docs/c.kang",
        "---\ndescription: C\n---\n\nkeyword `epoch`: C // iknow `docs`/`a`.`epoch`, `docs`/`b`.`epoch`\n",
    );

    let 진단 = 심볼_검사(&root);

    assert!(진단.is_empty(), "{진단:?}");
    정리(&root);
}

/// `[edit]` 는 문서 문법이다 (스펙 5.1.1). 계층 이름을 백틱 **한 쌍**으로 감싸면
/// `` `결제수단.카드` `` 라는, 문서 어디에도 없는 문자열을 찾으라고 시키게 된다.
#[test]
fn 계층_이름_진단은_조각마다_백틱을_두른다() {
    let root = 임시_루트("hierarchy-doc-syntax");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/x.kang",
        "---\ndescription: 결제\n---\n\nkeyword `결제수단`: 대금을 내는 방법\nkeyword `결제수단`.`카드`: 카드를 사용한 결제\n",
    );
    // alias 없이 계층 keyword 를 import 하고 쓰지 않는다.
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: 청구\n---\n\nimport `docs`/`x`.`결제수단`.`카드`\n\n## 청구의 방법\n\n청구서를 발행한다.\n",
    );

    let 진단 = 심볼_검사(&root);

    assert_eq!(코드들(&진단), vec!["K003"], "{진단:?}");
    let 편집 = &진단[0].fixes[0];
    assert_eq!(편집.kind, FixKind::Edit);
    assert!(편집.action.contains("`결제수단`.`카드`"), "{}", 편집.action);
    assert!(!편집.action.contains("`결제수단.카드`"), "{}", 편집.action);
    정리(&root);
}

/// 스펙 4.3:91 — "상위 키워드는 같은 파일에서 정의했거나 import한 것이어야 한다."
/// 지금은 상위가 어디에도 없는 계층 keyword 가 진단 없이 통과한다.
#[test]
fn 상위가_없는_계층_keyword_는_에러다() {
    let root = 임시_루트("parent-missing");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: 카드\n---\n\nkeyword `없는부모`.`카드`: 카드를 사용한 결제\n",
    );

    let 진단 = 심볼_검사(&root);

    assert_eq!(코드들(&진단), vec!["K005"], "{진단:?}");
    assert_eq!(위치들(&진단[0]), vec![("docs/a".to_string(), 5)]);
    assert!(!진단[0].locations[0].note.is_empty(), "{진단:?}");
    assert!(진단[0].message.contains("없는부모"), "{}", 진단[0].message);
    // 두 갈래를 다 준다 — 여기서 선언하거나, import 하거나.
    assert_eq!(수정_종류(&진단[0]), vec![&FixKind::Edit, &FixKind::Edit]);
    정리(&root);
}

/// **합법 문서를 거부하지 않는다.** 같은 파일에서 상위를 선언한 형태 (스펙 4.3 정본 예시).
#[test]
fn 같은_파일에서_선언한_상위는_통과한다() {
    let root = 임시_루트("parent-same-file");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: 결제\n---\n\nkeyword `결제수단`: 대금을 내는 방법\nkeyword `결제수단`.`카드`: 카드를 사용한 결제\n",
    );

    let 진단 = 심볼_검사(&root);

    assert!(진단.is_empty(), "{진단:?}");
    정리(&root);
}

/// **합법 문서를 거부하지 않는다.** 상위를 import 한 형태.
#[test]
fn import_한_상위는_통과한다() {
    let root = 임시_루트("parent-imported");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: 결제\n---\n\nkeyword `결제수단`: 대금을 내는 방법\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: 카드\n---\n\nimport `docs`/`a`.`결제수단`\n\nkeyword `결제수단`.`카드`: 카드를 사용한 결제\n",
    );

    let 진단 = 심볼_검사(&root);

    assert!(진단.is_empty(), "{진단:?}");
    정리(&root);
}

/// **합법 문서를 거부하지 않는다 — 이 라운드의 핵심 회귀 방어.**
/// alias 를 붙이면 스코프에 묶이는 이름은 `수단` 이지만, 스펙 4.3:91 은 "import**한 것**"
/// 을 허용하고 4.7 은 alias 가 정식 이름을 폐기한다고 하지 않는다.
/// 상위 판정을 `scope()` 로 하면 이 합법 문서가 거부된다.
#[test]
fn alias_로_import_한_상위도_통과한다() {
    let root = 임시_루트("parent-aliased");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: 결제\n---\n\nkeyword `결제수단`: 대금을 내는 방법\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: 카드\n---\n\nimport `docs`/`a`.`결제수단` as `수단`\n\nkeyword `결제수단`.`카드`: 카드를 사용한 결제\n",
    );

    let 진단 = 심볼_검사(&root);

    assert!(진단.is_empty(), "{진단:?}");
    정리(&root);
}

/// **합법 문서를 거부하지 않는다.** 요구되는 것은 **직접 상위 하나**다.
/// `결제`.`수단`.`카드` 는 `결제`.`수단` 만 요구하고 `결제` 는 요구하지 않는다 —
/// `결제`.`수단` 이 import 된 것이라면 `결제` 는 그 파일의 사정이다.
#[test]
fn 삼단_계층은_직접_상위만_요구한다() {
    let root = 임시_루트("parent-direct-only");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/x.kang",
        "---\ndescription: 결제\n---\n\nkeyword `결제`: 대금을 지불하는 행위\nkeyword `결제`.`수단`: 대금을 내는 방법\n",
    );
    // `결제` 는 이 문서에 없다. 그래도 통과해야 한다.
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: 카드\n---\n\nimport `docs`/`x`.`결제`.`수단`\n\nkeyword `결제`.`수단`.`카드`: 카드를 사용한 결제\n",
    );

    let 진단 = 심볼_검사(&root);

    assert!(진단.is_empty(), "{진단:?}");
    정리(&root);
}

/// **합법 문서를 거부하지 않는다.** 스펙 4.3:91 의 의무는 **선언**에 걸린다.
/// 계층 심볼을 import 하는 것은 남의 파일 심볼을 가져오는 것이고 그 계층은 그 파일의
/// 사정이므로, 상위를 함께 import 할 것을 요구하지 않는다 (스펙 4.7 에 그런 조항이 없다).
#[test]
fn 계층_심볼_import_는_상위를_요구하지_않는다() {
    let root = 임시_루트("import-no-parent-duty");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/x.kang",
        "---\ndescription: 결제\n---\n\nkeyword `결제`: 대금을 지불하는 행위\nkeyword `결제`.`수단`: 대금을 내는 방법\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: 청구\n---\n\nimport `docs`/`x`.`결제`.`수단`\n\n## 청구의 방법\n\n사용자는 `결제`.`수단` 으로 낸다.\n",
    );

    let 진단 = 심볼_검사(&root);

    assert!(진단.is_empty(), "{진단:?}");
    정리(&root);
}

/// 상위는 **keyword** 여야 한다. 스펙 4.1:68 은 `.` 를 "키워드 진입 및 키워드 계층",
/// 4.3:91 은 "상위 **키워드**" 라고 못박는다. 같은 파일 가지는 `document.keywords` 만
/// 보므로 이미 keyword 전용인데, import 가지가 종류를 보지 않으면 같은 규칙이 경로에
/// 따라 다르게 적용된다.
#[test]
fn topic_을_상위로_쓰면_에러다() {
    let root = 임시_루트("parent-wrong-kind");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: 결제\n---\n\n## 결제수단\n\n대금을 내는 방법을 설명한다.\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: 카드\n---\n\nimport `docs`/`a`#`결제수단`\n\nkeyword `결제수단`.`카드`: 카드를 사용한 결제\n",
    );

    let 진단 = 심볼_검사(&root);

    assert_eq!(코드들(&진단), vec!["K005"], "{진단:?}");
    assert_eq!(위치들(&진단[0]), vec![("docs/b".to_string(), 7)]);
    // 이 이름은 **import 되어 있다.** "선언되지도 import 되지도 않았다" 고 하면 거짓이다.
    assert!(진단[0].message.contains("topic"), "{}", 진단[0].message);
    assert!(
        !진단[0].message.contains("찾을 수 없습니다"),
        "{}",
        진단[0].message
    );
    정리(&root);
}

/// 사용 집계도 `K005` 와 같은 규칙을 써야 한다. 계층의 상위는 keyword 뿐이므로
/// 이름만 같은 topic import 는 하위 선언이 쓰는 것이 아니다.
/// 안 그러면 무관한 계층 선언 하나가 무관한 import 의 미사용 검사를 무력화한다.
#[test]
fn 무관한_topic_import_는_계층_선언에_묻어_통과하지_않는다() {
    let root = 임시_루트("usage-kind");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: 결제\n---\n\nkeyword `결제수단`: 대금을 내는 방법 // iknow `docs`/`c`#`결제수단`\n",
    );
    쓰기(
        &root,
        "docs/c.kang",
        "---\ndescription: 설명\n---\n\n## 결제수단 // iknow `docs`/`a`.`결제수단`\n\n결제수단을 서술한다.\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: 카드\n---\n\nimport `docs`/`c`#`결제수단` as `수단토픽`\nimport `docs`/`a`.`결제수단`\n\nkeyword `결제수단`.`카드`: 카드를 사용한 결제\n",
    );

    let 진단 = 심볼_검사(&root);

    // 쓰이지 않는 것은 topic import 하나뿐이다.
    assert_eq!(코드들(&진단), vec!["K003"], "{진단:?}");
    assert!(진단[0].message.contains("수단토픽"), "{}", 진단[0].message);
    정리(&root);
}

/// 사용 집계는 **직접 상위 하나**만 센다. `K005` 의 의무가 직접 상위에만 걸리므로
/// (`삼단_계층은_직접_상위만_요구한다`) 조부모까지 세면 참인 `K003` 이 사라진다.
#[test]
fn 조부모_import_는_하위_선언이_쓰지_않는다() {
    let root = 임시_루트("usage-direct-parent");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/x.kang",
        "---\ndescription: 결제\n---\n\nkeyword `결제`: 대금을 지불하는 행위\nkeyword `결제`.`수단`: 대금을 내는 방법\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: 카드\n---\n\nimport `docs`/`x`.`결제`\nimport `docs`/`x`.`결제`.`수단`\n\nkeyword `결제`.`수단`.`카드`: 카드를 사용한 결제\n",
    );

    let 진단 = 심볼_검사(&root);

    // `결제`.`수단` 은 직접 상위라 쓰이지만 `결제` 는 아무도 쓰지 않는다.
    assert_eq!(코드들(&진단), vec!["K003"], "{진단:?}");
    assert_eq!(위치들(&진단[0]), vec![("docs/b".to_string(), 5)]);
    정리(&root);
}

/// 같은 파일의 topic 이 상위 이름을 갖고 있으면 그 사실을 말해야 한다.
/// "이 문서에서 찾을 수 없습니다" 는 거짓이다 — 이름은 이 문서에 있고 종류가 다르다.
#[test]
fn 같은_파일_topic_이_상위_이름을_가지면_그_사실을_말한다() {
    let root = 임시_루트("parent-local-topic");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: 카드\n---\n\nkeyword `결제수단`.`카드`: 카드를 사용한 결제\n\n## 결제수단\n\n결제수단을 서술한다.\n",
    );

    let 진단 = 심볼_검사(&root);

    assert_eq!(코드들(&진단), vec!["K005"], "{진단:?}");
    assert!(진단[0].message.contains("topic"), "{}", 진단[0].message);
    assert!(
        !진단[0].message.contains("찾을 수 없습니다"),
        "{}",
        진단[0].message
    );
    정리(&root);
}

/// 스펙 V0001:417 — 에이전트는 진단의 `fix` 를 **그대로 적용한다.**
/// 같은 이름이 이미 이 문서에 묶여 있으면 "여기서 선언하세요" 만으로는 그 이름이 두 번
/// 묶여 `K052` 가 난다. 수정이 스스로 새 진단을 만들어서는 안 된다.
#[test]
fn k005_수정은_이미_묶인_이름을_먼저_풀라고_말한다() {
    let root = 임시_루트("k005-fix-k052");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: 결제\n---\n\n## 결제수단\n\n결제수단을 서술한다.\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: 카드\n---\n\nimport `docs`/`a`#`결제수단`\n\nkeyword `결제수단`.`카드`: 카드를 사용한 결제\n",
    );

    let 진단 = 심볼_검사(&root);

    assert_eq!(코드들(&진단), vec!["K005"], "{진단:?}");
    // 두 갈래 모두 이미 묶인 이름을 어떻게 할지 말해야 한다.
    for fix in &진단[0].fixes {
        assert!(
            fix.action.contains("먼저 지운") || fix.action.contains("바꾸세요"),
            "{}",
            fix.action
        );
    }

    // 첫 수정을 그대로 적용한다 — 같은 이름을 묶던 import 줄을 지우고 상위를 선언한다.
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: 카드\n---\n\nkeyword `결제수단`: 대금을 내는 방법\nkeyword `결제수단`.`카드`: 카드를 사용한 결제\n",
    );
    let 적용후 = 심볼_검사(&root);

    // `K005` 는 해소되고 **`K052` 가 생기지 않아야** 한다. 남는 `K012` 는 이 문서가
    // 자기 개념을 선언하기로 한 결과 생긴 참인 새 사실이지 수정이 만든 거짓이 아니다.
    assert!(!적용후.iter().any(|d| d.code == "K005"), "{적용후:?}");
    assert!(!적용후.iter().any(|d| d.code == "K052"), "{적용후:?}");
    정리(&root);
}

/// 후보 문서를 아는 경우의 `K005` 수정은 그 주소를 구체적으로 적어야 한다.
/// 두 픽스처 모두 후보가 비어 있으면 이 안내 문구가 통째로 미검증으로 남는다.
#[test]
fn k005_수정은_상위를_가진_문서를_지목한다() {
    let root = 임시_루트("k005-fix-candidate");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/x.kang",
        "---\ndescription: 결제\n---\n\nkeyword `결제수단`: 대금을 내는 방법\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: 카드\n---\n\nkeyword `결제수단`.`카드`: 카드를 사용한 결제\n",
    );

    let 진단 = 심볼_검사(&root);

    assert_eq!(코드들(&진단), vec!["K005"], "{진단:?}");
    assert_eq!(수정_종류(&진단[0]), vec![&FixKind::Edit, &FixKind::Edit]);
    assert!(진단[0].message.contains("docs/x"), "{}", 진단[0].message);
    // 두 번째 갈래는 그 문서에서 가져오라고 구체적으로 말한다.
    assert!(
        진단[0].fixes[1].action.contains("`docs`/`x`.`결제수단`"),
        "{}",
        진단[0].fixes[1].action
    );
    정리(&root);
}

/// `[edit]` 는 문서 문법이다. `K004`·`K012` 의 계층 이름도 조각마다 백틱을 둘러야
/// 한다 — `` `결제.상태` `` 는 문서 어디에도 없는 문자열이다.
#[test]
fn 이름_충돌_진단도_조각마다_백틱을_두른다() {
    let root = 임시_루트("collision-doc-syntax");
    git_저장소로(&root);
    // 상위 `결제` 는 상호 명시하고 하위 `결제`.`상태` 만 빠뜨린다.
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nkeyword `결제`: 대금 지불 // iknow `docs`/`b`.`결제`\nkeyword `결제`.`상태`: 결제가 놓인 단계\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: B\n---\n\nkeyword `결제`: 대금 지불 // iknow `docs`/`a`.`결제`\nkeyword `결제`.`상태`: 결제가 놓인 단계\n",
    );

    let 진단 = 심볼_검사(&root);

    assert_eq!(코드들(&진단), vec!["K012"], "{진단:?}");
    assert!(
        진단[0].message.contains("`결제`.`상태`"),
        "{}",
        진단[0].message
    );
    assert!(
        !진단[0].message.contains("`결제.상태`"),
        "{}",
        진단[0].message
    );
    for fix in &진단[0].fixes {
        assert!(!fix.action.contains("`결제.상태`"), "{}", fix.action);
    }
    정리(&root);
}

/// `K004` 도 마찬가지다. alias 없는 계층 import 의 로컬 이름은 조각마다 백틱이다.
#[test]
fn 이름_여럿_진단도_조각마다_백틱을_두른다() {
    let root = 임시_루트("alias-doc-syntax");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/x.kang",
        "---\ndescription: 결제\n---\n\nkeyword `결제`: 대금을 지불하는 행위\nkeyword `결제`.`상태`: 결제가 놓인 단계\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: 청구\n---\n\nimport `docs`/`x`.`결제`.`상태`\nimport `docs`/`x`.`결제`.`상태` as `상태별칭`\n\n## 청구의 방법\n\n`결제`.`상태` 와 `상태별칭` 은 같다.\n",
    );

    let 진단 = 심볼_검사(&root);

    assert_eq!(코드들(&진단), vec!["K004"], "{진단:?}");
    let 자리 = &진단[0].locations;
    assert_eq!(자리.len(), 2, "{진단:?}");
    assert!(자리[0].note.contains("`결제`.`상태`"), "{}", 자리[0].note);
    assert!(!자리[0].note.contains("`결제.상태`"), "{}", 자리[0].note);
    assert!(자리[1].note.contains("`상태별칭`"), "{}", 자리[1].note);
    정리(&root);
}

// ---------------------------------------------------------------------------
// exception 상태 기계 (스펙 5.2) — `K030`-`K034`
// ---------------------------------------------------------------------------

/// 프로젝트를 읽어 exception 상태 기계 검사만 돌린다.
///
/// 로드와 심볼 테이블 단계에서 진단이 나오면 픽스처가 잘못된 것이므로 여기서 잡는다 —
/// 그것을 그대로 두면 상태 기계 진단이 없는 이유를 픽스처 오타에서 찾게 된다.
///
/// # 매개변수
/// - `root`: 프로젝트 루트
///
/// # 반환값
/// [`check::check_exceptions`] 가 낸 진단들
fn 예외_검사(root: &Path) -> Vec<Diagnostic> {
    let (project, 로드_진단) = resolve::load(root);
    assert!(로드_진단.is_empty(), "{로드_진단:?}");
    let (table, 테이블_진단) = resolve::SymbolTable::build(&project);
    assert!(테이블_진단.is_empty(), "{테이블_진단:?}");
    check::check_exceptions(&project, &table)
}

/// 진리표 (일반, 커버 없음) 칸 — error 다 (스펙 5.2).
#[test]
fn 커버되지_않은_exception_은_에러다() {
    let root = 임시_루트("exc-uncovered");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\n## 정책\n\n본문이다.\n\nexception `무료 상품`\n",
    );

    let diagnostics = 예외_검사(&root);

    assert_eq!(코드들(&diagnostics), vec!["K030"]);
    assert_eq!(diagnostics[0].severity, Severity::Error);
    // 위치는 예외 선언 한 자리다.
    assert_eq!(위치들(&diagnostics[0]), vec![("docs/a".to_string(), 9)]);
    assert!(
        diagnostics[0].locations[0].note.contains("cover"),
        "{:?}",
        diagnostics[0].locations[0].note
    );
    // 커버를 붙이는 길과 pending 을 붙이는 길 둘 다 유효하다.
    assert_eq!(
        수정_종류(&diagnostics[0]),
        vec![&FixKind::Edit, &FixKind::Edit]
    );
    정리(&root);
}

/// 진리표 (일반, 커버 있음) 칸 — 통과다 (스펙 5.2).
#[test]
fn 커버된_exception_은_통과한다() {
    let root = 임시_루트("exc-covered");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\n## 정책\n\n본문이다.\n\nexception `무료 상품`\n\n## 무료 상품 처리\n\n무료는 0원이다.\n\ncover `무료 상품`\n",
    );

    let diagnostics = 예외_검사(&root);

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    정리(&root);
}

/// 진리표 (pending, 커버 없음) 칸 — warn 이며 통과다 (스펙 5.2).
#[test]
fn pending_이고_커버가_없으면_warn_이다() {
    let root = 임시_루트("exc-pending-bare");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\n## 정책\n\n본문이다.\n\nexception `해외 결제` pending\n",
    );

    let diagnostics = 예외_검사(&root);

    assert_eq!(코드들(&diagnostics), vec!["K031"]);
    // 이 칸만이 error 가 아니다. 빌드의 종료 코드를 1로 만들면 안 된다.
    assert_eq!(diagnostics[0].severity, Severity::Warn);
    assert_eq!(위치들(&diagnostics[0]), vec![("docs/a".to_string(), 9)]);
    assert!(
        diagnostics[0].locations[0].note.contains("pending"),
        "{:?}",
        diagnostics[0].locations[0].note
    );
    assert!(!diagnostics[0].fixes.is_empty());
    정리(&root);
}

/// 진리표 (pending, 커버 있음) 칸 — error 다 (스펙 5.2).
#[test]
fn pending_인데_커버가_있으면_에러다() {
    let root = 임시_루트("exc-pending-covered");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\n## 정책\n\n본문이다.\n\nexception `해외 결제` pending\n\n## 해외 결제 처리\n\n해외는 이렇게 한다.\n\ncover `해외 결제`\n",
    );

    let diagnostics = 예외_검사(&root);

    assert_eq!(코드들(&diagnostics), vec!["K032"]);
    assert_eq!(diagnostics[0].severity, Severity::Error);
    // 선언 자리와 커버 자리가 둘 다 관련 위치다 (스펙 5.1.1).
    assert_eq!(
        위치들(&diagnostics[0]),
        vec![("docs/a".to_string(), 9), ("docs/a".to_string(), 15)]
    );
    assert!(diagnostics[0].locations[0].note.contains("pending"));
    // 커버 자리의 note 는 어느 topic 이 커버하는지 말한다.
    assert!(
        diagnostics[0].locations[1].note.contains("해외 결제 처리"),
        "{:?}",
        diagnostics[0].locations[1].note
    );
    // pending 을 떼는 길과 cover 를 지우는 길 둘 다 유효하다.
    assert_eq!(
        수정_종류(&diagnostics[0]),
        vec![&FixKind::Edit, &FixKind::Edit]
    );
    정리(&root);
}

/// 다른 파일이 커버해도 (pending, 커버 있음) 칸이다. 두 파일이 다 관련 위치다.
#[test]
fn pending_을_다른_파일이_커버해도_에러다() {
    let root = 임시_루트("exc-pending-cross");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\n## 정책\n\n본문이다.\n\nexception `해외 결제` pending\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: B\n---\n\nimport `docs`/`a`!`해외 결제`\n\n## 해외 결제 처리\n\n해외는 이렇게 한다.\n\ncover `해외 결제`\n",
    );

    let diagnostics = 예외_검사(&root);

    assert_eq!(코드들(&diagnostics), vec!["K032"]);
    assert_eq!(
        위치들(&diagnostics[0]),
        vec![("docs/a".to_string(), 9), ("docs/b".to_string(), 11)]
    );
    정리(&root);
}

/// 한 exception 을 둘 이상이 커버하면 error 다 — 예외 하나에 정책 하나 (스펙 5.2).
#[test]
fn 한_exception_을_둘이_커버하면_에러다() {
    let root = 임시_루트("exc-double-cover");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\n## 정책\n\n본문이다.\n\nexception `무료 상품`\n\n## 처리 하나\n\n하나다.\n\ncover `무료 상품`\n\n## 처리 둘\n\n둘이다.\n\ncover `무료 상품`\n",
    );

    let diagnostics = 예외_검사(&root);

    assert_eq!(코드들(&diagnostics), vec!["K033"]);
    // 선언 자리와 커버 두 자리가 모두 관련 위치다.
    assert_eq!(
        위치들(&diagnostics[0]),
        vec![
            ("docs/a".to_string(), 9),
            ("docs/a".to_string(), 15),
            ("docs/a".to_string(), 21)
        ]
    );
    assert!(diagnostics[0].locations[1].note.contains("처리 하나"));
    assert!(diagnostics[0].locations[2].note.contains("처리 둘"));
    assert!(!diagnostics[0].fixes.is_empty());
    정리(&root);
}

/// 같은 topic 이 같은 예외를 두 번 커버해도 error 다. 중복 선언은 정보가 없고
/// 스펙 5.2 는 예외 하나에 정책 하나를 강제한다.
#[test]
fn 같은_topic_이_두_번_커버해도_에러다() {
    let root = 임시_루트("exc-same-topic-twice");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\n## 정책\n\n본문이다.\n\nexception `무료 상품`\n\n## 처리\n\n하나다.\n\ncover `무료 상품`\ncover `무료 상품`\n",
    );

    let diagnostics = 예외_검사(&root);

    assert_eq!(코드들(&diagnostics), vec!["K033"]);
    assert_eq!(
        위치들(&diagnostics[0]),
        vec![
            ("docs/a".to_string(), 9),
            ("docs/a".to_string(), 15),
            ("docs/a".to_string(), 16)
        ]
    );
    // 메시지가 "둘 이상의 topic" 이라고 말하면 거짓이다 — topic 은 하나뿐이다.
    assert!(
        !diagnostics[0].message.contains("topic 이 둘"),
        "{:?}",
        diagnostics[0].message
    );
    정리(&root);
}

/// 어떤 exception 도 가리키지 않는 cover 는 error 다.
/// Task 6 은 `cover` 를 미해결 판정에서 뺐으므로 (`src/check.rs` 의 `참조_해석`)
/// 여기서 진단하지 않으면 dangling 참조가 아무 진단 없이 통과한다.
#[test]
fn 실재하지_않는_예외를_커버하면_에러다() {
    let root = 임시_루트("exc-dangling-cover");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\n## 정책\n\n본문이다.\n\ncover `없는 예외`\n",
    );

    let diagnostics = 예외_검사(&root);

    assert_eq!(코드들(&diagnostics), vec!["K034"]);
    assert_eq!(diagnostics[0].severity, Severity::Error);
    assert_eq!(위치들(&diagnostics[0]), vec![("docs/a".to_string(), 9)]);
    assert!(!diagnostics[0].fixes.is_empty());
    정리(&root);
}

/// 다른 파일의 예외를 import 하지 않고 커버하면 error 다.
/// 그 이름은 이 문서의 스코프에 없으므로 이 문서가 가리킬 수 있는 예외가 아니다.
/// 진단은 어느 문서에서 import 하면 되는지까지 말해야 고칠 수 있다 (스펙 5.1.1).
#[test]
fn import_없이_남의_예외를_커버하면_에러다() {
    let root = 임시_루트("exc-cover-no-import");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\n## 정책\n\n본문이다.\n\nexception `무료 상품`\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: B\n---\n\n## 처리\n\n하나다.\n\ncover `무료 상품`\n",
    );

    let diagnostics = 예외_검사(&root);

    // docs/a 의 예외는 커버되지 않았고, docs/b 의 cover 는 대상이 없다.
    assert_eq!(코드들(&diagnostics), vec!["K030", "K034"]);
    let k034 = &diagnostics[1];
    assert_eq!(위치들(k034), vec![("docs/b".to_string(), 9)]);
    // 대상이 어디 있는지 알므로 fix 가 그 import 줄을 그대로 준다.
    assert!(
        k034.fixes[0].action.contains("`docs`/`a`!`무료 상품`"),
        "{:?}",
        k034.fixes[0].action
    );
    정리(&root);
}

/// alias 로 import 한 예외를 그 alias 이름으로 커버하는 것은 합법이다.
/// 스펙 4.6 의 정본 예시가 정확히 이 모양이다 — 선언 이름과 cover 이름이 다르고
/// 그 둘은 alias import 로만 이어진다. 이름만으로 짝을 맞추면 이 문서가 거부된다.
#[test]
fn alias_로_커버해도_통과한다() {
    let root = 임시_루트("exc-alias-cover");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: 청구\n---\n\n## 청구서와 결제의 관계\n\n모든 청구서는 결제로 생겨난다.\n\nexception `무료 상품에 대한 청구서`\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: 무료상품\n---\n\nimport `docs`/`a`!`무료 상품에 대한 청구서` as `무료상품 청구서 예외`\n\n## 무료상품 결제일 때 청구서\n\n무료상품은 0원 기록만 남긴다.\n\ncover `무료상품 청구서 예외`\n",
    );

    let diagnostics = 예외_검사(&root);

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    정리(&root);
}

/// exception 도 cover 도 없는 문서는 아무 진단을 받지 않는다.
#[test]
fn exception_이_없는_문서는_진단이_없다() {
    let root = 임시_루트("exc-none");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\n## 정책\n\n본문이다.\n",
    );

    let diagnostics = 예외_검사(&root);

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    정리(&root);
}

/// 진단 순서는 문서 경로 순이어야 실행마다 같다.
#[test]
fn 진단_순서가_문서_경로_순이다() {
    let root = 임시_루트("exc-order");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: B\n---\n\n## 정책\n\n본문이다.\n\nexception `둘`\n",
    );
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\n## 정책\n\n본문이다.\n\nexception `하나`\n",
    );

    let diagnostics = 예외_검사(&root);

    assert_eq!(코드들(&diagnostics), vec!["K030", "K030"]);
    assert_eq!(diagnostics[0].locations[0].doc, 문서경로(&["docs", "a"]));
    assert_eq!(diagnostics[1].locations[0].doc, 문서경로(&["docs", "b"]));
    정리(&root);
}

/// warn 은 error 와 다른 낱말로 찍혀야 한다. `pending` + 커버 없음 칸이
/// [`kang::ast::Severity::Warn`] 의 첫 소비자다.
#[test]
fn warn_은_report_에서_warning_으로_찍힌다() {
    let root = 임시_루트("exc-warn-report");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\n## 정책\n\n본문이다.\n\nexception `해외 결제` pending\n",
    );

    let 출력 = check::report(&예외_검사(&root));

    assert!(출력.starts_with("warning[K031]:"), "{출력}");
    assert!(!출력.contains("error["), "{출력}");
    정리(&root);
}

/// `K034` 의 import 수정을 그대로 적용하면 진단이 사라져야 한다 (스펙 V0001:417).
/// exception 검사뿐 아니라 심볼 검사도 깨끗해야 한다 — 새로 넣은 import 가
/// `K003`(미사용 import) 을 만들면 수정이 새 진단을 낳은 것이다.
#[test]
fn k034_의_import_수정을_적용하면_진단이_사라진다() {
    let root = 임시_루트("exc-fix-applies");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\n## 정책\n\n본문이다.\n\nexception `무료 상품`\n",
    );
    // `import_없이_남의_예외를_커버하면_에러다` 의 docs/b 에 fix 가 준 import 줄만 넣은 것이다.
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: B\n---\n\nimport `docs`/`a`!`무료 상품`\n\n## 처리\n\n하나다.\n\ncover `무료 상품`\n",
    );

    assert!(예외_검사(&root).is_empty(), "{:?}", 예외_검사(&root));
    assert!(심볼_검사(&root).is_empty(), "{:?}", 심볼_검사(&root));
    정리(&root);
}

/// 스코프에 이름은 있으나 그것이 exception 이 아니면 다른 사실이다.
/// "선언하지도 import 하지도 않았다" 고 말하면 진단이 거짓이 된다.
#[test]
fn 예외가_아닌_이름을_커버하면_다른_이유를_말한다() {
    let root = 임시_루트("exc-cover-keyword");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\nkeyword `결제`: 대금을 지불하는 행위\n\n## 정책\n\n`결제` 를 한다.\n\ncover `결제`\n",
    );

    let diagnostics = 예외_검사(&root);

    assert_eq!(코드들(&diagnostics), vec!["K034"]);
    assert_eq!(위치들(&diagnostics[0]), vec![("docs/a".to_string(), 11)]);
    assert!(
        diagnostics[0].message.contains("exception 이 아닙니다"),
        "{:?}",
        diagnostics[0].message
    );
    // 스코프에 이미 있으므로 import 를 권하면 거짓이다. 수정은 하나뿐이다.
    assert_eq!(diagnostics[0].fixes.len(), 1);
    assert!(!diagnostics[0].fixes[0].action.contains("import"));
    정리(&root);
}

/// `K032` 의 첫 수정은 pending 만 지우라고 말한다. 커버가 둘 이상이면 그것만으로는
/// `(일반, 커버 둘)` 이 되어 `K033` 이 새로 난다 — "통과합니다" 가 거짓이 된다.
/// 스펙 V0001:417 은 에이전트가 fix 를 **그대로 적용**한다고 정한다.
#[test]
fn k032_의_수정이_커버_둘일_때_통과를_약속하지_않는다() {
    let root = 임시_루트("exc-k032-multi");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\n## 정책\n\n본문이다.\n\nexception `해외 결제` pending\n\n## 처리 하나\n\n하나.\n\ncover `해외 결제`\n\n## 처리 둘\n\n둘.\n\ncover `해외 결제`\n",
    );

    let diagnostics = 예외_검사(&root);

    assert_eq!(코드들(&diagnostics), vec!["K032"]);
    let 첫_수정 = &diagnostics[0].fixes[0].action;
    // pending 만 지우면 `K033` 이 남으므로 통과를 약속하면 안 된다.
    assert!(!첫_수정.contains("통과합니다"), "{첫_수정}");
    // 대신 커버를 하나로 줄이라는 것까지 함께 말해야 그대로 적용할 수 있다.
    assert!(첫_수정.contains("하나만"), "{첫_수정}");
    정리(&root);
}

/// 커버가 하나뿐이면 pending 을 지우는 것으로 정말 통과한다. 그 문장은 남아야 한다.
#[test]
fn k032_의_수정이_커버_하나일_때는_통과를_약속한다() {
    let root = 임시_루트("exc-k032-single");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\n## 정책\n\n본문이다.\n\nexception `해외 결제` pending\n\n## 처리\n\n하나.\n\ncover `해외 결제`\n",
    );

    let diagnostics = 예외_검사(&root);

    assert!(
        diagnostics[0].fixes[0].action.contains("통과합니다"),
        "{:?}",
        diagnostics[0].fixes[0].action
    );
    정리(&root);
}

/// `K031` 의 수정대로 다른 문서의 topic 에 cover 만 넣으면 그 이름이 그 문서의 스코프에
/// 없어 `K034` 와 `K030` 이 난다. warn 을 없애려던 수정이 error 둘을 만들면 안 된다.
#[test]
fn k031_의_수정이_타_문서_import_를_함께_안내한다() {
    let root = 임시_루트("exc-k031-import");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\n## 정책\n\n본문이다.\n\nexception `해외 결제` pending\n",
    );

    let diagnostics = 예외_검사(&root);

    assert_eq!(코드들(&diagnostics), vec!["K031"]);
    let 수정 = &diagnostics[0].fixes[0].action;
    assert!(수정.contains("import `docs`/`a`!`해외 결제`"), "{수정}");
    정리(&root);
}

/// `K030`·`K031` 의 note 는 "프로젝트에 없습니다" 라고 단정하면 안 된다.
/// 이름만 같고 import 되지 않은 cover 줄이 문서에 **문자 그대로 있을 수 있고**,
/// 그때 같은 실행의 `K034` 가 그 줄을 지목해 두 진단이 서로를 반박하는 모양이 된다.
#[test]
fn k030_의_note_가_이름만_같은_cover_를_없다고_말하지_않는다() {
    let root = 임시_루트("exc-note-precise");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\n## 정책\n\n본문이다.\n\nexception `무료 상품`\n",
    );
    // import 이 없어 해석되지 않을 뿐, `cover `무료 상품`` 은 파일에 문자 그대로 있다.
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: B\n---\n\n## 처리\n\n하나다.\n\ncover `무료 상품`\n",
    );

    let diagnostics = 예외_검사(&root);

    assert_eq!(코드들(&diagnostics), vec!["K030", "K034"]);
    let note = &diagnostics[0].locations[0].note;
    assert!(!note.contains("프로젝트에 없습니다"), "{note}");
    // 해석 기준으로 좁혀 말해야 grep 으로 반증되지 않는다.
    assert!(note.contains("가리키는"), "{note}");
    정리(&root);
}

/// 같은 이름의 exception 이 iknow 로 여러 문서에 합법적으로 존재할 수 있다 (스펙 4.4·5.1).
/// 그때 `K034` 의 import 수정이 첫 문서를 단정하면 진단이 거짓을 말한다.
/// 후보마다 조건절을 붙인다 — `미해결_심볼` 이 같은 자리에서 지키는 규칙이다.
#[test]
fn k034_는_후보가_여럿이면_문서를_단정하지_않는다() {
    let root = 임시_루트("exc-k034-multi-owner");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\n## 정책 가\n\n본문이다.\n\nexception `무료 상품` // iknow `docs`/`b`!`무료 상품`\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: B\n---\n\n## 정책 나\n\n본문이다.\n\nexception `무료 상품` // iknow `docs`/`a`!`무료 상품`\n",
    );
    쓰기(
        &root,
        "docs/c.kang",
        "---\ndescription: C\n---\n\n## 처리\n\n하나다.\n\ncover `무료 상품`\n",
    );

    let diagnostics = 예외_검사(&root);

    assert_eq!(코드들(&diagnostics), vec!["K030", "K030", "K034"]);
    let k034 = &diagnostics[2];
    // 후보 둘을 각각 조건절과 함께 준다. 마지막은 이름 수정/줄 삭제 안내다.
    assert_eq!(k034.fixes.len(), 3);
    assert!(
        k034.fixes[0].action.contains("docs/a 가 같은 개념이라면"),
        "{:?}",
        k034.fixes[0].action
    );
    assert!(
        k034.fixes[0]
            .action
            .contains("import `docs`/`a`!`무료 상품`"),
        "{:?}",
        k034.fixes[0].action
    );
    assert!(
        k034.fixes[1].action.contains("docs/b 가 같은 개념이라면"),
        "{:?}",
        k034.fixes[1].action
    );
    정리(&root);
}

/// 후보가 하나뿐이면 조건절 없이 그대로 단정해도 참이다.
#[test]
fn k034_는_후보가_하나면_조건절을_붙이지_않는다() {
    let root = 임시_루트("exc-k034-single-owner");
    git_저장소로(&root);
    쓰기(
        &root,
        "docs/a.kang",
        "---\ndescription: A\n---\n\n## 정책\n\n본문이다.\n\nexception `무료 상품`\n",
    );
    쓰기(
        &root,
        "docs/b.kang",
        "---\ndescription: B\n---\n\n## 처리\n\n하나다.\n\ncover `무료 상품`\n",
    );

    let diagnostics = 예외_검사(&root);

    let k034 = &diagnostics[1];
    assert_eq!(k034.fixes.len(), 2);
    assert!(
        !k034.fixes[0].action.contains("같은 개념이라면"),
        "{:?}",
        k034.fixes[0].action
    );
    정리(&root);
}
