//! kang CLI 진입점.
//!
//! 서브커맨드를 디스패치하고, 프로젝트를 컴파일하고, 목록형 명령을 찍는다.
//! 스펙 6절이 이 파일의 계약이다.
//!
//! **인자 파싱을 직접 한다.** 플래그는 `bless --import` 하나뿐이라 슬라이스 패턴
//! 하나로 충분하다. 플래그가 늘면 그때 인자 파서를 들인다.
//!
//! ponytail: 브리프가 그린 `Command` 열거형을 두지 않는다. 파싱한 뒤 다시 분기하면
//! 같은 명령 목록을 두 곳에 적게 되고, 슬라이스 패턴은 인자 개수까지 한 번에 가른다.
//! 서브커맨드가 인자 파싱 이상의 상태를 갖게 되면 그때 타입으로 올린다.

use kang::ast::{Diagnostic, DocPath, Severity, SymbolKind, SymbolRef};
use kang::check;
use kang::resolve::{self, Project, SymbolId, SymbolTable};
use std::collections::HashMap;

/// `--help` 와 사용법 오류가 함께 쓰는 도움말.
///
/// 인자를 틀린 에이전트가 다음에 하는 일이 `kang --help` 이므로 여기서 명령·인자
/// 형식·종료 코드를 전부 보여줘야 재시도가 성공한다.
const 사용법: &str = "kang — 문서 컴파일러

명령:
  kang init                            에이전트 진입점과 첫 문서 생성
  kang build                           컴파일 및 검증
  kang bless <문서> --import <심볼>    rev 핀 갱신·삽입
  kang list [경로]                     문서 목록과 description
  kang keywords [경로]                 키워드 목록
  kang refs <키워드>                   키워드를 참조하는 topic
  kang show <문서|토픽>                문서/토픽 조회 (YAML)
  kang inspect                         코드 대조 (v2)
  kang --help                          이 도움말

인자 문법:
  인자에 백틱을 쓰지 않습니다. 경로는 / , 키워드는 . , topic 은 # , exception 은 ! 로 잇습니다.
  공백이 있는 이름은 셸 인용이 필요합니다.

  kang refs docs/A.결제
  kang show 'docs/A#결제의 방법'
  kang bless docs/B --import 'docs/A.결제 수단'

종료 코드:
  0  성공
  1  컴파일 error 존재
  2  사용법 오류, 또는 환경 오류 (git 저장소가 아님)
  3  아직 구현되지 않은 기능 (kang inspect 는 v2)";

/// 인자를 서브커맨드로 갈라 실행하고 그 종료 코드로 프로세스를 끝낸다.
fn main() {
    let 인자: Vec<String> = std::env::args().skip(1).collect();
    let 조각: Vec<&str> = 인자.iter().map(String::as_str).collect();

    let 코드 = match 조각.as_slice() {
        // 요청해서 본 도움말은 오류가 아니므로 표준 출력으로 낸다.
        ["--help"] => {
            println!("{사용법}");
            0
        }
        ["build"] => match compile() {
            Ok(_) => 0,
            Err(진단들) => 종료_코드(&진단들),
        },
        ["list"] => 목록(None),
        ["list", 스코프] => 목록(Some(스코프)),
        ["keywords"] => 키워드들(None),
        ["keywords", 스코프] => 키워드들(Some(스코프)),
        ["refs", 키워드] => 참조들(키워드),
        // 아래 셋은 Task 10·11·14 가 본체를 채운다. 지금 사용법 오류로 흘려보내면
        // 에이전트가 있는 명령을 없다고 배운다.
        ["init"] => 미구현("kang init"),
        ["bless", _문서, "--import", _심볼] => 미구현("kang bless"),
        ["show", _대상] => 미구현("kang show"),
        // 존재하지 않는 명령처럼 보이면 에이전트가 철자를 의심하며 재시도한다 (스펙 6절).
        ["inspect"] => 미구현("kang inspect (v2 기능입니다)"),
        // 알 수 없는 명령과 인자 개수 불일치가 함께 여기로 온다.
        _ => {
            // 무엇이 틀렸는지 먼저 알린다. 도움말만 내면 어느 부분을 고칠지 모른 채
            // 같은 줄을 다시 쓴다.
            if 조각.is_empty() {
                eprintln!("명령이 없습니다.");
            } else {
                eprintln!(
                    "알 수 없는 명령이거나 인자가 맞지 않습니다: kang {}",
                    조각.join(" ")
                );
            }
            eprintln!("{사용법}");
            2
        }
    };

    std::process::exit(코드);
}

