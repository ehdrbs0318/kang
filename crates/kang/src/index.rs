//! 심볼 인덱스 산출 — `kang index`.
//!
//! proc-macro 와 TypeScript 타입 생성기가 읽을 자료다. 컴파일러가 아는 심볼 전부를
//! 주소·종류·rev 로 한 줄씩 낸다.
//!
//! **소비자가 의존성 없이 파싱할 수 있어야 한다.** `kang-macros` 의 허용 의존성
//! (V0001 §10.1)은 `syn`·`quote`·`proc-macro2` 뿐이고 인덱스 파서는 그 목록에 없다 —
//! 손으로 쓴다. 그래서 형식이 탭 구분 텍스트다.
//!
//! **필드 순서가 `종류`·`rev`·`주소` 인 이유:**
//! 심볼 이름에 탭이 들어가는 것은 오늘 합법이다(`show` 가 인용해 내고 `refs` 가 받는다).
//! 주소를 앞에 두면 그 한 줄이 필드 넷이 되어 소비자가 조용히 오독한다. 가변 길이 필드를
//! 마지막에 두면 `splitn(3, '\t')` 가 이름 안의 탭을 그대로 살린다 — 파서는 여전히
//! 세 줄이고 **새 금지를 하나도 만들지 않는다.** 이름에 줄바꿈은 들어갈 수 없다
//! (선언이 한 줄에서 나온다).
//!
//! **핀은 [`crate::hash::rev`] 하나가 계산한다.** [`crate::check::check_revs`] 가 비교하는
//! 값과 같은 함수·같은 입력([`SymbolTable::hash_source`])에서 나와야 한다. 갈리면 매크로가
//! 인덱스의 핀으로 거짓을 검증한다.
//!
//! **주소는 [`crate::check::심볼_주소`] 가 조립한다.** 네 번째 사본을 만들면 인덱스가 낸
//! 주소를 `refs`·`show`·`bless` 가 못 받는 경우가 생긴다.

use crate::ast::{DocPath, SymbolKind, SymbolRef};
use crate::check::심볼_주소;
use crate::hash;
use crate::resolve::{Project, SymbolTable};
use std::io::{self, Write};

/// 심볼 인덱스를 한 줄 하나씩 쓴다.
///
/// 형식은 `{종류}\t{rev}\t{주소}\n` 이며 종류는 `keyword`·`topic`·`exception` 이다.
/// 줄 순서는 문서 경로, 그 안에서 선언 순서다 — 같은 프로젝트에서 두 번 돌리면 같은
/// 바이트가 나와야 한다. 생성물을 커밋하는 결정(V0004 Task 3 J2)이 그것에 의존한다.
///
/// # 매개변수
/// - `project`: 로드된 프로젝트
/// - `table`: 심볼 테이블. 핀 계산의 입력을 여기서 얻는다
/// - `out`: 인덱스를 받을 곳
///
/// # 오류
/// 쓰기가 실패하면 그 사정을 그대로 올린다. 호출자가 부분 결과를 버릴 책임을 진다.
pub fn write_index(project: &Project, table: &SymbolTable, out: &mut impl Write) -> io::Result<()> {
    // 문서 경로로 정렬해 출력이 실행마다 같게 만든다. `Project::docs` 는 HashMap 이라
    // 순회 순서가 실행마다 다르다 — 정렬하지 않으면 커밋한 인덱스가 매번 diff 를 낸다.
    let mut 경로들: Vec<_> = project.docs.keys().collect();
    경로들.sort_by_key(|p| p.0.clone());

    // 문서마다 그 문서가 **선언한** 심볼만 낸다. import 한 것은 owner 의 줄에 이미 있다.
    for 경로 in 경로들 {
        let 문서 = &project.docs[경로];

        // keyword 는 계층 조각을 `.` 로 이은 전체 이름이 주소가 된다 (스펙 4.1).
        for k in &문서.keywords {
            let 이름 = k.name.0.join(".");
            한_줄(out, table, 경로, 심볼종류::Keyword, &이름)?;
        }

        // topic 과 그 topic 이 선언한 exception 을 함께 낸다. exception 의 해시 입력은
        // 선언 topic 의 본문이므로(스펙 4.8) 두 줄의 rev 가 같은 것이 정상이다.
        for t in &문서.topics {
            한_줄(out, table, 경로, 심볼종류::Topic, &t.name)?;
            for e in &t.exceptions {
                한_줄(out, table, 경로, 심볼종류::Exception, &e.name)?;
            }
        }
    }
    Ok(())
}

