//! `kang bless` — rev 핀을 갱신하거나 삽입하는 유일한 명령 (스펙 4.8·6.2).
//!
//! **kang 에서 유일하게 사용자 파일을 고쳐 쓰는 층이다.** 다른 모든 명령은 읽기만 한다.
//! 그래서 두 가지를 신뢰 경계로 지킨다.
//!
//! - **원자성** — 같은 디렉토리에 임시 파일로 쓰고 [`std::fs::rename`] 한다. 쓰는 도중
//!   죽어도 사용자 문서가 반쯤 잘린 채 남지 않는다. `/tmp` 에 쓰고 옮기면 파일시스템이
//!   달라 원자적이지 않으므로 그렇게 하지 않는다.
//! - **원문 보존** — 문서를 파싱해 다시 직렬화하지 않고, 그 import 줄의 그 자리만
//!   치환한다. 줄 끝(`\r\n`)·파일 끝 개행 유무·들여쓰기·줄 끝 공백이 전부 그대로 남는다.
//!
//! **핀은 [`crate::hash::rev`] 하나가 계산한다.** [`crate::check::check_revs`] 가 비교하는
//! 값과 같은 함수·같은 입력([`SymbolTable::hash_source`])에서 나와야 한다. 두 번째 사본을
//! 만들면 `bless` 가 넣은 핀을 `build` 가 곧바로 `K021` 로 거부한다.
//!
//! **error 상태에서 실행되는 것이 정상이다.** 핀 없음(`K020`)도 핀 불일치(`K021`)도
//! error 이므로, `compile()` 통과를 요구하면 이 명령은 영원히 실행될 수 없다. 경계는
//! `parse_project` 다 — "문서를 읽지 못했다"(`K050`·`K051`·`K052`·`K1xx`)는 막고,
//! "문서가 규칙을 어겼다"(`K001` 이하)는 막지 않는다. 읽지 못한 문서를 고쳐 쓰면 사용자는
//! 깨진 프로젝트에 편집까지 얹은 상태를 받고, 규칙 위반에서 멈추면 개념 이름을 바꾸는
//! 정상 워크플로(스펙 6.1)가 통째로 막힌다.

use crate::ast::{DocPath, SymbolKind, SymbolRef};
use crate::hash;
use crate::parse::parse_document;
use crate::resolve::{Project, SymbolTable};
use std::path::Path;

/// 갱신 대상 import 를 가리키는 주소. `docs/A.결제` 를 파싱한 결과다.
/// 줄 번호를 쓰지 않는다 (ADR-0003).
pub struct ImportAddress {
    /// 가리키는 심볼.
    pub target: SymbolRef,
}

