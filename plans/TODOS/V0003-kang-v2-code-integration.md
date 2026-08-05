# V0003 — kang v2 코드 연동 설계

문서와 코드를 기계적으로 대조하는 `kang inspect` 의 설계. **v2 다.** v1 은 `V0001-kang-language-design.md` 이며 이 문서의 내용은 v1 구현 대상이 아니다.

배경 문제 2번(코드-문서 불일치)을 푸는 절반이 여기 있다. v1 이 문서 사이의 참조를 강제한다면, v2 는 그 참조를 코드까지 연장한다.

---

## 1. `kang inspect`

`kang inspect` 가 코드에서 kang 심볼 참조를 수집하여 문서와 대조한다.

검사 항목:

- 특정 키워드를 참조하는 코드의 개수와 위치
- 특정 topic 을 참조하는 코드의 개수와 위치
- 특정 exception 을 참조하는 코드의 위치
- 특정 cover 를 구현하는 코드의 위치
- 죽은 정책 판정

v1 바이너리에서 호출하면 "v2 기능이며 아직 구현되지 않았다" 를 명시하고 종료 코드 3 으로 끝낸다 (V0001 6절).

## 2. 참조 전파

코드가 어떤 topic 을 참조하면, 그 topic 이 import 하는 상위 topic 도 참조된 것으로 간주하여 재귀적으로 전파한다. import 그래프가 DAG 이므로 "상위" 가 well-defined 이며, 순환으로 인한 오판이 발생하지 않는다.

**이 규칙의 소비자는 v2 에만 있다.** 참조 전파는 코드가 topic 을 참조할 때 정의되고, `kang show` 의 재귀 임베드는 import 관계를 직접 순회하므로 전파가 필요 없다. v1 의 `resolve` 는 이 함수를 갖지 않는다.

## 3. 참조 표기 수단

주석이 아니라 **언어에 내장된 선언 기능**을 1순위로 사용한다. 주석은 위치가 모호하지만, 애노테이션은 AST 노드에 구조적으로 결합되어 리팩터링을 따라간다.

| 언어 | 수단 | 특징 |
|---|---|---|
| Rust | proc-macro 속성 | 컴파일 타임 실행, 런타임 비용 0, **검증까지 가능** |
| JVM | 애노테이션 (`RetentionPolicy.SOURCE`) | 런타임 비용 0, 클래스·메서드·필드 모두 적용 |
| TypeScript | 데코레이터 | **일반 함수에 적용 불가**, 런타임 비용 있음 |
| Python | 데코레이터 | 모듈 레벨 상수에 적용 불가, 런타임 비용 있음 |

TypeScript 데코레이터는 레거시(`experimentalDecorators`)와 표준(Stage 3) 모두 **클래스·메서드·접근자·프로퍼티에만** 붙는다. 일반 함수 선언, 타입, 상수 export에는 쓸 수 없다. Python도 모듈 레벨 상수를 다루지 못한다.

따라서 **주석 폴백**을 함께 지원한다.

```
// kang: docs/A#결제의 방법        Rust, TS, Go, Java
#  kang: docs/A#결제의 방법        Python
```

`kang inspect`는 두 형태를 모두 인식하되, **애노테이션이 가능한 위치인데 주석을 사용한 경우 warn**을 낸다. 더 나은 수단이 있는데 쓰지 않으면 귀찮게 한다는 kang의 일관된 원칙을 따른다.

## 4. 코드 참조도 rev 핀을 갖는다

코드의 정책 참조에도 4.7의 rev 핀을 적용한다. 문서가 바뀌면 그것을 참조하는 코드가 깨지고, 사람이 코드를 확인한 뒤에야 통과한다.

```rust
#[kang::topic("docs/A#결제의 방법", rev = "a3f9c1")]
pub fn process_payment() { }
```

```typescript
@kangTopic("docs/A#결제의 방법", "a3f9c1")
process() { }
```

```
// kang: docs/A#결제의 방법 rev a3f9c1
```

`kang inspect`가 핀 불일치를 error로 보고하며, `kang bless <코드 위치>`로 해제한다.

## 5. 언어별 컴파일 타임 강제

`kang inspect --ci`가 모든 언어의 최소 보장선이다. 그 위에 언어별로 더 이른 피드백을 얹는다.

**Rust — proc-macro**

proc-macro는 컴파일 타임에 실행되므로 마커에 그치지 않고 검증을 수행한다.

- `kang build`가 심볼 인덱스 파일을 산출한다.
- proc-macro가 인덱스를 읽어 심볼의 실재 여부와 rev 일치를 확인한다.
- 불일치는 **컴파일 에러**다. `kang inspect` 이전에 빌드에서 잡힌다.
- `build.rs`의 `cargo:rerun-if-changed`로 인덱스를 추적하여 문서 변경 시 재빌드를 보장한다.
- 매크로는 원본 아이템을 그대로 반환하므로 런타임 비용은 0이다.

**TypeScript — 생성 타입**

데코레이터는 런타임 구성물이라 그 자체로는 `tsc`가 검증하지 못한다. 대신 `kang build`가 타입 파일을 생성하여 타입 시스템으로 밀어넣는다.

```typescript
// .kang/generated.ts
export interface KangTopics {
  "docs/A#결제의 방법": "a3f9c1";
  "docs/B#카드 결제":   "b721e0";
}

declare function kangTopic<K extends keyof KangTopics>(
  topic: K,
  rev: KangTopics[K],
): MethodDecorator;
```

- 존재하지 않는 topic 이름은 `keyof` 제약 위반으로 **타입 에러**다.
- 낡은 rev는 리터럴 타입 불일치로 **타입 에러**다.
- 런타임 실행 없이 순수 타입 체크로 걸리며, IDE에 즉시 표시된다.

데코레이터가 붙지 않는 일반 함수는 주석 폴백을 쓰며 `kang inspect --ci`가 담당한다. ESLint 커스텀 룰로 IDE 통합과 `--fix` 기반 rev 갱신을 제공하는 선택지가 있으나, 패키지가 늘어나므로 필요해질 때 판단한다.

## 6. 예외 미구현 검사

그래프 탐색만으로 판정되는 결정론적 검사다.

> 어떤 코드가 topic T를 참조하는데, T가 선언한 exception을 커버하는 정책을 구현한 코드가 어디에도 없으면 **warn**

문서 레벨에서만 잡던 예외 누락을 구현 레벨까지 확장한다.

## 7. uncoded 상태 기계

| topic 상태 | 코드 참조 있음 | 코드 참조 없음 |
|---|---|---|
| 일반 | 통과 | **warn** — 죽은 정책 |
| `// uncoded` | **warn** — 코드가 존재하는데 uncoded로 선언됨 | 통과 |

`// uncoded` 문법 자체는 v1의 파서가 인식하지만, 검사는 코드가 있어야 성립하므로 `kang inspect`가 수행한다.
