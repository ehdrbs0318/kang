# V0004 — 코드 결합과 도그푸딩

`V0003-kang-v2-code-integration.md` 의 설계를 구현하고, kang 을 자기 저장소에 적용한다.

**선행:** `V0002-kang-v1-implementation.md` (Task 1~14 완료, 커밋 76개, 테스트 309, `main` 대비 랜딩 가능)
**설계 원천:** `V0003-kang-v2-code-integration.md` (§1 inspect / §3 참조 표기 / §4 코드 rev 핀 / §5 언어별 강제)
**언어 계약:** `V0001-kang-language-design.md`

---

## 왜 이 플랜인가

v1 은 **문서 사이**의 참조를 강제한다. 이 플랜은 그 참조를 **코드까지** 연장하고, 동시에 kang 을 kang 자신에게 적용한다.

세 목표가 서로를 검증한다.

| 목표 | 무엇을 증명하는가 |
|---|---|
| (a) 저장소 문서를 `.kang` 으로 이관 | kang 이 **실제 코퍼스**에서 견딘다. 지금 모든 성능·천장 수치는 합성 픽스처에서 잰 값이다 |
| (b) kang 자신의 Rust 코드에 매크로 | 코드-문서 결합이 **컴파일 타임에** 성립한다. 자기 저장소가 첫 사용자다 |
| (c) TypeScript 구현체 + 문서 충실도 테스트 | kang 의 실제 소비자(**Rust 툴체인이 없는 프로젝트**)에서 성립한다 |

---

## 착수 전 해결 필요 (BLOCKING)

세 충돌은 설계 결정이 필요하며, 결정 없이 착수하면 되돌리는 비용이 크다.

### B1. 의존성 금지 vs proc-macro

V0001 은 의존성을 `sha2` 하나로 못박았다. V0003 §5 의 proc-macro 는 `syn`·`quote` 를 요구한다.

**제안: cargo workspace 로 가른다.**

```
kang/                       워크스페이스 루트
  crates/kang/              CLI·컴파일러. 의존성 sha2 하나 (V0001 제약 유지)
  crates/kang-macros/       proc-macro. syn·quote·proc-macro2
  crates/kang-macros-test/  매크로 소비 테스트 (dev 전용)
```

근거: V0001 의 제약은 **컴파일러의 신뢰 경계**에 대한 것이다 — 문서를 읽고 진단을 내는 층이 제3자 코드에 의존하지 않는다는 뜻이다. `kang-macros` 는 소비자 측 라이브러리이고 컴파일러 산출물(인덱스)만 읽는다. **다만 이 해석을 V0001 에 한 문장으로 명문화해야 한다** — 그러지 않으면 다음 사람이 제약을 어겼다고 읽는다.

**결정 필요:** 워크스페이스로 가를 것인가, `syn` 없이 손으로 토큰을 파싱할 것인가(가능하지만 속성 인자 파싱이 취약해진다).

### B2. v1 에 심볼 인덱스 파일이 없다

V0003 §5 는 "`kang build` 가 심볼 인덱스 파일을 산출한다" 를 전제하지만 v1 의 `build` 는 **어떤 파일도 쓰지 않는다**(진단만 낸다).

**결정 필요 셋:**
1. **산출 경로** — `.kang/index.yaml` 인가, `--index <path>` 인가. 전자면 `.gitignore` 에 넣는가(생성물) 커밋하는가(CI 에서 재생성 없이 쓰려면 커밋이 편하다).
2. **형식** — 기존 `src/yaml.rs` 이미터를 재사용하는가. YAML 은 사람이 읽기 좋지만 **proc-macro 가 파서를 가져야 한다**(의존성 또는 손 파서). 한 줄 `키\t해시` 텍스트면 파서가 3줄이다.
3. **`build` 가 항상 쓰는가, 플래그가 있을 때만 쓰는가.** 항상 쓰면 `build` 가 읽기 전용이 아니게 되고 스펙 6.2 를 고쳐야 한다.

**제안:** `kang index <경로>` 를 **별도 명령**으로 둔다. `build` 의 읽기 전용 성질을 지키고, 형식은 탭 구분 텍스트로 해 proc-macro 파서를 의존성 0으로 만든다.

### B3. 부트스트랩 순환

kang 자신의 Rust 코드가 `kang::topic` 매크로를 쓰면: 매크로가 인덱스를 읽어야 하고 → 인덱스는 `kang index` 가 만들고 → 그 바이너리는 매크로가 붙은 코드를 컴파일해야 만들어진다.