impl ImportAddress {
    /// `docs/A.결제`·`docs/A#결제의 방법`·`docs/A!무료 상품` 을 파싱한다.
    /// Task 8 의 진단 fix 가 이 형식으로 출력한다. 백틱은 쓰지 않는다 (스펙 6.0).
    ///
    /// 마지막 `/` 뒤에서 keyword `.` · topic `#` · exception `!` 중 **가장 앞선 것**을
    /// 구분자로 삼는다. 문서 문법의 [`crate::parse`] 가 백틱 밖에서 하는 판정과 같은
    /// 규칙이며, 디렉토리 이름의 `.` 을 먼저 가르는 `kang refs` 의 실수도 함께 피한다.
    ///
    /// ponytail: **문서 파일 이름 자체**에 `.`·`#`·`!` 이 있으면 그 문자에서 갈려 틀린다
    /// (`docs/a.b#정책` 은 keyword `docs/a`.`b#정책` 으로 읽힌다). `kang refs`·`kang show`
    /// 가 선 자리와 같은 스펙 6.0 주소 문법의 천장이고, `K020`·`K021` 의 `[shell] fix` 도
    /// 같은 문자열을 만들므로 셋이 함께 어긋난다. `project.docs` 를 뒤져 최장 일치로
    /// 고치는 것은 "구분자만으로 파싱한다" 는 스펙 지시를 어기고 경로 오타의 참인 진단을
    /// 망친다. 문법이 문서 이름의 구분자를 다루는 방법을 정하면 세 명령을 함께 올린다.
    ///
    /// # 매개변수
    /// - `s`: 백틱 없는 심볼 주소
    ///
    /// # 반환값
    /// 파싱한 [`ImportAddress`]
    ///
    /// # 오류
    /// 구분자가 없거나 문서 경로·심볼 이름이 비어 있으면 그 사정을 담은 문장
    pub fn parse(s: &str) -> Result<ImportAddress, String> {
        // 마지막 `/` 뒤가 문서 이름과 심볼 이름이고, 그 앞은 전부 디렉토리다.
        let (디렉토리, 마지막) = s.rsplit_once('/').unwrap_or(("", s));

        let Some((at, 구분자, kind)) = [
            ('.', SymbolKind::Keyword),
            ('#', SymbolKind::Topic),
            ('!', SymbolKind::Exception),
        ]
        .into_iter()
        .filter_map(|(구분자, kind)| 마지막.find(구분자).map(|at| (at, 구분자, kind)))
        .min_by_key(|&(at, _, _)| at) else {
            return Err(format!(
                "심볼 주소가 아닙니다: {s}. 문서 경로와 심볼을 keyword 는 . , topic 은 # , exception 은 ! 로 이어 적으세요. 예: kang bless docs/B --import 'docs/A.결제'"
            ));
        };

        let 문서명 = &마지막[..at];
        let 이름 = &마지막[at + 구분자.len_utf8()..];
        // 문서 이름이나 심볼 이름이 비면 어느 심볼도 가리키지 못한다. 그대로 두면
        // "그런 심볼이 없습니다" 라는 엉뚱한 원인을 말하게 된다.
        if 문서명.is_empty() {
            return Err(format!("심볼 주소에 문서 이름이 없습니다: {s}"));
        }
        if 이름.is_empty() {
            return Err(format!("심볼 주소에 심볼 이름이 없습니다: {s}"));
        }

        // 빈 조각은 버린다 — `docs//A.결제` 나 `/docs/A.결제` 가 없는 경로 조각을 만들지
        // 않게 한다.
        let mut 조각: Vec<String> = 디렉토리
            .split('/')
            .filter(|조각| !조각.is_empty())
            .map(str::to_string)
            .collect();
        조각.push(문서명.to_string());

        Ok(ImportAddress {
            target: SymbolRef {
                doc: DocPath(조각),
                // 계층을 갖는 것은 keyword 뿐이다 (스펙 4.3). topic·exception 이름의
                // `.` 은 이름의 일부다.
                name: match kind {
                    SymbolKind::Keyword => 이름.split('.').map(str::to_string).collect(),
                    _ => vec![이름.to_string()],
                },
                kind,
            },
        })
    }
}

