//! 심볼 산출 — `kang index` 와 `kang types`.
//!
//! proc-macro 가 읽을 탭 구분 인덱스와 `tsc` 가 읽을 TypeScript 타입을 낸다. **둘은 같은
//! [`순회`] 를 공유한다** — 순회를 복제하면 Rust 쪽과 TS 쪽이 서로 다른 심볼 집합이나
//! 다른 핀을 보게 되고, 한쪽만 통과하는 상태가 조용히 생긴다.
//!
//! 인덱스는 컴파일러가 아는 심볼 전부를 주소·종류·rev 로 한 줄씩 낸다.
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
    순회(project, table, |종류, rev, 주소| {
        writeln!(out, "{}\t{rev}\t{주소}", 종류.이름())
    })
}

/// 주어진 내용이 [`write_index`] 가 낸 것으로 보이는지 본다.
///
/// **덮어쓰기 방어의 절반이다.** `kang index <경로>` 는 경로를 그대로 받아 덮어쓰므로
/// `kang index Cargo.toml` 한 번이 그 파일을 지운다. 산출물은 언제든 다시 만들 수 있지만
/// 남의 파일은 그렇지 않고, 되돌릴 방법이 없다. 그래서 이미 있는 파일이 **내가 낸 것으로
/// 보일 때만** 덮어쓴다.
///
/// 모든 줄이 탭 셋으로 갈린 세 필드이고 가운데가 6자리 16진수면 이 형식이다. 빈 파일도
/// 통과시킨다 — 심볼이 없는 프로젝트의 정당한 산출물이다.
///
/// # 매개변수
/// - `내용`: 이미 그 자리에 있는 파일의 내용
///
/// # 반환값
/// 이 형식으로 읽히면 `true`
pub fn 인덱스_형식인가(내용: &str) -> bool {
    내용.lines().all(|줄| {
        let mut 필드 = 줄.splitn(3, '\t');
        // 종류·rev·주소 셋이 다 있어야 한다. 하나라도 없으면 다른 형식이다.
        let (Some(종류), Some(rev), Some(주소)) = (필드.next(), 필드.next(), 필드.next())
        else {
            return false;
        };
        // rev 는 [`crate::hash::rev`] 가 내는 6자리 소문자 16진수다.
        matches!(종류, "keyword" | "topic" | "exception")
            && rev.len() == 6
            && rev
                .chars()
                .all(|글자| matches!(글자, '0'..='9' | 'a'..='f'))
            && !주소.is_empty()
    })
}

/// 주어진 내용이 [`write_types`] 가 낸 것으로 보이는지 본다.
///
/// [`인덱스_형식인가`] 와 같은 이유로 있다. 이쪽은 첫 줄에 머리글이 있으므로 그것을 본다 —
/// 그 머리글이 "손으로 고치지 않습니다" 라고 적은 이상, 그것이 없는 파일은 손으로 쓴
/// 파일이고 덮어쓰면 안 된다.
///
/// # 매개변수
/// - `내용`: 이미 그 자리에 있는 파일의 내용
///
/// # 반환값
/// 이 형식으로 읽히면 `true`
pub fn 타입_형식인가(내용: &str) -> bool {
    내용.is_empty() || 내용.starts_with("// kang 이 생성한 파일입니다.")
}

