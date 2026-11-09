---
name: kang
description: 문서, 정책, 도메인 용어, 용어집, 설계 문서, ADR 를 읽거나 쓰거나 정리할 때 사용한다. 기존 마크다운 문서를 고치거나 옮길 때, 개념 이름을 바꿀 때, kang build 가 실패했을 때, .kang 파일이나 kang 명령을 다룰 때도 사용한다.
---

# kang

kang 을 쓰는 프로젝트의 문서는 `.kang` 파일이다. 한 개념은 정확히 한 파일이 정의한다.
다른 문서의 개념은 `import` 로만 가져와 쓴다. `kang build` 는 선언하지 않은 참조, 중복 정의,
참조한 원문이 바뀐 곳을 컴파일 error 로 알린다.

**이 스킬을 불렀으면 문서는 항상 `.kang` 으로 쓴다.** 프로젝트에 `.kang` 문서가 아직 없어도,
기존 문서가 마크다운이어도 같다. 마크다운으로 쓰면 import 도 rev 핀도 없다. 그러면 kang 이
막으려는 정의의 분열이 그대로 생긴다.

## 시작할 때

1. `kang list` 를 실행한다.
2. "`.kang` 문서가 없습니다" 가 나오면 `kang init` 을 실행한다. `kang init` 은 스킬 파일,
   `AGENTS.md`, `CLAUDE.md` 의 kang 안내와 `docs/example.kang` 를 만든다. 기존 파일은
   덮어쓰지 않는다. 실제 문서를 쓴 다음 `docs/example.kang` 는 지운다.
3. 문서는 `docs/` 아래에 둔다. 프로젝트에 이미 문서 폴더가 있으면 그 폴더에 둔다.
4. 끝나기 전에 `kang build` 를 실행한다. 종료 코드 0 이 나와야 끝난 것이다.

`kang build` 는 `.kang/graph.kang.json` 을 만든다. `.kang/` 폴더는 kang 이 만드는 결과물의
위치다. 지우지 말고 커밋한다.

## 기존 마크다운 문서가 있을 때

- 새로 쓰는 문서는 `.kang` 으로 쓴다.
- 새 문서가 기존 마크다운 문서의 개념을 쓰면, 그 마크다운 문서를 `.kang` 으로 옮긴다. 그다음
  새 문서에서 그 개념을 import 한다. 옮긴 마크다운 파일은 지운다. 두 형식을 함께 두면 정의가
  두 곳에 생긴다.
- "기존 문서 스타일에 맞춰" 라는 요청은 문장의 말투와 구성을 맞추라는 뜻이다. 파일 형식은
  `.kang` 이다.

| 이런 생각이 들면 | 실제로는 |
|---|---|
| "이 프로젝트의 문서는 마크다운이라 스킬 설명이 맞지 않는다" | 이 스킬을 부른 것이 kang 을 쓰라는 요청이다. `kang init` 부터 한다. |
| "`kang init` 은 요청 범위를 넘는다" | `kang init` 은 kang 으로 문서를 쓰는 첫 단계다. |
| "`.kang/` 은 내가 만든 부산물이다" | `kang build` 의 결과물이다. 커밋한다. |
| "문법을 몰라서 keyword 를 선언하지 않았다" | 문법은 아래에 있다. |
| "용어는 topic 본문에 풀어 쓰면 된다" | 도메인 용어는 keyword 로 선언하고 백틱으로 부른다. |

## 문법

```
---
description: 이 문서를 고를 때 읽는 설명. 무엇을 정하고 언제 읽는지 적는다.
---

import `docs`/`payment`.`결제` rev "cc2c2e"
import `docs`/`payment`#`결제 수단` as `결제 수단 정책` rev "a3f9c1"

keyword `환불`: 고객에게 `결제` 금액을 돌려주는 일 #`환불 금액`
keyword `환불`.`전액`: `결제` 금액 전체를 돌려주는 `환불`

## 환불 금액

`환불` 기간 안이면 `환불`.`전액` 을 한다. 돈은 `결제 수단 정책` 이 정한 수단으로 돌려준다.

exception `해외 결제 환불` pending

## 문서 검토 절차 // uncoded

사람이 분기마다 검토한다.
```