/// `doc` 안에서 `addr` 이 가리키는 import 의 rev 핀을 현재 해시로 맞춘다.
/// 핀이 있으면 갱신하고 없으면 삽입한다 (스펙 4.8).
///
/// **주소는 심볼이다** (ADR-0003). 그리고 **편집할 좌표는 지금 쓰려는 바이트에서만
/// 나온다** — 읽어 들인 원문을 다시 파싱해 그 결과의 줄 번호로 고친다.
///
/// `project` 의 줄 번호를 쓰면 안 된다. 그것은 **앞선 읽기**에서 나온 좌표이고, 그
/// 사이에 파일이 바뀌면 낡는다. 낡은 좌표가 우연히 다른 `import ` 접두 줄(코드펜스 안의
/// 사용자 산문 등)을 가리키면 거기에 핀이 박히고, 그 줄은 topic 본문이므로 **그 topic 의
/// 해시가 바뀌어** 그것을 import 한 하위 문서까지 무너진다. 그러고도 bless 는 성공을
/// 알린다. 재파싱은 그 창을 닫는다 — 편집할 줄이 정의상 방금 읽은 바이트에서 나온다.
///
/// 재파싱이 실패한다는 것은 곧 **파일이 바뀌었다**는 뜻이다. 같은 바이트가 앞서
/// `parse_project` 를 통과했기 때문이다.
///
/// 같은 심볼을 여러 줄이 import 하는 것은 `K004` 가 잡는 error 지만 파싱은 통과하므로
/// 여기까지 올 수 있다. 그때는 **맞는 줄을 전부** 갱신한다 — 하나만 고치면 나머지 줄의
/// `K020` 을 해소할 방법이 없어져 빌드가 영원히 깨진 채로 남는다.
///
/// # 매개변수
/// - `project`: 파싱을 마친 프로젝트. 대상 문서가 이 프로젝트의 것인지만 본다
/// - `table`: 전역 심볼 테이블. 해시 입력을 여기서 얻는다
/// - `root`: 프로젝트 루트. [`DocPath`] 는 루트 기준이다
/// - `doc`: 고쳐 쓸 문서
/// - `addr`: 갱신할 import 의 주소
///
/// # 반환값
/// 파일을 실제로 고쳤으면 `true`, 이미 맞는 핀이라 쓰지 않았으면 `false`.
/// **부르는 쪽이 "갱신했다" 와 "이미 최신이다" 를 구분해 말해야 한다** — 아무 바이트도
/// 바뀌지 않았는데 갱신했다고 하면 검증하면 거짓이다.
///
/// # 오류
/// 문서가 없거나, 그 문서에 그 import 가 없거나, 대상 심볼이 없거나,
/// 파일을 읽고 쓰지 못하면 그 사정을 담은 문장
pub fn bless(
    project: &Project,
    table: &SymbolTable,
    root: &Path,
    doc: &DocPath,
    addr: &ImportAddress,
) -> Result<bool, String> {
    if !project.docs.contains_key(doc) {
        return Err("그런 문서가 없습니다. kang list 로 문서 경로를 확인하세요.".to_string());
    }

    // 대상이 없으면 해시할 것이 없다. 핀을 지어내면 `K021` 이 영원히 남는다.
    let Some(대상) = table.resolve(&addr.target) else {
        return Err(
            "import 대상 심볼이 없습니다. 대상 문서와 이름을 확인하세요 — kang list 로 문서를, kang keywords 로 키워드를 볼 수 있습니다.".to_string()
        );
    };
    // 핀 계산은 `hash::rev` 하나가 담당한다. `check_revs` 가 비교하는 값과 같은
    // 함수·같은 입력이어야 방금 넣은 핀을 build 가 거부하지 않는다.
    let 새_핀 = hash::rev(table.hash_source(대상));

    // [`DocPath`] 의 Display 는 `/` 로 잇는다. 조각을 하나씩 join 하지 않는 이유는
    // 마지막 조각에 `.` 이 든 문서(`docs/a.b`)에서 확장자 API 가 이름을 잘라내서다.
    let 파일 = root.join(format!("{doc}.kang"));
    let 원문 = std::fs::read_to_string(&파일)
        .map_err(|오류| format!("문서 파일을 읽지 못했습니다 ({}) — {오류}", 파일.display()))?;

    // 고칠 줄을 **방금 읽은 바이트**에서 다시 찾는다. BOM 은 디코딩 아티팩트이므로
    // 로더와 같은 방법으로 벗긴다 — 줄 1 안에 있으므로 줄 번호는 달라지지 않는다.
    let Ok(현재) = parse_document(doc.clone(), 원문.strip_prefix('\u{feff}').unwrap_or(&원문))
    else {
        return Err(
            "문서 파일을 다시 읽었더니 파싱되지 않습니다. kang 이 도는 중에 파일이 바뀐 것으로 보이니 kang build 로 다시 확인하세요.".to_string()
        );
    };

    // 줄 단위로 다시 조립한다. 오프셋을 직접 다루지 않으므로 한 파일에서 여러 자리를
    // 고쳐도 뒤의 자리가 밀리지 않는다. `split_inclusive` 는 줄 끝 문자를 조각에
    // 남기므로 `\r\n` 과 파일 끝 개행 유무가 그대로 보존된다.
    let mut 줄들: Vec<String> = 원문.split_inclusive('\n').map(str::to_string).collect();
    let mut 고친_줄 = 0;
    // 이 문서의 import 를 훑어 주소가 가리키는 것을 전부 찾는다.
    for import in &현재.imports {
        // 이름이 아니라 심볼로 맞춘다. `` `결제`.`상태` `` 와 `결제.상태` 는 같은 심볼이다.
        if table.resolve(&import.target) != Some(대상) {
            continue;
        }
        // 줄 번호는 1-based 이고 이 원문과 같은 바이트에서 나왔으므로 범위 안이다.
        // 그래도 인덱싱으로 패닉하지 않는다 — 파일을 쓰는 층이다.
        let 자리 = import.line - 1;
        let Some(줄) = 줄들.get(자리) else {
            return Err(format!(
                "{}번째 줄을 파일에서 찾지 못했습니다. kang build 로 다시 확인하세요.",
                import.line
            ));
        };
        줄들[자리] = 핀_박기(줄, import.rev.as_deref(), &새_핀)?;
        고친_줄 += 1;
    }

    // 없는 import 를 조용히 성공으로 돌려주면 에이전트는 핀을 붙였다고 믿고 같은
    // error 를 다시 만난다.
    if 고친_줄 == 0 {
        return Err(
            "그 문서에 이 import 가 없습니다. 문서 최상단의 import 줄과 주소가 같은 심볼을 가리켜야 합니다.".to_string()
        );
    }

    let 새_원문 = 줄들.concat();
    // 이미 맞는 핀이면 쓰지 않는다. 두 번째 bless 는 파일을 바꾸지 않는다.
    if 새_원문 == 원문 {
        return Ok(false);
    }

    쓰기_원자적으로(&파일, &새_원문)?;
    Ok(true)
}