**결정 필요:** 매크로가 **인덱스 부재 시 무엇을 하는가.**
- (i) 컴파일 에러 — 부트스트랩이 불가능해진다.
- (ii) 조용히 통과 — 인덱스를 지우면 검증이 사라진다. **"검증하면 거짓" 의 새 사례**가 된다.
- (iii) **warn 을 내고 통과 + `KANG_REQUIRE_INDEX` 환경 변수로 CI 에서 에러로 승격.** Task 13 의 `KANG_REQUIRE_YAML` 과 같은 형태이며 저장소에 선례가 있다.

**제안: (iii).** 그리고 부트스트랩 순서를 `Makefile` 이나 `xtask` 로 못박아 사람이 순서를 기억하지 않게 한다.

---

## Task 1 — CI 게이트

**최종 리뷰 우선순위 1.** 지금 워크플로는 `v*` 태그에서만 돌아 **일상 푸시·PR 에서 `cargo test` 가 0회 실행**되고 `clippy`·`fmt` 는 CI 에 아예 없다. 크레이트가 늘어나는 이 플랜에서 회귀를 잡을 곳이 없다.

**파일:** `.github/workflows/ci.yml` (신규)

- [ ] `pull_request` + `push: [main]` 트리거, `ubuntu-latest` 한 대
- [ ] `cargo test` (`KANG_REQUIRE_YAML=1`), `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`
- [ ] python3 + pyyaml 보장 (Task 13 이 만든 게이트가 조용히 빠지지 않게)
- [ ] 매트릭스는 `release.yml` 에만 남긴다 — CI 에서 4타깃을 돌릴 이유가 없다
- [ ] **검증:** 일부러 실패하는 커밋을 브랜치에 올려 게이트가 빨간지 확인한다 (red-green)

## Task 2 — 주소 문법 완결

**최종 리뷰 우선순위 2.** V0002 완료 조건의 미달 항목이고, **코드 심볼 이름이 `crate::mod::fn` 을 담으므로 매크로 착수 전에 닫아야 한다.**

**파일:** `src/parse.rs`, 스펙 `V0001` 6.0

- [ ] **심볼 이름의 `/` 금지** — 스펙 6.0:415 가 규칙을 정했고 :418 이 `K105` 형제 판정으로 배당했다. 미구현이며 **셋 중 유일하게 빌드를 봉쇄한다**(`## 환불/취소` 는 컴파일을 통과하되 그 topic 을 import 한 문서의 핀을 어떤 명령으로도 붙일 수 없다)
- [ ] **keyword 이름 한 조각의 `.` 금지** — 최종 리뷰는 "무해하므로 스펙 문면에서 빼는 편이 맞다" 고 판정했다. **금지할지 스펙에서 뺄지 결정하고 근거를 남긴다**
- [ ] **CLI 주소에 코드 심볼 이름을 담을 수 있는지 판정** — `crate::mod::fn` 의 `::`, 경로 조각의 `/`, 제네릭의 `<>`. 담을 수 없으면 코드 참조는 **문서 심볼만** 가리키게 좁힌다(V0003 §4 의 예시가 이미 그렇다). 담아야 하면 이스케이프를 결정한다 — **`parse.rs:121-123` 의 `K105` 가 확립한 "CLI 로 주소를 댈 수 없는 이름은 이름을 고친다" 처방이 코드 심볼에는 통하지 않는다**(코드 이름을 kang 이 정할 수 없다)
- [ ] 회귀: 오늘 통과하는 이름 전부가 여전히 통과 (`결제 v1.2 개요`·`` `C#` ``·`` `무료! 상품` ``·계층 keyword)

## Task 3 — 심볼 인덱스 산출

**B2 의 결정을 구현한다.**

**파일:** `src/index.rs` (신규), `src/main.rs`

```rust
/// 심볼 인덱스를 한 줄 하나씩 쓴다. 소비자가 의존성 없이 파싱할 수 있어야 한다.
/// 형식: {주소}\t{종류}\t{rev}\n
pub fn write_index(project: &Project, table: &SymbolTable, out: &mut impl Write) -> io::Result<()>;
```

- [ ] `kang index <경로>` 명령 — error 가 있으면 쓰지 않고 exit 1 (스펙 6절 핵심 규칙)
- [ ] 주소는 **`ImportAddress::parse` 가 되받을 수 있는 형태**여야 한다 — 왕복 테스트로 못박는다
- [ ] `--help` 에 추가하고 **양방향 게이트**(미구현 절에서 뺀 것이 실제로 exit 3 이 아닌지)를 통과시킨다
- [ ] 원자적 쓰기 — `bless.rs` 가 세운 임시 파일 + `rename` 규약을 재사용한다. **사본을 만들지 말고 공용 함수로 올린다**
- [ ] `.gitignore` 결정 (B2-1)