- **frontmatter**: `description` 하나만 있고 반드시 있어야 한다.
- **topic 밖에는** frontmatter, `import` 줄, `keyword` 줄, 빈 줄만 둔다. 문서 제목(`#`)과
  서문 단락은 error 다. 제목은 파일 경로가, 서문은 `description` 이 맡는다.
- **topic** 은 `##` 로 시작한다. 혼자 읽어도 뜻이 통하게 쓴다.
- **백틱은 전부 심볼 참조다.** 이 문서가 선언했거나 import 한 이름만 백틱으로 감싼다. 백틱
  문자 자체는 `` \` `` 로 쓴다. 코드 펜스 안은 해석하지 않는다.
- **keyword** 는 한 줄 정의가 필수다. 상세 topic 은 `#` 로 잇는다. 하위 개념은 `.` 로 잇는다.
- **import** 는 파일 맨 위에 한 줄에 하나씩 쓴다. 대상은 keyword(`.`), topic(`#`),
  exception(`!`) 이다. 다른 이름이 필요하면 `as` 를 쓴다. 본문에서 쓰지 않는 import 는
  error 다.
- **exception** 은 정책의 예외를 선언한다. 정책을 아직 정하지 않았으면 `pending` 을 붙인다.
  예외를 다루는 topic 에는 `cover` 와 import 한 예외 이름을 적는다.
- **`// uncoded`** 는 코드가 없는 것이 정상인 topic 에 붙인다.
- **`// iknow`** 는 다른 문서가 같은 이름을 다른 뜻으로 선언할 때 쓴다. 쓸 일이 생기면
  `kang build` 진단이 정확한 줄을 알려 준다.

## 무엇을 keyword 로 선언하나

프로젝트를 아는 사람이 듣자마자 특정 개념을 떠올리는 용어를 선언한다. 예: 환불, 정산일,
디지털 상품, 승격. id, name, status, 금액 같은 흔한 명사는 선언하지 않는다.

개념은 그 개념의 주인 문서 하나에서만 선언한다. 다른 문서가 이미 선언한 개념은 다시 선언하지
않는다. import 한다. `kang keywords` 로 이미 있는 개념을 먼저 확인한다.

## 정책을 조회할 때

`kang keywords` 로 개념 목록을 보고, `kang refs` 로 범위를 좁힌 다음, `kang show` 로 읽는다.
`.kang` 파일을 직접 열지 않는다. `kang show` 는 상위 정책을 모두 펼친 YAML 을 출력한다.
그래서 링크를 따라갈 필요가 없다.

## 정책을 쓸 때

1. `kang keywords` 와 `kang show` 로 관련 정책을 읽는다.
2. 다른 문서의 개념은 자기 문장으로 다시 쓰지 않는다. rev 없이 import 줄을 쓴다.
   ```
   import `docs`/`payment`.`결제`
   ```
3. `kang build` 를 실행한다. K020 진단이 rev 핀이 없다고 알린다.
4. 진단의 `[shell]` 명령으로 핀을 넣는다. 핀은 손으로 계산하거나 지어내지 않는다.
   ```
   kang bless 'docs/refund' --import 'docs/payment.결제'
   ```
5. `kang build` 가 종료 코드 0 으로 끝날 때까지 반복한다.

`kang bless` 는 이미 있는 import 줄에만 핀을 넣는다. import 줄이 없으면 종료 코드 2 로 실패한다.

## 컴파일이 실패했을 때

진단의 `fix` 항목을 **위에서부터 순서대로** 적용한다. `[edit]` 항목은 손으로 고칠 곳이다.
`[shell]` 항목은 셸 따옴표까지 들어 있는 명령이다. 복사해서 그대로 실행한다. `[edit]` 다음에
`[shell]` 이 오면 `[edit]` 을 먼저 한다. 진단을 읽지 않고 추측으로 고치지 않는다.

## 개념 이름을 바꿀 때

