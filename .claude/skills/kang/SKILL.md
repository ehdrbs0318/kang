---
name: kang
description: 이 프로젝트의 문서는 .kang 파일이고 kang 컴파일러가 지킨다. 정책·도메인 용어·설계 문서를 읽거나 쓸 때, 개념 이름을 바꿀 때, kang build 가 실패했을 때 사용한다.
---

# kang

이 프로젝트의 문서는 `.kang` 이다. 한 개념의 주인은 정확히 한 파일이고, 다른 문서의
개념은 `import` 로만 당겨 쓴다. `kang build` 가 미선언 참조·중복 정의·참조한 원문이
바뀐 자리를 컴파일 error 로 낸다.

**`.kang` 파일을 직접 열지 않는다.** 원본은 `import` 간접 참조 때문에 마크다운보다 읽기
나쁘다. `kang show` 가 상위 정책을 전부 평탄화한 YAML 로 주므로 링크를 따라갈 일이 없다.

## 명령

```
kang build                              컴파일 및 검증
kang bless    <문서> --import <심볼>    rev 핀 갱신·삽입
kang list     [경로]                    문서 목록과 description
kang keywords [경로]                    키워드 목록
kang refs     <키워드>                  키워드를 참조하는 topic
kang show     <문서|토픽>               문서/토픽 조회 (YAML)
kang index    <경로>                    심볼 인덱스 산출 (탭 구분)
kang types    <경로>                    TypeScript 타입 산출 (topic)
kang --version                          버전 (쥔 계약이 무엇인지)
```

인자에 백틱을 쓰지 않는다. 경로는 `/`, 키워드는 `.`, topic 은 `#`, exception 은 `!` 로
잇는다. 공백이 있는 이름은 셸 인용이 필요하다.

```
kang refs docs/A.결제
kang show 'docs/A#결제의 방법'
kang bless docs/B --import 'docs/A.결제 수단'
```

종료 코드는 0 성공, 1 컴파일 error, 2 사용법·환경 오류, 3 아직 구현되지 않은 기능이다.
1 이면 문서를 고치고, 2 면 명령줄이나 환경을 고친다. 환경 오류에는 git 저장소가 아닌
경우, 문서를 읽지 못한 경우(권한·UTF-8 아님), 그리고 출력을 쓰지 못한 경우(디스크가 참,
리다이렉트가 깨짐)가 들어간다 — 셋 다 원인이 문서 밖에 있으므로 문서를 고쳐도 풀리지
않는다. **1 은 언제나 `error[Kxxx]` 진단을 동반한다** — 진단 없는 1 은 없다.

## 정책을 조회할 때

`kang keywords` 로 개념을 훑고 `kang refs` 로 좁힌 뒤 `kang show` 로 읽는다.
`.kang` 파일을 직접 열지 않는다.

## 정책을 쓸 때

먼저 `kang show` 로 관련 정책을 읽는다. 읽은 내용을 자기 문장으로 다시 쓰지 않고
`import` 로 참조한다. 서술을 복제하면 SoT 가 갈라진다.

## 컴파일이 실패했을 때

진단의 `fix` 를 그대로 적용한다. `[shell]` 항목은 셸 인용까지 포함된 명령이므로 복사해
실행하면 되고, `[edit]` 항목은 손으로 고칠 자리를 말한다. 진단을 읽지 않고 추측으로
고치지 않는다.

## 개념 이름을 바꿀 때

그 개념을 참조하던 모든 문서가 깨진다. 이름을 바꾼 뒤 `kang build` 를 돌린다. 깨진 자리
전부가 진단으로 나오므로 그 `fix` 를 순서대로 적용한다. `kang refs` 는 본문 참조만 세므로
영향 범위를 미리 가늠하는 데만 쓴다. 폐기한 이름의 묘비를 남기지 않는다.

## 코드를 고칠 때

**코드를 문서에 묶을 수 있다.** 코드가 어떤 정책을 구현하는지 애노테이션으로 적어 두면,
그 정책의 원문이 바뀐 순간 **빌드가 선다.** 문서만 고치고 코드를 잊는 일이 없어진다.

Rust 는 `kang index` 가 낸 인덱스를 proc-macro 가 빌드 타임에 읽는다.

```rust
#[kang::topic("docs/A#결제의 방법", rev = "a3f9c1")]
pub fn process_payment() { }
```

TypeScript 는 `kang types` 가 낸 타입을 `tsc` 가 읽는다.

```typescript
@kangTopic("docs/A#결제의 방법", "a3f9c1")
process() { }
```

둘 다 없는 topic 은 즉시 에러이고, 낡은 `rev` 는 리터럴 불일치로 걸린다. 문서를 고쳤으면
`kang bless <문서> --import <심볼>` 로 핀을 갱신한 뒤 `kang index`·`kang types` 를 다시
돌린다. 생성물은 커밋한다 — 그래야 그것을 읽는 빌드가 재현된다.

**아직 없는 것:** 애노테이션이 붙지 않은 코드까지 훑어 "이 정책을 구현한 코드가 하나도
없다" 를 말하는 것은 `kang inspect` 이며 v2 다. 부르면 종료 코드 3 을 낸다.