## Task 4 — 워크스페이스 분할

**B1 의 결정을 구현한다. 코드 이동만이며 동작 변경 0 이어야 한다.**

- [ ] `Cargo.toml` 을 워크스페이스로, 기존 크레이트를 `crates/kang/` 으로 이동 (`git mv`)
- [ ] `crates/kang-macros/` 생성 — `proc-macro = true`
- [ ] **`cargo test` 309개가 전부 그대로 통과해야 한다.** 하나라도 깨지면 이동이 아니라 변경이다
- [ ] `release.yml`·`ci.yml`·`README.md` 의 경로 갱신
- [ ] **V0001 에 의존성 제약의 범위를 한 문장 명문화** (B1) — "제약은 컴파일러 크레이트에 적용된다"

## Task 5 — proc-macro

**V0003 §4·§5.** 인덱스를 읽어 심볼 실재와 rev 일치를 **컴파일 에러**로 검증한다.

```rust
/// #[kang::topic("docs/A#결제의 방법", rev = "a3f9c1")]
/// 원본 아이템을 그대로 반환한다 — 런타임 비용 0.
#[proc_macro_attribute]
pub fn topic(attr: TokenStream, item: TokenStream) -> TokenStream;

#[proc_macro_attribute]
pub fn keyword(attr: TokenStream, item: TokenStream) -> TokenStream;

#[proc_macro_attribute]
pub fn covers(attr: TokenStream, item: TokenStream) -> TokenStream;
```

- [ ] 인덱스 경로 결정 — 환경 변수 `KANG_INDEX` 인가 `build.rs` 가 넘기는가
- [ ] **인덱스 부재 시 동작 (B3)** — warn + `KANG_REQUIRE_INDEX` 로 승격
- [ ] `build.rs` 의 `cargo:rerun-if-changed` 로 인덱스를 추적 — **문서를 바꾸면 재빌드되는 것을 실제로 확인한다**(수동으로 `touch` 하지 않고)
- [ ] 심볼 부재 → 컴파일 에러. 메시지가 **스펙 5.1.1 의 세 요소**(무엇이 틀렸나 / 어디인가 / 어떻게 고치나)를 담는가. `kang bless` 를 짝지을 수 있는가
- [ ] rev 불일치 → 컴파일 에러 + `kang bless` 처방. **그 처방을 그대로 복사해 실행하면 실제로 낫는지 확인한다**(V0002 가 세운 fix 계약)
- [ ] 매크로가 원본 아이템을 그대로 반환하는지 — 확장 결과를 `cargo expand` 없이 확인할 방법을 정한다

## Task 6 — kang 자신의 코드에 매크로 적용

**사용자 목표 (b).** 자기 저장소가 첫 사용자다.

- [ ] `crates/kang/src/*.rs` 의 주요 함수에 `#[kang::topic(...)]` 을 붙인다 — **먼저 Task 9 가 그 topic 들을 `.kang` 문서로 만들어야 한다.** 순서 의존이므로 Task 9 뒤에 온다
- [ ] 부트스트랩 순서를 `xtask` 나 `Makefile` 로 못박는다 (B3)
- [ ] **문서를 고치면 컴파일이 깨지고, `bless` 후 통과하는 것을 손으로 왕복한다**
- [ ] CI 에서 `KANG_REQUIRE_INDEX=1` 로 돌린다

## Task 7 — TypeScript 타입 생성

**V0003 §5.** 데코레이터는 런타임 구성물이라 `tsc` 가 검증하지 못하므로 타입 시스템으로 민다.

```typescript
// .kang/generated.ts — kang 이 생성한다. 손으로 고치지 않는다.
export interface KangTopics {
  "docs/A#결제의 방법": "a3f9c1";
}
declare function kangTopic<K extends keyof KangTopics>(topic: K, rev: KangTopics[K]): MethodDecorator;
```

- [ ] `kang index --ts <경로>` 인가 별도 명령인가 결정
- [ ] 존재하지 않는 topic → `keyof` 제약 위반으로 **타입 에러**인 것을 실제 `tsc` 로 확인
- [ ] 낡은 rev → 리터럴 타입 불일치로 **타입 에러**인 것을 확인
- [ ] **한글 심볼 이름이 TS 식별자·리터럴에서 문제 없는지** 확인 — 문자열 리터럴 키라 괜찮을 것이나 실측한다
- [ ] 생성 파일의 헤더에 "생성물이며 손으로 고치지 않는다" 를 명시