/// import 줄 하나의 rev 핀 자리만 갈아 끼운다.
///
/// 핀이 있으면 **따옴표로 감싼 값만** 치환한다. `rev` 와 값 사이의 공백까지 다시 쓰면
/// 원문이 바뀐다. 핀이 없으면 마지막 비공백 문자 뒤에 삽입한다 — 그 자리가 아니면
/// `\r` 앞뒤가 뒤집혀 `import ...\r rev "..."` 같은 깨진 줄이 나온다.
///
/// # 매개변수
/// - `줄`: 줄 끝 문자를 포함한 원문 한 줄
/// - `옛_핀`: 파서가 읽어낸 기존 핀 값. 없으면 `None`
/// - `새_핀`: 넣을 핀 값
///
/// # 반환값
/// 핀만 바뀐 같은 줄
///
/// # 오류
/// 파서가 읽은 기존 핀이 줄 끝에서 발견되지 않으면 (파서와 이 함수가 어긋난 경우)
/// 그 사정을 담은 문장
fn 핀_박기(줄: &str, 옛_핀: Option<&str>, 새_핀: &str) -> Result<String, String> {
    // 줄 끝 공백과 줄 끝 문자를 잘라 두었다가 그대로 다시 붙인다.
    let 몸통 = 줄.trim_end();
    let 꼬리 = &줄[몸통.len()..];

    match 옛_핀 {
        // 기존 핀은 줄의 마지막 토큰이다 (파서가 그 뒤에 아무것도 오지 못하게 한다).
        Some(옛) => {
            let Some(앞) = 몸통.strip_suffix(&format!("\"{옛}\"")) else {
                return Err(format!(
                    "이 줄의 끝에서 기존 rev 핀 {옛} 을 찾지 못했습니다. 파일이 kang 실행 중에 바뀌었을 수 있습니다."
                ));
            };
            Ok(format!("{앞}\"{새_핀}\"{꼬리}"))
        }
        // 핀이 없으면 줄 끝에 덧붙인다 (스펙 4.7 의 `rev "<해시>"` 는 줄 끝 토큰이다).
        None => Ok(format!("{몸통} rev \"{새_핀}\"{꼬리}")),
    }
}