/// 프로젝트 루트를 찾아 문서를 전부 읽고 파싱한다. 진단 규칙은 돌리지 않는다.
///
/// `bless` 처럼 **error 상태에서 실행되어야 하는 명령**이 쓴다. `bless` 가 필요한
/// 상황은 정의상 전부 error 이므로(핀 없음도 error, 핀 불일치도 error),
/// [`compile`] 을 통과해야 한다면 영원히 실행될 수 없다.
///
/// 진단은 이 함수가 직접 표준 오류로 찍는다 — 부르는 쪽이 전부 같은 일을 하므로
/// 찍는 자리를 하나로 둔다.
///
/// # 반환값
/// 파싱된 프로젝트와 전역 심볼 테이블
///
/// # 오류
/// [`Severity::Error`] 진단이 하나라도 나오면 그때까지 모은 진단 전부
fn parse_project() -> Result<(Project, SymbolTable), Vec<Diagnostic>> {
    // 현재 디렉토리를 잃은 프로세스는 어떤 문서 경로도 해석할 수 없다. 진단 코드를
    // 새로 만들지 않고 환경 오류로 즉시 끝낸다.
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(오류) => {
            eprintln!("현재 디렉토리를 확인하지 못했습니다 — {오류}");
            std::process::exit(2);
        }
    };

    // `find_root` 는 절대 경로를 요구한다. `current_dir()` 은 항상 절대 경로다.
    let root = match resolve::find_root(&cwd) {
        Ok(root) => root,
        // 루트를 못 찾으면 읽을 문서 자체가 없다. 함께 낼 진단도 없으므로 여기서 끝낸다.
        Err(진단) => {
            let 진단들 = vec![진단];
            eprint!("{}", check::report(&진단들));
            return Err(진단들);
        }
    };

    let (project, mut 진단들) = resolve::load(&root);
    let (table, 이름_충돌) = SymbolTable::build(&project);
    진단들.extend(이름_충돌);

    // 문서가 없는 것은 error 가 아니다. 다만 조용히 성공하면 사용자는 명령이 동작한
    // 것인지 알 수 없다. 진단이 있으면 그것이 이미 사정을 설명하므로 겹쳐 말하지 않는다.
    if project.docs.is_empty() && 진단들.is_empty() {
        eprintln!("이 프로젝트에 .kang 문서가 없습니다. kang init 으로 첫 문서를 만드세요.");
    }

    진단_마감(진단들)?;
    Ok((project, table))
}

/// [`parse_project`] 에 진단 규칙 전부를 이어 돌린다.
///
/// **조회 명령은 전부 이 함수를 먼저 거친다.** 통과하지 못한 문서는 어떤 CLI 명령으로도
/// 출력되지 않는다 (스펙 5절).
///
/// # 반환값
/// 파싱된 프로젝트와 전역 심볼 테이블
///
/// # 오류
/// [`Severity::Error`] 진단이 하나라도 나오면 그때까지 모은 진단 전부
fn compile() -> Result<(Project, SymbolTable), Vec<Diagnostic>> {
    let (project, table) = parse_project()?;

    // 검사 **사이**의 순서는 구조 → 이름 → 상태 → 핀이다. 앞의 것이 틀리면 뒤의 것을
    // 읽을 이유가 없는 순서다 — 순환이 있으면 문서 구조부터 갈라야 하고, 이름이
    // 해석되지 않으면 예외 짝 맞추기가 성립하지 않는다. 핀은 고치는 방법이 `bless`
    // 하나로 기계적이라 마지막이다. 각 검사의 내부 순서는 이미 문서 경로 순이므로
    // 이 호출 순서가 곧 실행마다 같은 출력 순서다.
    let mut 진단들 = check::check_cycles(&project);
    진단들.extend(check::check_symbols(&project, &table));
    진단들.extend(check::check_exceptions(&project, &table));
    진단들.extend(check::check_revs(&project, &table));

    진단_마감(진단들)?;
    Ok((project, table))
}

/// 모은 진단을 찍고, error 가 있으면 실패로 마감한다.
///
/// 실패 판정은 **진단 개수가 아니라 [`Severity::Error`] 개수**로 한다. `K031`
/// (pending 인데 cover 없음)이 저장소에서 유일한 [`Severity::Warn`] 이며,
/// 스펙 5.2 가 그 칸을 통과로 정했다. 비어 있는지로 판정하면 그 진단이 사용자에게 한
/// "빌드를 실패시키지 않습니다" 라는 약속이 거짓이 된다.
///
/// # 매개변수
/// - `진단들`: 지금까지 모은 진단 전부
///
/// # 오류
/// error 가 하나라도 있으면 받은 진단 전부
fn 진단_마감(진단들: Vec<Diagnostic>) -> Result<(), Vec<Diagnostic>> {
    // 진단은 문서가 아니라 사람과 에이전트에게 가는 말이므로 표준 오류로 낸다.
    // 그래야 error 가 있을 때 표준 출력이 비어 있다는 규약이 유지된다.
    if !진단들.is_empty() {
        eprint!("{}", check::report(&진단들));
    }

    // warn 은 이미 찍혔고 빌드를 실패시키지 않는다.
    if 진단들.iter().any(|진단| 진단.severity == Severity::Error) {
        return Err(진단들);
    }

    Ok(())
}

