// `resolve::find_root` / `resolve::load` / `resolve::SymbolTable` 을 검증하는 통합 테스트.
//
// 이 파일의 테스트는 **실제 파일 시스템**을 쓴다. 각 테스트는 자기만의 임시 디렉토리를
// 만들고 그 안에서만 움직이므로 `cargo test` 의 기본 병렬 실행에서도 서로를 밟지 않는다.
use kang::ast::{DocPath, FixKind, Severity, SymbolKind, SymbolRef};
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