## Task 8 — 예시 TypeScript 구현체 + 문서 충실도 테스트

**사용자 목표 (c).** kang 의 실제 소비자는 **Rust 툴체인이 없는 프로젝트**다.

**파일:** `examples/ts-consumer/` (신규)

- [ ] 최소 TS 프로젝트 — `.kang` 문서 2~3개 + 그 정책을 구현한 코드
- [ ] `kang` 바이너리를 **릴리즈 아티팩트로 받아** 쓰는 형태(소스 빌드 금지) — README 의 curl 경로가 실제로 동작하는지 이 예시가 검증한다
- [ ] `npm test` 가 (1) `kang build` (2) 타입 생성 (3) `tsc --noEmit` (4) 문서 충실도 검사를 순서대로 돈다
- [ ] **문서 충실도 검사** — `kang inspect` 가 v2 이므로 이 태스크에서 만들 범위를 정한다. 최소선: 코드가 참조하는 topic 이 전부 실재하고 rev 가 일치하는가. `inspect` 본체(죽은 정책 판정·참조 전파)는 별도 태스크로 가른다
- [ ] **정책 문서를 고치면 `npm test` 가 실패하는 것을 실제로 확인한다.** 이것이 이 태스크의 유일한 성공 기준이다

## Task 9 — 도그푸딩 이관

**사용자 목표 (a).** 최종 리뷰 우선순위 5이며 **Task 6 의 선행**이다.

- [ ] 이관 대상과 순서 결정 — `plans/`·`docs/adr/`·`CONTEXT.md` 중 무엇을 먼저. **스펙 자신(`V0001`)을 `.kang` 으로 옮기는 것이 가장 강한 도그푸딩이지만 순환 위험이 있다** — 스펙이 깨지면 컴파일러의 진실 원천이 사라진다
- [ ] 루트의 추적되지 않는 `kang init` 산출물 넷 처리 결정 (`.claude/`·`AGENTS.md`·`CLAUDE.md`·`docs/example.kang`)
- [ ] **참조 병합 천장 재측정** — `check.rs` 의 마커가 담은 "충돌 0건" 은 **마크다운 코퍼스**에서 잰 값이다. 실제 `.kang` 코퍼스가 유일한 재측정 수단이고, 이번에는 **측정 스크립트를 저장소에 남긴다**(Task 12 가 안 남겨 재측정이 일회성으로 끝났다)
- [ ] `K114`(topic 뒤 import) 의 실제 발화 빈도 측정 — 산문이 많은 코퍼스가 그 진단의 첫 실사용이다
- [ ] 성능 실측 — 지금 모든 수치는 합성 픽스처 값이다

## Task 10 — show 스키마를 소비자 계약으로 정본화

**최종 리뷰 우선순위 3. Task 7·8 의 선행** — TS 클라이언트가 첫 줄에서 부딪히는 것이 포인터 재조립 규약이다.

- [ ] keyword 포인터 비대칭을 스펙 6.4 에 명문화 — 포인터는 `{path}.{name}`, 전개 자리는 `name`/`path` 분리
- [ ] **js-yaml·go-yaml 파싱 게이트** — 지금 검증은 PyYAML·Ruby Psych 경로만 돌았다. YAML 1.2 파서에서 `=`·`true` 의 해석이 다를 수 있다
- [ ] 빈 절을 생략하는가 `[]` 로 내는가 확정 (V0002 M7 이월) — **실제 소비 코드(Task 8)를 보고 정한다**

## Task 11 — 진단 문면 계약 확정

**최종 리뷰 우선순위 4.** `inspect` 류 도구와 TS 툴링이 진단을 **기계 파싱**한다.

- [ ] **`Diagnostic` 에 `detail: Option<String>` 을 둘지 결정** — 스펙 5.1.1 의 3단 배치를 구현이 message 한 줄로 뭉쳤다. 세 요소는 전부 있으므로 스펙 요구는 충족되고 배치만 다르다. **사람의 판정이 필요하다**
- [ ] 진단 문체 통일 — `parse.rs` 의 note 는 마침표 없는 명사구·스펙 인용 0건, `check.rs`·`resolve.rs` 는 완전문·스펙 인용 있음. **한 실행에 두 문체가 섞이면 기계가 규칙을 하나로 배울 수 없다**
- [ ] **stderr EPIPE** — `kang build 2>&1 | head` 가 exit 101. `찍기` 는 stdout 만 지키고 진단이 흐르는 stderr 는 무보호다(`eprint!` 34곳). `main.rs` 의 두 자리가 진단 전량을 내므로 그 둘만 태운다. **`set -euo pipefail` 이 CI 표준이라 실질 피해가 있다**
- [ ] `K051`(권한 없음·비 UTF-8)의 종료 코드를 1 → 2 로. 진단 자신의 fix 가 `ls -l`·`file -I` 인데 "문서를 고쳐야 한다" 로 분기된다
- [ ] 스펙 5.1.1 에 **"적용하면 해소되는 fix" vs "확인만 하는 fix"** 를 성질로 구분하는 문장 — 지금 스펙에 그 구분이 없어 `ls -l` fix 의 옳고 그름을 판정할 근거가 없다