/// TypeScript 타입 파일을 낸다 (V0003 §5).
///
/// **topic 만 담는다.** 데코레이터가 붙는 자리는 클래스·메소드·접근자·프로퍼티이며
/// (V0003 §3) 메소드가 구현하는 것은 정책이다. keyword 는 용어 정의이고 exception 은
/// 정책의 구멍이므로 코드가 "구현" 하는 대상이 아니다 — 그 둘을 가리키는 코드는 V0003 §3
/// 의 주석 폴백과 `kang inspect --ci` 가 담당하고, [`write_index`] 가 이미 세 종류를
/// 전부 낸다. V0003 §5 가 적은 선언도 `KangTopics` 하나다.
///
/// **핀은 [`write_index`] 와 같은 순회에서 나온다.** 갈리면 Rust 매크로와 `tsc` 가 서로
/// 다른 핀을 보고, 한쪽만 통과하는 상태가 조용히 생긴다.
///
/// # 매개변수
/// - `project`: 로드된 프로젝트
/// - `table`: 심볼 테이블
/// - `out`: 타입 파일을 받을 곳
///
/// # 오류
/// 쓰기가 실패하면 그 사정을 그대로 올린다.
pub fn write_types(project: &Project, table: &SymbolTable, out: &mut impl Write) -> io::Result<()> {
    writeln!(
        out,
        "// kang 이 생성한 파일입니다. 손으로 고치지 않습니다 — 다음 `kang types` 가 덮어씁니다.\n\
         // 없는 topic 은 keyof 제약 위반, 낡은 rev 는 리터럴 불일치로 tsc 가 잡습니다.\n\n\
         export interface KangTopics {{"
    )?;

    순회(project, table, |종류, rev, 주소| match 종류 {
        // topic 만 낸다 (위 rustdoc 의 근거).
        심볼종류::Topic => writeln!(out, "  \"{}\": \"{rev}\";", ts_리터럴(주소)),
        심볼종류::Keyword | 심볼종류::Exception => Ok(()),
    })?;

    // **본문이 있는 함수로 낸다.** `declare` 로 내면 컴파일된 JS 에 그 export 가 없어
    // `import { kangTopic }` 이 `undefined` 를 받고, 첫 데코레이터 적용에서 터진다.
    // 존재하지 않는 것을 존재한다고 선언하는 파일을 kang 이 만들면 안 된다.
    writeln!(
        out,
        "}}\n\n\
         export function kangTopic<K extends keyof KangTopics>(\n\
         \x20 topic: K,\n\
         \x20 rev: KangTopics[K],\n\
         ): MethodDecorator {{\n\
         \x20 return () => {{}};\n\
         }}"
    )
}

/// 프로젝트의 모든 심볼을 종류·핀·주소로 한 번 훑는다.
///
/// **[`write_index`] 와 [`write_types`] 가 이 하나를 공유한다.** 순회를 복제하면 인덱스와
/// 타입이 서로 다른 심볼 집합이나 다른 핀을 보게 되고, Rust 쪽과 TS 쪽 중 한쪽만 통과하는
/// 상태가 조용히 생긴다.
///
/// 줄 순서는 문서 경로, 그 안에서 선언 순서다 — 같은 프로젝트에서 두 번 돌리면 같은
/// 바이트가 나와야 한다. 생성물을 커밋하는 결정(V0004 Task 3 J2)이 그것에 의존한다.
///
/// 테이블에서 찾지 못한 심볼은 건너뛴다. `compile()` 을 통과한 프로젝트에서는 일어나지
/// 않지만, 조용히 잘못된 rev 를 내는 것보다 그 줄을 빼는 것이 낫다 — 소비자에게 없는
/// 심볼은 "가리키는 심볼이 없다" 는 참인 진단이 된다.
///
/// # 매개변수
/// - `project`: 로드된 프로젝트
/// - `table`: 심볼 테이블. 핀 계산의 입력을 여기서 얻는다
/// - `낼것`: 심볼 하나마다 `(종류, 핀, 주소)` 로 불린다
///
/// # 오류
/// `낼것` 이 낸 오류를 그대로 올린다. 호출자가 부분 결과를 버릴 책임을 진다.
fn 순회(
    project: &Project,
    table: &SymbolTable,
    mut 낼것: impl FnMut(심볼종류, &str, &str) -> io::Result<()>,
) -> io::Result<()> {
    // 문서 경로로 정렬해 출력이 실행마다 같게 만든다. `Project::docs` 는 HashMap 이라
    // 순회 순서가 실행마다 다르다 — 정렬하지 않으면 커밋한 산출물이 매번 diff 를 낸다.
    let mut 경로들: Vec<_> = project.docs.keys().collect();
    경로들.sort_by_key(|p| p.0.clone());

    // 문서마다 그 문서가 **선언한** 심볼만 낸다. import 한 것은 owner 의 줄에 이미 있다.
    for 경로 in 경로들 {
        let 문서 = &project.docs[경로];

        // keyword 는 계층 조각을 `.` 로 이은 전체 이름이 주소가 된다 (스펙 4.1).
        for k in &문서.keywords {
            한_줄(
                &mut 낼것,
                table,
                경로,
                심볼종류::Keyword,
                &k.name.0.join("."),
            )?;
        }

        // topic 과 그 topic 이 선언한 exception 을 함께 낸다. exception 의 해시 입력은
        // 선언 topic 의 본문이므로(스펙 4.8) 두 줄의 rev 가 같은 것이 정상이다.
        for t in &문서.topics {
            한_줄(&mut 낼것, table, 경로, 심볼종류::Topic, &t.name)?;
            for e in &t.exceptions {
                한_줄(&mut 낼것, table, 경로, 심볼종류::Exception, &e.name)?;
            }
        }
    }
    Ok(())
}