/// 컴파일 실패의 종료 코드를 정한다.
///
/// `K050`(git 저장소가 아님)만이 **환경 오류**다. 문서가 틀린 것이 아니므로 1 을 주면
/// 에이전트가 고칠 것 없는 문서를 뒤진다.
///
/// # 매개변수
/// - `진단들`: 컴파일이 돌려준 진단
///
/// # 반환값
/// 환경 오류면 `2`, 그 밖의 컴파일 error 면 `1`
fn 종료_코드(진단들: &[Diagnostic]) -> i32 {
    if 진단들.iter().any(|진단| 진단.code == "K050") {
        2
    } else {
        1
    }
}

/// 아직 구현되지 않은 명령임을 알린다.
///
/// 사용법 오류로 흘려보내면 에이전트가 **있는 명령을 없다고 배운다.** 종료 코드 3 은
/// "kang 이 아직 못 하는 일" 이며, 명령줄을 고쳐 재시도할 것이 없다는 뜻이다.
///
/// # 매개변수
/// - `명령`: 사용자가 부른 명령
///
/// # 반환값
/// 종료 코드 `3`
fn 미구현(명령: &str) -> i32 {
    eprintln!("아직 구현되지 않은 명령입니다: {명령}");
    3
}

/// 문서 목록과 description 을 한 줄씩 찍는다 (스펙 6.3).
///
/// # 매개변수
/// - `스코프`: 경로 스코프. `None` 이면 프로젝트 전체
///
/// # 반환값
/// 프로세스 종료 코드
fn 목록(스코프: Option<&str>) -> i32 {
    let (project, _) = match compile() {
        Ok(결과) => 결과,
        Err(진단들) => return 종료_코드(&진단들),
    };

    let 조각 = 경로_조각(스코프);
    // 문서를 경로 순으로 훑는다. 경로는 계층 축약 없이 전체 경로로 찍는다.
    for path in 정렬된_경로(&project) {
        // 스코프 밖의 문서는 건너뛴다.
        if !path.0.starts_with(&조각) {
            continue;
        }
        println!("{path}: {}", project.docs[path].description);
    }

    0
}

/// 키워드 목록을 한 줄씩 찍는다 (스펙 6.3).
///
/// 스코프는 **경로 스코프만** 지원한다.
///
/// # 매개변수
/// - `스코프`: 경로 스코프. `None` 이면 프로젝트 전체
///
/// # 반환값
/// 프로세스 종료 코드
fn 키워드들(스코프: Option<&str>) -> i32 {
    let (project, _) = match compile() {
        Ok(결과) => 결과,
        Err(진단들) => return 종료_코드(&진단들),
    };

    let 조각 = 경로_조각(스코프);
    // 문서를 경로 순으로 훑는다.
    for path in 정렬된_경로(&project) {
        // 스코프 밖의 문서는 건너뛴다.
        if !path.0.starts_with(&조각) {
            continue;
        }
        // 한 문서 안의 키워드는 선언 순서 그대로 낸다.
        for keyword in &project.docs[path].keywords {
            println!(
                "{path}.{}: {}",
                keyword.name.0.join("."),
                keyword.definition
            );
        }
    }

    0
}

/// 키워드를 참조하는 topic 을 한 줄씩 찍는다 (스펙 6.3·6.5).
///
/// # 매개변수
/// - `주소`: `docs/A.결제` 꼴의 키워드 주소. 백틱을 쓰지 않는다 (스펙 6.0)
///
/// # 반환값
/// 프로세스 종료 코드
fn 참조들(주소: &str) -> i32 {
    let (project, table) = match compile() {
        Ok(결과) => 결과,
        Err(진단들) => return 종료_코드(&진단들),
    };

    // 마지막 `/` 뒤가 문서 이름과 키워드이고, 그 둘은 첫 `.` 로 갈린다.
    // 남은 `.` 은 키워드의 계층이다.
    let (디렉토리, 마지막) = 주소.rsplit_once('/').unwrap_or(("", 주소));
    let Some((문서명, 이름)) = 마지막.split_once('.') else {
        eprintln!("키워드 주소가 아닙니다: {주소}");
        eprintln!("문서 경로와 키워드를 점으로 이어 적으세요. 예: kang refs docs/A.결제");
        return 2;
    };
    let mut 조각 = 경로_조각(Some(디렉토리));
    조각.push(문서명.to_string());

    let 참조 = SymbolRef {
        doc: DocPath(조각),
        kind: SymbolKind::Keyword,
        name: 이름.split('.').map(str::to_string).collect(),
    };
    // 없는 키워드를 빈 결과로 돌려주면 "아무도 참조하지 않는다" 와 구분할 수 없다.
    let Some(대상) = table.resolve(&참조) else {
        eprintln!("그런 키워드가 없습니다: {주소}");
        eprintln!("kang keywords 로 선언된 키워드를 확인하세요.");
        return 2;
    };

    // 문서를 경로 순으로, 그 안의 topic 은 선언 순서로 훑는다.
    for path in 정렬된_경로(&project) {
        let scope = table.scope(path);
        for topic in &project.docs[path].topics {
            // 참조는 alias 를 거칠 수 있으므로 이름이 아니라 심볼로 맞춘다.
            if 가리키는가(&topic.refs, &scope, 대상) {
                println!("{path}#{}", topic.name);
            }
        }
    }

    0
}

