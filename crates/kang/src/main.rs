//! kang CLI 진입점.
//!
//! 서브커맨드를 디스패치하고, 프로젝트를 컴파일하고, 목록형 명령을 찍는다.
//! 스펙 6절이 이 파일의 계약이다.
//!
//! **인자 파싱을 직접 한다.**
//! ponytail: 플래그는 `bless --import` 하나뿐이라 슬라이스 패턴 하나로 충분하다.
//! 플래그가 늘면 — Task 10 의 `show`, Task 11 의 `bless` 가 그 자리다 — 그때 인자 파서를
//! 들인다. 지금 들이면 가지 하나에만 쓰는 의존성이 된다.
//!
//! ponytail: 브리프가 그린 `Command` 열거형을 두지 않는다. 파싱한 뒤 다시 분기하면
//! 같은 명령 목록을 두 곳에 적게 되고, 슬라이스 패턴은 인자 개수까지 한 번에 가른다.
//! 서브커맨드가 인자 파싱 이상의 상태를 갖게 되면 그때 타입으로 올린다.

use kang::ast::{Diagnostic, DocPath, Severity, SymbolKind};
use kang::bless::{self, ImportAddress};
use kang::check;
use kang::index;
use kang::init;
use kang::resolve::{self, Project, SymbolTable};
use kang::show::{self, ShowTarget};
use std::io::Write;
use std::path::PathBuf;

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
  kang index <경로>                    심볼 인덱스 산출 (탭 구분)
  kang types <경로>                    TypeScript 타입 산출 (topic)
  kang --help                          이 도움말

아직 구현되지 않은 명령 (부르면 종료 코드 3 이며, 다른 방법이 없습니다):
  kang inspect                         코드 대조 (v2)

인자 문법:
  인자에 백틱을 쓰지 않습니다. 경로는 / , 키워드는 . , topic 은 # , exception 은 ! 로 잇습니다.
  공백이 있는 이름은 셸 인용이 필요합니다.

  kang refs docs/A.결제
  kang show 'docs/A#결제의 방법'
  kang bless docs/B --import 'docs/A.결제 수단'

종료 코드:
  0  성공
  1  컴파일 error 존재
  2  사용법 오류, 또는 환경 오류 (git 저장소가 아님, 문서를 읽지 못함, 출력을 쓰지 못함)
  3  아직 구현되지 않은 기능 (kang inspect)";