이름을 바꾸면 그 개념을 참조하던 모든 문서가 깨진다. 이름을 바꾼 다음 `kang build` 를
실행한다. 깨진 곳이 모두 진단으로 나온다. 진단의 `fix` 를 순서대로 적용한다. `kang refs` 는
본문의 참조만 세므로, 영향 범위를 미리 짐작할 때만 쓴다. 폐기한 이름은 묘비로 남기지 않는다.

## 코드를 고칠 때

**코드를 문서에 연결할 수 있다.** 코드가 어떤 정책을 구현하는지 애노테이션으로 적어 둔다.
그러면 그 정책의 원문이 바뀌는 순간 **빌드가 실패한다.** 문서만 고치고 코드를 잊는 일이
없어진다.

- `.kang/index.tsv` 나 `.kang/generated.ts` 가 있으면 이 프로젝트는 이미 코드를 연결했다.
  고친 정책을 구현하는 코드에 애노테이션을 붙이거나 핀을 갱신한다.
- 둘 다 없으면 연결은 하지 않는다. 작업 보고에 "코드를 문서에 연결할 수 있다" 를 한 줄로
  알린다.

Rust 에서는 `kang index .kang/index.tsv` 가 만든 인덱스를 proc-macro 가 빌드할 때 읽는다.

```rust
#[kang::topic("docs/A#결제의 방법", rev = "a3f9c1")]
pub fn process_payment() { }
```

TypeScript 에서는 `kang types .kang/generated.ts` 가 만든 타입을 `tsc` 가 읽는다. 클래스
메소드에 붙이고, tsconfig 에 `experimentalDecorators` 가 있어야 한다.

```typescript
@kangTopic("docs/A#결제의 방법", "a3f9c1")
process() { }
```

두 언어 모두, 없는 topic 을 가리키면 바로 에러가 난다. `rev` 가 낡았으면 리터럴 값이 맞지
않아 에러가 난다. 문서를 고쳤으면 `kang bless <문서> --import <심볼>` 로 핀을 갱신한다. 그다음
`kang index` 와 `kang types` 를 다시 실행한다. 생성된 파일은 커밋한다. 그래야 그 파일을 읽는
빌드를 다른 곳에서도 똑같이 재현할 수 있다.

**아직 없는 기능:** 애노테이션이 없는 코드까지 살펴서 "이 정책을 구현한 코드가 하나도 없다"
를 알려 주는 기능은 `kang inspect` 이고 v2 에 들어간다. 지금 실행하면 종료 코드 3 으로 끝난다.

## 명령

```
kang init                               에이전트 진입점과 첫 문서 생성
kang build                              컴파일 및 검증, 성공하면 .kang/graph.kang.json 생성
kang bless    <문서> --import <심볼>    rev 핀 갱신·삽입
kang list     [경로]                    문서 목록과 description
kang keywords [경로]                    키워드 목록
kang refs     <키워드>                  키워드를 참조하는 topic
kang show     <문서|토픽>               문서/토픽 조회 (YAML)
kang index    <경로>                    심볼 인덱스 생성 (탭 구분)
kang types    <경로>                    TypeScript 타입 생성 (topic)
kang --version                          버전 (지금 쓰는 계약 확인)
```

명령 인자에는 백틱을 쓰지 않는다. 경로는 `/`, 키워드는 `.`, topic 은 `#`, exception 은 `!` 로
잇는다. 이름에 공백이 있으면 셸에서 따옴표로 감싼다.

```
kang refs docs/A.결제
kang show 'docs/A#결제의 방법'
kang bless docs/B --import 'docs/A.결제 수단'
```

종료 코드는 다음과 같다.

- 0: 성공
- 1: 컴파일 error. 문서를 고친다. 항상 `error[Kxxx]` 진단이 함께 나온다.
- 2: 사용법 오류 또는 환경 오류. 명령줄이나 환경을 고친다.
- 3: 아직 구현되지 않은 기능

환경 오류는 세 가지다. git 저장소가 아니다. 문서를 읽지 못했다(권한이 없다, UTF-8 이
아니다). 출력을 쓰지 못했다(디스크가 가득 찼다). 원인이 문서 밖에 있으므로 문서를 고쳐도
해결되지 않는다.