---

## 의존 순서

```
Task 1 (CI)  ─────────────────────────────────────┐
Task 2 (주소 문법) ──┬─→ Task 3 (인덱스) ─→ Task 4 (워크스페이스) ─→ Task 5 (매크로) ─┐
                    │                                                              │
Task 11 (진단 계약) ─┘                                                              │
Task 9 (도그푸딩) ───────────────────────────────────────────────────→ Task 6 (자기 적용)
Task 10 (show 계약) ─→ Task 7 (TS 타입) ─→ Task 8 (TS 예시 + 충실도)
```

Task 1 은 다른 전부의 안전망이므로 **가장 먼저**. Task 2 는 매크로가 쓸 이름 문법을 정하므로 Task 3 앞. Task 9(도그푸딩)는 Task 6 의 선행이면서 독립적으로 진행 가능하다.

## 완료 조건

- [ ] `cargo test` 전부 통과, `clippy -D warnings`·`fmt --check` 통과, **CI 가 PR 에서 돈다**
- [ ] 스펙 6.0 의 세 금지가 전부 구현됨 (V0002 미달 항목)
- [ ] **kang 자신의 코드에 매크로가 붙어 있고, 문서를 고치면 `cargo build` 가 깨지고 `kang bless` 로 낫는다**
- [ ] **`examples/ts-consumer` 의 `npm test` 가 통과하고, 정책 문서를 고치면 실패한다**
- [ ] **저장소의 실제 문서가 `.kang` 으로 이관되어 `kang build` 가 exit 0** (V0002 미달 항목)
- [ ] 참조 병합 천장이 **실제 `.kang` 코퍼스**에서 재측정되었고 측정 스크립트가 저장소에 있다
- [ ] `kang index` 가 낸 주소를 `ImportAddress::parse` 가 되받는다 (왕복)
- [ ] 태그 푸시 검증 — **remote 가 붙은 뒤에만 가능하다.** 붙지 않으면 미달로 남긴다

## NOT in scope

- `kang inspect` 본체 — 죽은 정책 판정, 참조 전파, 예외 미구현 검사, uncoded 상태 기계 (V0003 §1·§2·§6·§7). Task 8 은 "코드가 참조하는 topic 이 실재하고 rev 가 일치하는가" 최소선만 만든다
- JVM·Python 애노테이션 (V0003 §3 의 표에 있으나 소비자가 없다)
- ESLint 커스텀 룰 (V0003 §5 가 "필요해질 때 판단한다" 로 남겼다)
- 주석 폴백(`// kang: ...`) — 애노테이션 경로가 먼저 성립해야 폴백의 필요를 잴 수 있다

## 이 플랜이 물려받는 열린 항목

V0002 의 SDD 원장(`.superpowers/sdd/V0002-kang-v1-implementation/progress.md`)에 근거가 있다.

| 항목 | 어느 Task |
|---|---|
| 심볼 이름 `/`·keyword 조각 `.` 금지 | 2 |
| `Diagnostic` 의 `detail` 필드 (사람 판정 필요) | 11 |
| stderr EPIPE, `K051` 종료 코드, 진단 문체 | 11 |
| `show` 빈 절 생략 vs `[]` | 10 |
| js-yaml·go-yaml 파싱 게이트 | 10 |
| 참조 병합 천장 재측정 + 측정 스크립트 | 9 |
| 루트의 추적되지 않는 `init` 산출물 넷 | 9 |
| `K114` 천장 (문법 오류 + 자리 오류가 겹친 import) | 9 에서 실측 후 판단 |
| README 의 `OWNER` 플레이스홀더 | remote 가 붙을 때 |
| Linux ext4/NFS `rename` 원자성, ENOSPC | 1 (CI 가 ubuntu 라 여기서 잰다) |
| 생성된 `SKILL.md` 가 kang 버전을 따라가지 않음 | 미정 — 사용자가 생긴 뒤 실측 |
| `ls` fix 3자리의 렌더 실행 테스트 부재 | 11 |
| `ponytail:` 마커 30건 재고 | 각 Task 가 건드리는 것만 |