/// 인자를 서브커맨드로 갈라 실행하고 그 종료 코드로 프로세스를 끝낸다.
fn main() {
    // `std::env::args()` 는 잘못된 유니코드 인자에 **패닉**한다. 인자 파싱은 도구의
    // 최외곽 신뢰 경계이고 그 위에 아무것도 없으므로, 여기서 죽으면 진단도 종료 코드도
    // 남지 않는다. 로더가 비 UTF-8 파일 이름을 견디는데(`resolve::문서경로`) 그 앞의
    // 인자 파서가 죽으면 안 된다.
    let Ok(인자) = std::env::args_os()
        .skip(1)
        .map(std::ffi::OsString::into_string)
        .collect::<Result<Vec<String>, _>>()
    else {
        // 어떤 명령 이름도 심볼 주소도 UTF-8 이 아닐 수 없다. 답은 사용법 오류다.
        eprintln!("인자가 UTF-8 이 아닙니다.");
        eprintln!("{사용법}");
        std::process::exit(2);
    };
    let 조각: Vec<&str> = 인자.iter().map(String::as_str).collect();

    // **플래그는 자기 자리에서만 플래그다.** 이름만 보고 허용하면 `kang index --help` 가
    // "경로가 `--help` 인 인덱스" 로 읽혀 그 이름의 파일이 생긴다 (실제로 생겼다).
    // 플래그처럼 보이는 것을 위치 인자로 삼키는 명령은 **도구가 사용자의 오타를 파일로
    // 만드는 자리**다.
    //
    // kang 의 플래그는 둘이고 각각 자리가 하나뿐이다 — `--help` 는 단독 인자,
    // `--import` 는 `bless` 의 셋째 인자. 그 밖의 `-` 로 시작하는 인자는 전부 사용법
    // 오류다. 문서·심볼 이름이 `-` 로 시작하면 CLI 로 가리킬 수 없고, 그때 처방은
    // 스펙 6.0 과 같다 — 주소를 댈 수 없는 이름은 이름을 고친다.
    let 제자리_플래그 = |자리: usize| match 조각[자리] {
        "--help" => 조각.len() == 1,
        "--import" => 자리 == 2 && 조각.first() == Some(&"bless"),
        _ => false,
    };
    if let Some(자리) =
        (0..조각.len()).find(|자리| 조각[*자리].starts_with('-') && !제자리_플래그(*자리))
    {
        eprintln!("모르는 플래그입니다: {}", 조각[자리]);
        eprintln!("{사용법}");
        std::process::exit(2);
    }

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
        ["show", 대상] => 조회(대상),
        ["bless", 문서, "--import", 심볼] => 축복(문서, 심볼),
        ["init"] => 초기화(),
        ["index", 경로] => 산출(
            경로,
            "인덱스",
            index::write_index,
            index::인덱스_형식인가,
            "tsv.tmp",
        ),
        ["types", 경로] => 산출(
            경로,
            "타입",
            index::write_types,
            index::타입_형식인가,
            "ts.tmp",
        ),
        // v1 에 없는 것과 **v1 이 만들지 않기로 한 것**은 다르다. 앞의 것은 다음 빌드를
        // 기다리면 되지만 이것은 기다려도 오지 않는다 (스펙 6절).
        ["inspect"] => 미구현("kang inspect", "v2 기능이며 아직 구현되지 않았습니다."),
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

/// 프로젝트 루트를 찾아 문서를 전부 읽고 파싱하고 심볼 테이블을 세운다.
/// [`kang::check`] 의 규칙(순환·심볼·예외·rev 핀)은 돌리지 않는다.
///
/// **진단이 없다는 뜻은 아니다.** 루트를 못 찾거나(`K050`) 파일을 읽지 못하거나(`K051`)
/// 문법이 틀리거나(`K1xx`) 한 문서가 같은 로컬 이름을 두 번 묶으면(`K052`) 여기서 이미
/// error 로 끝난다. 그 넷은 전부 "문서를 읽을 수 없다" 이지 "문서가 규칙을 어겼다" 가 아니다.
///
/// `bless` 처럼 **error 상태에서 실행되어야 하는 명령**이 쓴다. `bless` 가 필요한
/// 상황은 정의상 전부 error 이므로(핀 없음도 error, 핀 불일치도 error),
/// [`compile`] 을 통과해야 한다면 영원히 실행될 수 없다.
///
/// 진단은 이 함수가 직접 표준 오류로 찍는다 — 부르는 쪽이 전부 같은 일을 하므로
/// 찍는 자리를 하나로 둔다.
///
/// # 반환값
/// 프로젝트 루트와, 파싱된 프로젝트와 전역 심볼 테이블.
/// **루트를 함께 돌려준다** — [`축복`] 이 문서 파일 경로를 조립하려면 필요하고,
/// 여기서 이미 찾았으므로 부르는 쪽이 다시 찾으면 같은 일을 두 번 한다
///
/// # 오류
/// [`Severity::Error`] 진단이 하나라도 나오면 그때까지 모은 진단 전부
fn parse_project() -> Result<(PathBuf, Project, SymbolTable), Vec<Diagnostic>> {
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
            진단_찍기(&진단들);
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
    Ok((root, project, table))
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
    // 루트는 조회 명령에 쓸모가 없다. 파일을 고쳐 쓰는 `bless` 만 필요로 한다.
    let (_root, project, table) = parse_project()?;

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
        진단_찍기(&진단들);
    }

    // warn 은 이미 찍혔고 빌드를 실패시키지 않는다.
    if 진단들.iter().any(|진단| 진단.severity == Severity::Error) {
        return Err(진단들);
    }

    Ok(())
}