/// 심볼 하나를 인덱스 한 줄로 쓴다.
///
/// 테이블에서 찾지 못하면 아무것도 쓰지 않는다. `compile()` 을 통과한 프로젝트에서는
/// 일어나지 않지만, 조용히 잘못된 rev 를 내는 것보다 그 줄을 빼는 것이 낫다 —
/// 소비자에게 없는 심볼은 "매크로가 가리키는 심볼이 없다" 는 참인 진단이 된다.
///
/// # 매개변수
/// - `out`: 인덱스를 받을 곳
/// - `table`: 심볼 테이블
/// - `doc`: 심볼을 선언한 문서
/// - `종류`: 심볼의 종류. 주소의 구분자와 인덱스의 종류 칸을 함께 정한다
/// - `name`: 전체 이름. keyword 계층은 `.` 로 이어진 형태다
///
/// # 오류
/// 쓰기가 실패하면 그 사정을 그대로 올린다.
fn 한_줄(
    out: &mut impl Write,
    table: &SymbolTable,
    doc: &DocPath,
    종류: 심볼종류,
    name: &str,
) -> io::Result<()> {
    let 주소 = 심볼_주소(doc, &종류.kind(), name, false);
    let 참조 = SymbolRef {
        doc: doc.clone(),
        kind: 종류.kind(),
        name: name.split('.').map(|s| s.to_string()).collect(),
    };

    // 테이블에 없으면 그 줄을 빼고 넘어간다 (위 rustdoc 의 근거).
    let Some(id) = table.resolve(&참조) else {
        return Ok(());
    };

    // 핀 계산은 `hash::rev` 하나가 담당한다. `check_revs` 가 비교하는 값과 같은
    // 함수·같은 입력에서 나와야 한다 — 갈리면 매크로가 거짓을 검증한다.
    let rev = hash::rev(table.hash_source(id));
    writeln!(out, "{}\t{rev}\t{주소}", 종류.이름())
}

/// 인덱스가 다루는 세 종류.
///
/// [`SymbolKind`] 를 그대로 쓸 수 없다 — `Clone` 을 파생하지 않아 주소 조립과
/// [`SymbolRef`] 구성에 두 번 쓸 수 없고, `ast.rs` 는 굳은 파일이다. 종류 이름 문자열도
/// 여기 함께 두어 인덱스의 종류 칸과 주소의 구분자가 갈리지 않게 한다.
#[derive(Copy, Clone)]
enum 심볼종류 {
    Keyword,
    Topic,
    Exception,
}

impl 심볼종류 {
    /// 대응하는 [`SymbolKind`] 를 새로 만든다.
    ///
    /// # 반환값
    /// 같은 종류를 가리키는 [`SymbolKind`]
    fn kind(self) -> SymbolKind {
        match self {
            심볼종류::Keyword => SymbolKind::Keyword,
            심볼종류::Topic => SymbolKind::Topic,
            심볼종류::Exception => SymbolKind::Exception,
        }
    }

    /// 인덱스의 종류 칸에 쓰는 이름.
    ///
    /// # 반환값
    /// `keyword`·`topic`·`exception` 중 하나
    fn 이름(self) -> &'static str {
        match self {
            심볼종류::Keyword => "keyword",
            심볼종류::Topic => "topic",
            심볼종류::Exception => "exception",
        }
    }
}