/// topic 본문의 백틱 조각들이 대상 심볼을 가리키는지 본다.
///
/// [`kang::ast::Topic::refs`] 는 백틱 쌍 하나가 조각 하나이므로 계층 이름
/// `` `결제수단`.`카드` `` 는 두 항목으로 들어온다. 같은 줄에서 이어지는 조각만 하나의
/// 이름이 될 수 있고(줄이 바뀌면 원문에서 `.` 로 이어져 있을 수 없다), 긴 이름부터
/// 맞춰야 `` `결제수단`.`카드` `` 가 `결제수단` 으로 끊기지 않는다.
///
/// ponytail: 탐욕 최장 일치다. 긴 이름을 잡은 탓에 남은 조각이 해석되지 않는 분할
/// (스코프에 `A`·`A.B`·`B.C` 가 함께 있고 본문이 `` `A`.`B`.`C` `` 인 경우)에서는
/// [`kang::check`] 의 분할과 갈려 그 topic 을 놓치거나 더 낸다. 그 형태가 실제로
/// 나타나면 check 층과 같은 표를 채우는 분할로 올린다.
///
/// # 매개변수
/// - `조각들`: topic 이 언급한 백틱 조각과 등장 줄
/// - `scope`: 그 문서에서 쓸 수 있는 로컬 이름들
/// - `대상`: 찾는 심볼
///
/// # 반환값
/// 하나라도 대상을 가리키면 `true`
fn 가리키는가(
    조각들: &[(String, usize)],
    scope: &HashMap<String, SymbolId>,
    대상: SymbolId,
) -> bool {
    let 이음 = |구간: &[(String, usize)]| {
        구간
            .iter()
            .map(|(이름, _)| 이름.as_str())
            .collect::<Vec<&str>>()
            .join(".")
    };

    let mut 자 = 0;
    // 조각을 앞에서부터 이름 단위로 끊어 가며 대상을 만나는지 본다.
    while 자 < 조각들.len() {
        let 줄 = 조각들[자].1;
        let 끝 = 자
            + 조각들[자..]
                .iter()
                .take_while(|(_, 그_줄)| *그_줄 == 줄)
                .count();
        let 길이 = (1..=끝 - 자)
            .rev()
            .find(|&길이| scope.contains_key(&이음(&조각들[자..자 + 길이])))
            .unwrap_or(1);
        if scope.get(&이음(&조각들[자..자 + 길이])) == Some(&대상) {
            return true;
        }
        자 += 길이;
    }

    false
}

/// 경로 스코프 인자를 경로 조각들로 나눈다.
///
/// 빈 조각은 버린다 — `docs/` 처럼 슬래시로 끝나는 인자가 아무것도 맞지 않는 빈 결과로
/// 조용히 끝나면 사용자는 그런 문서가 없다고 잘못 배운다.
///
/// # 매개변수
/// - `스코프`: 경로 스코프 인자
///
/// # 반환값
/// 경로 조각들. 스코프가 없으면 빈 목록이며 그때 모든 문서가 맞는다
fn 경로_조각(스코프: Option<&str>) -> Vec<String> {
    스코프.map_or_else(Vec::new, |스코프| {
        스코프
            .split('/')
            .filter(|조각| !조각.is_empty())
            .map(str::to_string)
            .collect()
    })
}

/// 프로젝트의 문서 경로를 정렬해 돌려준다.
///
/// # 매개변수
/// - `project`: 파싱을 마친 프로젝트
///
/// # 반환값
/// 경로 순으로 정렬된 문서 경로들
fn 정렬된_경로(project: &Project) -> Vec<&DocPath> {
    // HashMap 의 나열 순서는 보장되지 않는다. 정렬해야 출력이 실행마다 같다.
    let mut 경로들: Vec<&DocPath> = project.docs.keys().collect();
    // DocPath 는 Vec<String> 래퍼다. 조각을 그대로 비교하면 비교마다 String 을 만들지 않는다.
    경로들.sort_by(|a, b| a.0.cmp(&b.0));
    경로들
}