/// 컴파일 실패의 종료 코드를 정한다.
///
/// **환경 오류는 둘이다** — `K050`(git 저장소가 아님)과 `K051`(문서를 읽지 못함).
/// 문서가 규칙을 어긴 것이 아니므로 1 을 주면 에이전트가 고칠 것 없는 문서를 뒤진다.
/// 두 진단의 `fix` 가 `git init`·`ls -l`·`file -I` 인 것이 그 사실을 이미 말한다
/// (스펙 6절, 5.1.1 의 확인용 `fix`).
///
/// **`K051` 은 다른 error 와 함께 날 수 있고 그때도 2 다.** 읽지 못한 파일이 있는
/// 컴파일은 애초에 전부를 보지 못한 컴파일이므로, 문서를 고치기 전에 환경을 먼저
/// 세워야 한다. 환경을 세운 뒤 남은 문서 오류가 1 로 나온다.
///
/// # 매개변수
/// - `진단들`: 컴파일이 돌려준 진단
///
/// # 반환값
/// 환경 오류면 `2`, 그 밖의 컴파일 error 면 `1`
fn 종료_코드(진단들: &[Diagnostic]) -> i32 {
    if 진단들
        .iter()
        .any(|진단| 진단.code == "K050" || 진단.code == "K051")
    {
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
/// - `사정`: 왜 아직 쓸 수 없는지 한 문장. v1 이 곧 채울 명령과 v2 로 미룬 명령을 가른다
///
/// # 반환값
/// 종료 코드 `3`
fn 미구현(명령: &str, 사정: &str) -> i32 {
    eprintln!("아직 쓸 수 없는 명령입니다: {명령} — {사정}");
    3
}

/// 에이전트 진입점과 첫 문서를 만든다 (스펙 6.1).
///
/// **git 저장소를 요구하지 않는다.** `init` 은 갓 만든 디렉토리에서 실행되는 첫 명령이므로
/// 여기서 종료 코드 2 로 죽으면 T0 에서 벽을 만든다. 저장소가 아니면 현재 디렉토리를
/// 루트로 삼고 `git init` 안내를 함께 낸다 — `build` 는 저장소를 요구하기 때문이다
/// (스펙 3절). 그래서 `K050` 진단을 찍지 않는다. 찍으면 에이전트가 성공한 명령에서
/// 고칠 것을 찾는다.
///
/// **출력은 전부 표준 오류다.** `init` 은 파일을 만들 뿐 데이터를 내지 않으므로 [`축복`]
/// 과 같은 규약을 따른다. 산출물마다의 처리는 [`kang::init::init`] 이 직접 찍는다.
///
/// # 반환값
/// 프로세스 종료 코드
fn 초기화() -> i32 {
    // 현재 디렉토리를 잃은 프로세스는 루트를 정할 수 없다. `parse_project` 와 같은 답이다.
    let cwd = std::env::current_dir().unwrap_or_else(|오류| {
        eprintln!("현재 디렉토리를 확인하지 못했습니다 — {오류}");
        std::process::exit(2);
    });

    // 하위 디렉토리에서 불러도 파일은 저장소 루트에 만들어져야 한다 (스펙 3절).
    let (root, git_없음) = match resolve::find_root(&cwd) {
        Ok(root) => (root, false),
        Err(_) => (cwd, true),
    };

    let 만든것 = match init::init(&root) {
        Ok(만든것) => 만든것,
        // 실패는 전부 IO 다. 문서가 규칙을 어긴 것이 아니므로 환경 오류로 2 다.
        Err(사유) => {
            eprintln!("{사유}");
            return 2;
        }
    };

    // 하나도 만들지 않았으면 그렇다고 말한다. 침묵하면 무엇이 일어났는지 알 수 없다.
    if 만든것.is_empty() {
        eprintln!("이미 초기화되어 있습니다. 바꾼 것이 없습니다.");
    }

    // 여기서 안내하지 않으면 다음 명령인 `kang build` 가 `K050` 으로 막힌다.
    if git_없음 {
        eprintln!(
            "이 디렉토리는 git 저장소가 아닙니다. kang build 는 저장소 루트를 프로젝트 루트로 삼으므로 git init 을 먼저 실행하세요."
        );
    }

    0
}

/// import 의 rev 핀을 갱신하거나 삽입한다 (스펙 6.2).
///
/// **[`compile`] 이 아니라 [`parse_project`] 를 쓴다.** `bless` 가 필요한 상황은 정의상
/// 전부 error 이므로(`K020`·`K021`) 컴파일 통과를 요구하면 영원히 실행될 수 없다.
/// 문서를 **읽지 못한** 경우(`K050`·`K051`·`K052`·`K1xx`)만 막는다.
///
/// 성공 알림은 표준 오류로 낸다. `bless` 는 데이터를 내지 않는 명령이므로 표준 출력은
/// 비어 있어야 하고, 파일을 고쳐 쓰고 조용히 끝나면 무엇이 바뀌었는지 알 길이 없다.
///
/// # 매개변수
/// - `문서`: 고쳐 쓸 문서 경로. 백틱을 쓰지 않는다 (스펙 6.0)
/// - `심볼`: 갱신할 import 의 심볼 주소. `K020`·`K021` 의 fix 가 내는 것과 같은 형식이다
///
/// # 반환값
/// 프로세스 종료 코드
/// 컴파일한 프로젝트에서 산출물 하나를 만들어 파일로 낸다 (V0004 Task 3·7).
///
/// **error 가 있으면 쓰지 않고 끝낸다.** 깨진 프로젝트의 인덱스를 proc-macro 가, 타입을
/// `tsc` 가 읽으면 거짓을 검증하게 된다 — 스펙 6절이 조회 명령에 요구하는 것과 같은
/// 규칙이다.
///
/// stdout 은 비운다. 산출물은 파일이고, 표준 출력으로도 내면 소비자가 둘 중 어느 것을
/// 읽을지 갈린다.
///
/// **`index` 와 `types` 가 이 하나를 공유한다.** 원자성 규약과 "error 면 쓰지 않는다" 를
/// 두 벌 두면 한쪽만 고쳐지는 날이 온다.
///
/// # 매개변수
/// - `경로`: 산출물을 쓸 파일 경로. 현재 디렉토리 기준으로 해석한다
/// - `무엇`: 진단에 쓰는 산출물 이름 (`인덱스`·`타입`)
/// - `생성기`: 프로젝트를 산출물 바이트로 바꾸는 함수
/// - `임시_확장자`: 원자적 쓰기의 임시 파일 확장자. 원래 확장자를 앞에 남긴다
///
/// # 반환값
/// 프로세스 종료 코드. 성공 0 / 컴파일 error 1 / 쓰기 실패 2
fn 산출(
    경로: &str,
    무엇: &str,
    생성기: fn(&Project, &SymbolTable, &mut Vec<u8>) -> std::io::Result<()>,
    내_산출물인가: fn(&str) -> bool,
    임시_확장자: &str,
) -> i32 {
    let (project, table) = match compile() {
        Ok(결과) => 결과,
        Err(진단들) => return 종료_코드(&진단들),
    };

    let mut 내용 = Vec::new();
    // 메모리 버퍼에 쓰므로 실패할 수 없다. 그래도 삼키지 않는다 — 삼키면 빈 산출물이
    // 성공으로 나가고 소비자는 "심볼이 없는 프로젝트" 로 읽는다.
    if let Err(오류) = 생성기(&project, &table, &mut 내용) {
        eprintln!("{무엇}을 만들지 못했습니다 — {오류}");
        return 2;
    }
    let 내용 = match String::from_utf8(내용) {
        Ok(내용) => 내용,
        Err(오류) => {
            eprintln!("{무엇}이 UTF-8 이 아닙니다 — {오류}");
            return 2;
        }
    };

    let 파일 = std::path::PathBuf::from(경로);

    // **이미 있는 파일이 내 산출물로 보이지 않으면 덮어쓰지 않는다.** 경로를 그대로 받아
    // 쓰므로 `kang types Cargo.toml` 이나 `kang index docs/policy.kang` 한 번이 그 파일을
    // 지운다. 산출물은 언제든 다시 만들 수 있지만 남의 파일은 그렇지 않고 되돌릴 방법이
    // 없다. 재실행은 정상 사용이므로 내가 낸 것으로 보이면 그대로 덮어쓴다.
    if let Ok(기존) = std::fs::read_to_string(&파일)
        && !내_산출물인가(&기존)
    {
        eprintln!(
            "그 자리에 kang 이 만들지 않은 파일이 있어 {무엇}을 쓰지 않았습니다 ({}).\n\n  fix:\n    [edit]  다른 경로를 주거나, 그 파일이 필요 없다면 지우고 다시 실행하세요",
            파일.display()
        );
        return 2;
    }

    // 상위 디렉토리가 없으면 만든다. `.kang/index.tsv` 처럼 새 디렉토리에 두는 것이
    // 정상 사용이고, 여기서 실패하면 원자적 쓰기가 임시 파일부터 실패한다.
    match 파일.parent() {
        // 경로에 디렉토리 조각이 있으면 만든다. 빈 부모는 `index.tsv` 처럼 현재
        // 디렉토리에 바로 두는 경우이며 만들 것이 없다.
        Some(상위) if !상위.as_os_str().is_empty() => {
            if let Err(오류) = std::fs::create_dir_all(상위) {
                eprintln!(
                    "{무엇}을 둘 디렉토리를 만들지 못했습니다 ({}) — {오류}",
                    상위.display()
                );
                return 2;
            }
        }
        _ => {}
    }

    // 쓰기는 `bless` 와 같은 함수를 쓴다. 원자성 규약의 두 번째 사본을 만들면 한쪽만
    // 고쳐지는 날이 온다.
    if let Err(사유) = bless::쓰기_원자적으로(&파일, &내용, 임시_확장자) {
        eprintln!("{무엇}을 쓰지 못했습니다: {사유}");
        return 2;
    }
    0
}

fn 축복(문서: &str, 심볼: &str) -> i32 {
    // 주소부터 본다. 인자의 **모양**이 틀린 것은 프로젝트를 읽기 전에 답이 나오는
    // 사용법 오류이고, 그때 도움말을 함께 내는 것이 에이전트의 재시도 경로다.
    let addr = match ImportAddress::parse(심볼) {
        Ok(addr) => addr,
        Err(사유) => {
            eprintln!("{사유}");
            eprintln!("{사용법}");
            return 2;
        }
    };

    let (root, project, table) = match parse_project() {
        Ok(결과) => 결과,
        Err(진단들) => return 종료_코드(&진단들),
    };

    let path = DocPath(경로_조각(Some(문서)));
    // 실패는 전부 "그런 문서·import·대상이 없다" 또는 IO 다. 문서가 규칙을 어긴 것이
    // 아니므로 1 이 아니라 2 다 — `refs`·`show` 가 없는 주소에 주는 코드와 같다.
    let 고쳤나 = match bless::bless(&project, &table, &root, &path, &addr) {
        Ok(고쳤나) => 고쳤나,
        Err(사유) => {
            eprintln!("rev 핀을 갱신하지 못했습니다 — 문서 {문서}, import {심볼}: {사유}");
            return 2;
        }
    };

    // **바뀐 게 없는데 갱신했다고 하면 검증하면 거짓이다.** 스펙 4.8·6.2 는 삽입과
    // 갱신을 편집으로 부르고, 이미 맞는 핀에 재실행하는 것은 그 어느 쪽도 아니다.
    if 고쳤나 {
        eprintln!("rev 핀을 갱신했습니다 — 문서 {문서}, import {심볼}");
    } else {
        eprintln!("이미 최신 핀입니다. 바꾼 것이 없습니다 — 문서 {문서}, import {심볼}");
    }
    0
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
    let mut out = std::io::stdout().lock();
    let mut 맞은_문서 = 0;
    // 문서를 경로 순으로 훑는다. 경로는 계층 축약 없이 전체 경로로 찍는다.
    for path in project.경로들() {
        // 스코프 밖의 문서는 건너뛴다.
        if !path.0.starts_with(&조각) {
            continue;
        }
        맞은_문서 += 1;
        if let Some(코드) = 찍기(
            &mut out,
            &format!("{path}: {}", project.docs[path].description),
        ) {
            return 코드;
        }
    }

    빈_스코프_안내(스코프, 맞은_문서);
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
    let mut out = std::io::stdout().lock();
    let mut 맞은_문서 = 0;
    // 문서를 경로 순으로 훑는다.
    for path in project.경로들() {
        // 스코프 밖의 문서는 건너뛴다.
        if !path.0.starts_with(&조각) {
            continue;
        }
        맞은_문서 += 1;
        // 한 문서 안의 키워드는 선언 순서 그대로 낸다.
        for keyword in &project.docs[path].keywords {
            let 줄 = format!(
                "{path}.{}: {}",
                keyword.name.0.join("."),
                keyword.definition
            );
            if let Some(코드) = 찍기(&mut out, &줄) {
                return 코드;
            }
        }
    }

    빈_스코프_안내(스코프, 맞은_문서);
    0
}

/// 경로 스코프가 아무 문서도 맞히지 못했음을 알린다.
///
/// 조용히 빈 출력으로 끝나면 오타 하나가 "이 디렉토리에는 문서가 없다" 는 결론이 된다.
/// 같은 이유로 [`참조들`] 은 없는 키워드를 빈 결과로 돌려주지 않고, [`경로_조각`] 은
/// 슬래시로 끝나는 인자를 빈 결과로 만들지 않는다.
///
/// **판정은 맞은 문서 수로 한다.** 출력 줄 수로 하면 `kang keywords docs` 처럼 문서는
/// 있고 키워드만 없는 **합법 상태**에 거짓 안내가 붙는다.
///
/// **종료 코드는 바꾸지 않는다.** 필터가 빈 결과를 내는 것은 오류가 아니고, 스펙 6절의
/// 종료 코드 표는 사용법 오류와 환경 오류로 닫혀 있어 새 칸을 지어낼 근거가 없다.
///
/// # 매개변수
/// - `스코프`: 사용자가 준 경로 스코프. `None` 이면 필터가 없었으므로 알릴 것도 없다
/// - `맞은_문서`: 스코프에 걸린 문서 수
fn 빈_스코프_안내(스코프: Option<&str>, 맞은_문서: usize) {
    // 스코프를 준 사람만이 그것이 빗나갔다는 사실을 알아야 한다.
    if let Some(스코프) = 스코프
        && 맞은_문서 == 0
    {
        eprintln!("경로 스코프에 맞는 문서가 없습니다: {스코프}");
        eprintln!("kang list 로 문서 경로를 확인하세요.");
    }
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

    // **주소 파싱은 [`ImportAddress::parse`] 하나가 한다.** 여기서 손으로 다시 가르면
    // `K020` 의 `[shell] fix` 가 낸 주소를 `bless` 는 받고 `refs` 는 거절하는 어긋남이
    // 생기고, 스펙 6.0 주소 문법의 천장이 세 곳에 흩어진다. 천장 설명은 그 함수에 있다.
    let 참조 = match ImportAddress::parse(주소) {
        Ok(addr) if addr.target.kind == SymbolKind::Keyword => addr.target,
        // 주소가 아니거나 키워드 주소가 아니다. 둘 다 인자의 **모양** 문제이므로 사용법
        // 오류다 — 도움말이 에이전트의 첫 접점이라는 규약이 이 분기에서만 깨지면 안 된다.
        _ => {
            eprintln!("키워드 주소가 아닙니다: {주소}");
            eprintln!("문서 경로와 키워드를 점으로 이어 적으세요. 예: kang refs docs/A.결제");
            eprintln!("{사용법}");
            return 2;
        }
    };
    // 없는 키워드를 빈 결과로 돌려주면 "아무도 참조하지 않는다" 와 구분할 수 없다.
    let Some(대상) = table.resolve(&참조) else {
        eprintln!("그런 키워드가 없습니다: {주소}");
        eprintln!("kang keywords 로 선언된 키워드를 확인하세요.");
        return 2;
    };

    // 문서를 경로 순으로, 그 안의 topic 은 선언 순서로 훑는다.
    let mut out = std::io::stdout().lock();
    for path in project.경로들() {
        let scope = table.scope(path);
        for topic in &project.docs[path].topics {
            // 분할은 진단을 내는 층과 **같은 함수**를 쓴다. 두 층이 같은 문장을 다르게
            // 읽으면 빌드가 통과하는 문서에서 조회가 조용히 틀린 답을 낸다.
            // 참조는 alias 를 거칠 수 있으므로 이름이 아니라 심볼로 맞춘다.
            let 가리킨다 = check::이름_분할(&topic.refs, &scope)
                .iter()
                .any(|(이름, _)| scope.get(이름) == Some(&대상));
            // 이 topic 이 대상을 가리키지 않으면 찍을 것이 없다.
            if !가리킨다 {
                continue;
            }
            if let Some(코드) = 찍기(&mut out, &format!("{path}#{}", topic.name)) {
                return 코드;
            }
        }
    }

    0
}

/// 문서나 topic 을 관계 정보까지 펼친 YAML 로 찍는다 (스펙 6.4).
///
/// **`show` 가 `cat` 보다 쓸모 있어야 kang 의 원칙이 성립한다.** 원본 `.kang` 은 import
/// 간접 참조 때문에 마크다운보다 읽기 나쁘므로, 여기서 평탄화된 완결 뷰를 주지 못하면
/// 도구를 도입하고 오히려 나빠진다 (스펙 6.1).
///
/// # 매개변수
/// - `주소`: `docs/A` 또는 `docs/A#결제의 방법` 꼴의 주소. 백틱을 쓰지 않는다 (스펙 6.0)
///
/// # 반환값
/// 프로세스 종료 코드
fn 조회(주소: &str) -> i32 {
    let (project, table) = match compile() {
        Ok(결과) => 결과,
        Err(진단들) => return 종료_코드(&진단들),
    };

    // **마지막 `/` 뒤에서 가른다** (스펙 6.0 주소 분할 규칙). 그 앞은 전부 디렉토리이므로
    // 디렉토리 이름의 `#` 에서 갈리지 않는다. 여기서만 전체 문자열의 첫 `#` 를 보면
    // `list`·`refs` 가 받는 문서를 `show` 만 거절한다.
    //
    // **[`ImportAddress::parse`] 와 같은 것은 `/` 처리뿐이다.** 그 함수는 `.`·`#`·`!` 중
    // 가장 앞선 것을 구분자로 삼지만 `show` 가 받는 주소에는 topic `#` 뿐이므로 여기는
    // `#` 만 본다. 그래서 문서 이름의 `.` 은 여기서 그대로 통과하고, 그 차이는 `K113` 이
    // 문서 파일 이름의 구분자를 거절하면서 닫힌다.
    //
    // 그 함수를 그대로 부르지는 못한다. `show` 는 구분자가 없는 문서 주소(`docs/A`)도
    // 받는데 `parse` 는 그것을 오류로 낸다.
    //
    // ponytail: 문서 **파일 이름**에 `#` 이 있으면 여전히 그 자리에서 갈린다. 스펙 6.0 이
    // 그 이름을 금지하기로 정했고, 그것을 거절하는 `K113` 은 문서를 로드하는 층의 몫이다.
    let (디렉토리, 마지막) = 주소.rsplit_once('/').unwrap_or(("", 주소));
    let (문서명, 토픽) = match 마지막.split_once('#') {
        Some((문서명, 토픽)) => (문서명, Some(토픽)),
        None => (마지막, None),
    };

    let mut 조각 = 경로_조각(Some(디렉토리));
    조각.push(문서명.to_string());
    let path = DocPath(조각);
    // 없는 주소를 빈 출력으로 돌려주면 "그런 문서가 없다" 와 "내용이 없다" 를 구분할 수 없다.
    let Some(document) = project.docs.get(&path) else {
        eprintln!("그런 문서가 없습니다: {주소}");
        eprintln!("kang list 로 문서 경로를 확인하세요.");
        return 2;
    };

    let target = match 토픽 {
        Some(이름) => {
            if !document.topics.iter().any(|topic| topic.name == 이름) {
                eprintln!("그 문서에 그런 topic 이 없습니다: {주소}");
                eprintln!("문서 전체를 조회하면 topic 목록이 함께 나옵니다.");
                return 2;
            }
            ShowTarget::Topic(path, 이름.to_string())
        }
        None => ShowTarget::Document(path),
    };

    // 한 번에 쓴다. 뷰는 하나의 YAML 문서이므로 중간에 끊기면 파싱되지 않는다.
    let mut out = std::io::stdout().lock();
    if let Some(코드) = 찍기(&mut out, &show::show(&project, &table, &target)) {
        return 코드;
    }
    0
}

/// 표준 출력에 한 줄을 쓴다.
///
/// `println!` 은 쓰기에 실패하면 **패닉**한다. `kang list | head -20` 은 에이전트의
/// 관용구인데, 파이프를 닫은 쪽이 먼저 끝나면 그 패닉이 종료 코드 101 과 함께 Rust
/// 런타임 트레이스를 남긴다 — 문서화된 종료 코드 4종 밖이고, 코드도 `fix` 도 없는
/// 글이 `error[Kxxx]` 진단 채널에 섞인다.
///
/// # 매개변수
/// - `out`: 잠근 표준 출력
/// - `줄`: 쓸 한 줄
///
/// # 반환값
/// 계속 써도 되면 `None`, 멈춰야 하면 그때의 종료 코드
fn 찍기(out: &mut std::io::StdoutLock<'_>, 줄: &str) -> Option<i32> {
    match writeln!(out, "{줄}") {
        Ok(()) => None,
        // 파이프를 닫은 쪽은 원하는 만큼 읽었다. 그것이 곧 성공이다.
        Err(오류) if 오류.kind() == std::io::ErrorKind::BrokenPipe => Some(0),
        // 그 밖의 실패는 출력이 잘린 것이다. 0 을 주면 잘린 목록을 전부라고 말하게 된다.
        //
        // **2 다. 1 이 아니다.** 계약상 1 은 "컴파일 error 존재" 이고 그때는 stderr 에
        // `error[Kxxx]` 진단이 함께 온다. 디스크가 찼거나 리다이렉트가 깨진 것은 진단이
        // 하나도 없는 1 이 되어, 종료 코드로 분기하는 에이전트가 "문서를 고쳐라" 로 읽는다.
        // 스펙 6절의 기준("원인이 문서 밖에 있는가")대로 환경 오류다.
        Err(오류) => {
            eprintln!("표준 출력에 쓰지 못했습니다 — {오류}");
            Some(2)
        }
    }
}

/// 진단 전량을 표준 오류에 쓴다.
///
/// `eprint!` 는 쓰기에 실패하면 **패닉**한다 — [`찍기`] 가 표준 출력에서 막은 것과 같은
/// 사고다. `kang build 2>&1 | head` 는 `set -euo pipefail` 을 쓰는 CI 의 관용구이고,
/// 진단은 실 규모에서 수십만 바이트이므로 파이프 버퍼(64KB)를 넘긴 뒤 EPIPE 가 난다.
/// 그때 패닉하면 스펙 6절의 종료 코드가 101 로 뒤집힌다 — warn 만 있는 프로젝트의 0 도
/// 함께 뒤집힌다.
///
/// **진단을 내는 자리는 둘뿐이다** ([`parse_project`] 의 루트 탐색 실패와 [`진단_마감`]).
/// 그 밖의 `eprint!` 는 한 줄짜리 안내라 파이프 버퍼에 든다.
///
/// # 매개변수
/// - `진단들`: 찍을 진단 전부
fn 진단_찍기(진단들: &[Diagnostic]) {
    let mut err = std::io::stderr().lock();
    let Err(오류) = err.write_all(check::report(진단들).as_bytes()) else {
        return;
    };
    // 파이프를 닫은 쪽은 원하는 만큼 읽었다. 그것이 곧 성공이므로 종료 코드는 진단이
    // 정한 그대로 둔다 — 여기서 바꾸면 EPIPE 가 판정을 뒤집는다.
    if 오류.kind() == std::io::ErrorKind::BrokenPipe {
        return;
    }
    // 그 밖의 실패는 진단이 잘린 것이다. 그때 0 이나 2 를 주면 보지 못한 진단을 없다고
    // 배운다. 안내도 같은 스트림이라 실패할 수 있으므로 결과를 버린다.
    let _ = writeln!(err, "표준 오류에 쓰지 못했습니다 — {오류}");
    std::process::exit(1);
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