/// 파일을 원자적으로 바꿔 쓴다.
///
/// 같은 디렉토리에 임시 파일로 쓰고 [`std::fs::rename`] 한다. 같은 파일시스템 안의
/// rename 은 원자적이므로, 어느 시점에 죽어도 문서는 **옛 내용이거나 새 내용**이지
/// 반쯤 잘린 내용이 되지 않는다.
///
/// 임시 파일 확장자는 `.bless` 다. `.kang` 으로 끝나면 동시에 도는 다른 kang 프로세스가
/// 그것을 문서로 읽는다.
///
/// **rename 은 임시 파일의 inode 를 문서 자리에 올린다.** 그래서 파일 mode 를 옮겨
/// 심는다 — 아무것도 하지 않으면 사용자가 정한 권한이 `0666 & !umask` 로 갈린다.
///
/// ponytail: `fsync` 를 부르지 않으므로 전원이 나가면 rename 이 디스크에 닿기 전일 수
/// 있다. 막는 것은 "쓰다 만 파일" 이지 "전원 장애" 다. 파일 하나와 디렉토리를 함께
/// 동기화하는 비용이 정당해지면 그때 올린다.
///
/// ponytail: mode 만 옮기고 xattr·ACL·하드링크는 옮기지 않는다. 새 inode 를 올리는
/// 방식의 대가이며, 이것들이 문제가 되면 임시 파일 없이 제자리에 쓰는 방식으로 원자성을
/// 포기하는 대신 파일 잠금을 들여야 한다. 문서 파일에 xattr 를 다는 사례가 나오면 올린다.
///
/// ponytail: 임시 파일 이름에 프로세스 id 를 넣지 않는다. `K020` 은 import 하나마다
/// 별도 fix 를 내므로(`check.rs`) 같은 문서를 겨냥한 명령 두 줄이 동시에 돌 수 있다.
/// 이름이 같으면 겹친 한쪽이 `exit 2` 로 **시끄럽게** 실패하고, 그 실패는 다음
/// `kang build` 의 `K020` 이 같은 fix 로 다시 알려 스펙 4.8 3단계가 자가 복구한다.
/// id 를 붙이면 두 rename 이 모두 성공해 "나중 쪽이 이기고 앞선 쪽은 성공했다고 착각"
/// 하는 **조용한 유실**로 바뀐다. 잠금이 필요해지면 그때 올린다.
///
/// # 매개변수
/// - `파일`: 바꿔 쓸 파일
/// - `내용`: 새 내용 전체
///
/// # 오류
/// 임시 파일을 쓰지 못하거나 옮기지 못하면 그 사정을 담은 문장.
/// **실패한 경로를 지목한다** — 멀쩡한 파일을 지목하면 사용자가 엉뚱한 곳을 뒤진다
fn 쓰기_원자적으로(파일: &Path, 내용: &str) -> Result<(), String> {
    let 임시 = 파일.with_extension("kang.bless");

    // 실패한 자리에 임시 파일이 남으면 사용자 저장소에 쓰레기가 쌓인다.
    if let Err(오류) = std::fs::write(&임시, 내용) {
        let _ = std::fs::remove_file(&임시);
        return Err(format!(
            "임시 파일에 쓰지 못했습니다 ({}) — {오류}",
            임시.display()
        ));
    }

    // mode 를 못 옮기는 것은 편집을 포기할 이유가 아니다. 핀을 넣는 것이 주 목적이고
    // 권한은 사용자가 다시 줄 수 있다.
    if let Ok(메타) = std::fs::metadata(파일) {
        let _ = std::fs::set_permissions(&임시, 메타.permissions());
    }

    if let Err(오류) = std::fs::rename(&임시, 파일) {
        let _ = std::fs::remove_file(&임시);
        return Err(format!(
            "임시 파일을 문서 자리로 옮기지 못했습니다 ({} → {}) — {오류}",
            임시.display(),
            파일.display()
        ));
    }

    Ok(())
}