/// 심볼 하나의 주소와 핀을 구해 `낼것` 에 넘긴다.
///
/// # 매개변수
/// - `낼것`: 산출을 담당하는 콜백
/// - `table`: 심볼 테이블
/// - `doc`: 심볼을 선언한 문서
/// - `종류`: 심볼의 종류. 주소의 구분자와 인덱스의 종류 칸을 함께 정한다
/// - `name`: 전체 이름. keyword 계층은 `.` 로 이어진 형태다
///
/// # 오류
/// `낼것` 이 낸 오류를 그대로 올린다.
fn 한_줄(
    낼것: &mut impl FnMut(심볼종류, &str, &str) -> io::Result<()>,
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

    // 테이블에 없으면 그 심볼을 빼고 넘어간다 (`순회` rustdoc 의 근거).
    let Some(id) = table.resolve(&참조) else {
        return Ok(());
    };

    // 핀 계산은 `hash::rev` 하나가 담당한다. `check_revs` 가 비교하는 값과 같은
    // 함수·같은 입력에서 나와야 한다 — 갈리면 매크로가 거짓을 검증한다.
    낼것(종류, &hash::rev(table.hash_source(id)), &주소)
}

/// 주소를 TypeScript 큰따옴표 문자열 리터럴의 **내용**으로 만든다.
///
/// 한글은 그대로 통과한다 — TS 문자열 리터럴은 UTF-8 이며 식별자 규칙이 걸리지 않는다.
/// 이스케이프가 필요한 것은 리터럴을 끊는 문자뿐이다.
///
/// **YAML 이미터의 [`crate::yaml::scalar`] 를 재사용하지 않는다.** 두 인용 규칙이 오늘
/// 겹치지만 YAML 의 큰따옴표 스칼라에는 JS 에 없는 이스케이프(`\a`·`\N`·`\L`)가 있어,
/// 그쪽 규칙이 하나 늘면 여기서 조용히 문법 오류가 난다.
///
/// # 매개변수
/// - `주소`: 심볼 주소
///
/// # 반환값
/// 큰따옴표 사이에 그대로 넣을 수 있는 문자열
fn ts_리터럴(주소: &str) -> String {
    let mut 결과 = String::with_capacity(주소.len());
    // 주소 한 글자씩 보며 리터럴을 끊는 것만 이스케이프한다.
    for c in 주소.chars() {
        match c {
            '\\' => 결과.push_str("\\\\"),
            '"' => 결과.push_str("\\\""),
            // 제어 문자는 이름에 합법이고(V0004 Task 3 J1) 탭이 실제로 든다. U+2028·
            // U+2029 는 옛 파서가 리터럴 안에서도 줄바꿈으로 읽는다. 둘 다 BMP 이므로
            // 네 자리 `\u` 로 적을 수 있다.
            c if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') => {
                결과.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => 결과.push(c),
        }
    }
    결과
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
