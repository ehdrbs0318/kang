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

## 착수 전 결정 — 해소됨 (2026-08-06)

세 충돌을 사용자 결정과 저장소 선례로 닫았다. **재론하지 않는다.**

### B1. 의존성 — V0001 §10.1 에 크레이트별 목록을 두었다 (해소)

**컨트롤러 오기 정정:** `sha2` 제약은 V0001 이 아니라 **V0002 의 전역 제약**(14행)과 의존성 표(1094행)에 있었다. V0001 §10 에는 의존성 목록이 아예 없었다.

**결정(사용자):** 제약의 **범위를 재해석하지 않고 목록 자체를 고친다.** V0001 에 §10.1 「의존성 목록」을 새로 두어 크레이트별로 열거했다 — `kang` 은 `sha2`, `kang-macros` 는 `syn`·`quote`·`proc-macro2`. **목록에 없는 것은 쓰지 않으며, 늘려야 하면 그 절을 고치고 근거를 남긴다.**

컨트롤러가 제안했던 "제약은 컴파일러의 신뢰 경계에만 적용된다" 는 해석 방식은 **채택하지 않았다.** 같은 결과를 내지만, 제약을 읽는 사람이 범위를 다시 추론해야 하고 그 추론이 갈릴 수 있다. 목록은 갈리지 않는다.

**워크스페이스 분할은 선택이 아니라 Rust 의 요구다** — `proc-macro = true` 는 같은 크레이트의 바이너리·라이브러리와 공존하지 못한다.

### B2. 심볼 인덱스 — `kang index` 별도 명령 + 탭 구분 텍스트 (해소)

**결정(사용자):**

```
docs/A.결제\tkeyword\ta3f9c1
docs/A#결제의 방법\ttopic\tb721e0
docs/A!무료 상품\texception\tb721e0
```

- **`build` 의 읽기 전용 성질을 지킨다** — 스펙 6.2 를 고치지 않는다.
- **proc-macro 의 파서가 의존성 0으로 3줄이다** — `line.split('\t')`.
- 사람이 읽을 수 있고 `grep` 이 통한다.
- 중첩 구조를 담을 수 없으나 지금 담을 것이 없다. 필요해지면 그때 판단한다.

**주소는 `ImportAddress::parse` 가 되받을 수 있는 형태여야 한다** — 왕복 테스트로 못박는다. 이것이 이 형식의 유일한 취약점이다: 탭이 심볼 이름에 들어가면 형식이 깨진다. **스펙 6.0 이 이름에 금지한 문자 목록에 탭을 넣을지 Task 3 에서 판정한다.**

### B3. 부트스트랩 순환 — warn + `KANG_REQUIRE_INDEX` (컨트롤러 결정)

**저장소 선례를 따른다.** Task 13 이 같은 문제를 `KANG_REQUIRE_YAML` 로 닫았다 — 파서가 없으면 건너뛰되 환경 변수가 켜지면 panic.

- 인덱스 부재 → **warn 을 내고 통과.** 부트스트랩이 가능해진다.
- `KANG_REQUIRE_INDEX=1` → **컴파일 에러.** CI 와 릴리즈에서 켠다.
- 조용히 통과(선택지 ii)는 채택하지 않는다 — **인덱스를 지우면 검증이 사라지는데 빌드가 성공한다.** 이 저장소가 열세 번 걸린 "검증하면 거짓" 의 새 사례가 된다.
- 부트스트랩 순서를 `xtask` 나 `Makefile` 로 못박아 사람이 순서를 기억하지 않게 한다.

## Task 1 — CI 게이트

**최종 리뷰 우선순위 1.** 지금 워크플로는 `v*` 태그에서만 돌아 **일상 푸시·PR 에서 `cargo test` 가 0회 실행**되고 `clippy`·`fmt` 는 CI 에 아예 없다. 크레이트가 늘어나는 이 플랜에서 회귀를 잡을 곳이 없다.

**파일:** `.github/workflows/ci.yml` (신규)

- [x] `pull_request` + `push: [main]` 트리거, 러너 한 대
  - `.github/workflows/ci.yml` 신규. job 하나(`check`), 스텝 넷.
  - **`ubuntu-latest` 대신 `ubuntu-24.04` 로 고정했다.** 같은 항목이 "`release.yml` 과 같은 러너 이미지를 쓰라" 도 요구하는데 `release.yml` 은 `ubuntu-24.04` 를 핀한다(:16, :48). `ubuntu-latest` 는 GitHub 이 이미지를 올리는 시점에 흘러가므로 두 요구를 동시에 만족할 수 없고, 흘러간 뒤에는 "CI 는 통과하는데 릴리즈가 깨진다" 가 정확히 생긴다. actionlint 가 `ubuntu-24.04` 를 유효 라벨로 인정한다(오염판 A 의 출력에 목록이 있다).
- [x] `cargo test` (`KANG_REQUIRE_YAML=1`), `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`
  - 세 명령을 로컬에서 실제로 돌렸다: `cargo test` exit 0 / **309 passed, 0 failed**, `clippy` exit 0, `fmt --check` exit 0.
- [x] python3 + pyyaml 보장 (Task 13 이 만든 게이트가 조용히 빠지지 않게)
  - `release.yml:26` 과 **바이트 단위로 같은 줄**을 썼다 (`python3 -m pip install --break-system-packages --quiet pyyaml`).
  - **게이트에 이빨이 있음을 2×2 로 확인했다** — 파서 있음+ON → pass, 파서 가림+ON → **FAILED exit 101** (`tests/cli.rs:2517` panic: "KANG_REQUIRE_YAML 이 켜졌는데 python3 또는 pyyaml 이 없다"), 파서 가림+OFF → pass(건너뛰기). 즉 실패를 만드는 것은 환경 변수이며 파서 부재만으로는 조용히 빠진다.
- [x] 매트릭스는 `release.yml` 에만 남긴다 — CI 에서 4타깃을 돌릴 이유가 없다
  - `ci.yml` 은 `runs-on: ubuntu-24.04` 한 대. 28줄로 `release.yml`(60줄)보다 짧다.
- [x] **`release.yml` 의 중복 `cargo test` 4회는 줄이지 않는다** (판단 및 근거)
  - `ci.yml` 은 linux x86_64 **한 타깃만** 돈다. `release.yml:28` 의 `cargo test` 를 빼면 **macOS(aarch64·x86_64)와 linux arm64 에서 테스트가 어디서도 돌지 않는다.**
  - 이 스위트는 플랫폼 의존이 큰 종류다 — `tests/cli.rs` 가 바이너리를 `Command` 로 띄우고 임시 디렉터리와 실제 git 저장소를 만든다(`임시_루트`, `git_저장소로`). 경로·파일 락·프로세스 동작이 OS 마다 갈리는 자리다.
  - 태그 릴리즈는 드물고 산출물이 사용자에게 나가므로, 4회 중복은 낭비가 아니라 **네 아티팩트 각각에 대한 안전망**이다. `release.yml` 을 건드리지 않았다.
- [ ] **검증:** 일부러 실패하는 커밋을 브랜치에 올려 게이트가 빨간지 확인한다 (red-green)
  - **미완 — remote 가 붙은 뒤로 미룬다.** 이 저장소에는 `git remote` 가 없어 워크플로를 GitHub 에서 실제로 실행할 수 없고, `act` 도 설치되어 있지 않다. 가짜 remote 나 시뮬레이션으로 대신하지 않았다 — 그것은 이 항목이 요구하는 증거가 아니다.
  - **대신 확보한 증거:** ① `ci.yml` 이 PyYAML 로 파싱된다(`on` 이 YAML 1.1 규칙으로 boolean `True` 키가 되는데, `release.yml` 도 동일하며 GitHub·actionlint 파서에서는 문자열이다). ② `actionlint` exit 0, 그리고 **오염판으로 red-green** — 잘못된 러너 라벨 → exit 1, `actions/checkout@v4` 에 없는 입력 → exit 1. 린터에 이빨이 있다. ③ 워크플로가 돌리는 세 명령이 로컬에서 통과. ④ `KANG_REQUIRE_YAML` 2×2 (위 항목).
  - **여전히 검증 못 한 것:** 워크플로가 GitHub 러너에서 실제로 도는지, 그리고 `ubuntu-24.04` 이미지에 `clippy`·`rustfmt` 컴포넌트가 선설치되어 있는지. 후자는 `release.yml` 과 같은 방식(러너 선설치 툴체인 신뢰, `rustup` 설치 스텝 없음)을 따른 결과이며, 없을 경우 **조용히 빠지지 않고 "no such command" 로 시끄럽게 빨개진다** — 받아들일 수 있는 실패 양식이므로 `rustup component add` 를 넣지 않았다.

## Task 2 — 주소 문법 완결

**최종 리뷰 우선순위 2.** V0002 완료 조건의 미달 항목이고, **코드 심볼 이름이 `crate::mod::fn` 을 담으므로 매크로 착수 전에 닫아야 한다.**

**파일:** `src/parse.rs`, 스펙 `V0001` 6.0

- [x] **심볼 이름의 `/` 금지** — 스펙 6.0:415 가 규칙을 정했고 :418 이 `K105` 형제 판정으로 배당했다. 미구현이며 **셋 중 유일하게 빌드를 봉쇄한다**(`## 환불/취소` 는 컴파일을 통과하되 그 topic 을 import 한 문서의 핀을 어떤 명령으로도 붙일 수 없다)
  - **`K115`** 신설 (`src/parse.rs`). `src` 가 실제 발화하는 것이 `K101`~`K114` 이므로 `K100`-`K199` 대역의 다음 빈 번호다. `K105` 를 재사용하지 않은 이유: 그 문면("topic 헤딩에 백틱이 있습니다")이 keyword·exception 에는 거짓이고, 스펙 :419 가 `K113` 에 새 번호를 준 것과 같은 배당 규칙이다
  - 판정 자리 **둘**: `백틱_검사`(keyword·exception·import·iknow·cover 일곱 호출자가 전부 지나간다) + topic 헤딩(백틱을 쓰지 않아 `백틱_검사` 를 거치지 않는다). 본문 참조 스캔은 중복 로직을 지우고 `백틱_검사` 를 부르게 바꿨다 — 4줄 순감
  - **선언만이 아니라 참조도 거절한다.** 선언만 막으면 어느 문서도 그 이름을 선언할 수 없어 `K001` 이 언제나 "이 문서에서 선언하세요" 를 처방하고, 그대로 적용하면 `K115` 가 난다 (스펙 5.1.1 의 "그대로 적용 가능한 `fix`" 위반). 실측으로 확인한 사슬이다
  - fix 는 `[edit]` 이고 줄 번호를 `action` 에 넣지 않는다 (ADR-0003)
- [x] **keyword 이름 한 조각의 `.` 금지** — 최종 리뷰는 "무해하므로 스펙 문면에서 빼는 편이 맞다" 고 판정했다. **금지할지 스펙에서 뺄지 결정하고 근거를 남긴다**
  - **판정: 스펙에서 뺀다. 코드 0줄.** 스펙 6.0 의 금지 항목 3을 삭제하고 그 자리에 근거를 남겼다
  - 근거(실측): `` keyword `버전 1.2` `` 는 `build`·`keywords`·`refs`·`show`·`K001` fix→`import`→`bless`→`build` 왕복이 **전부 exit 0** 이다. 스코프 키가 조각을 `.` 으로 이은 전체 이름이라 두 읽기가 같은 키로 수렴한다
  - **"조용히 다른 심볼로 읽히는" 경우는 없다** — 두 읽기가 다른 선언을 가리키려면 `` `결제`.`카드` `` 와 `` `결제.카드` `` 가 한 스코프에 함께 묶여야 하고 그것은 이미 `K052` 가 처방과 함께 거부한다 (실측)
  - 금지하면 `Node.js`·`버전 1.2` 처럼 오늘 통과하는 자연스러운 이름을 거부하게 된다. 원장 Task 12 의 교훈("제약을 넓히는 방향의 실수가 특히 위험하다")이 그대로 적용된다
  - 남는 천장 하나는 진단의 정규 표기(`` `버전 1.2` `` → `` `버전 1`.`2` ``)뿐이고 가리키는 심볼과 `fix` 의 CLI 주소는 정확하다. `check.rs` 의 `심볼_주소` 에 `ponytail:` 마커로 승급 조건을 남겼다
- [x] **CLI 주소에 코드 심볼 이름을 담을 수 있는지 판정** — `crate::mod::fn` 의 `::`, 경로 조각의 `/`, 제네릭의 `<>`. 담을 수 없으면 코드 참조는 **문서 심볼만** 가리키게 좁힌다(V0003 §4 의 예시가 이미 그렇다). 담아야 하면 이스케이프를 결정한다 — **`parse.rs:121-123` 의 `K105` 가 확립한 "CLI 로 주소를 댈 수 없는 이름은 이름을 고친다" 처방이 코드 심볼에는 통하지 않는다**(코드 이름을 kang 이 정할 수 없다)
  - **판정: 담을 수 없고 담을 필요도 없다.** V0003 §4 에 "코드 심볼 이름은 CLI 주소에 담지 않는다" 절을 신설해 명문화했다. 이스케이프 문법은 만들지 않는다
  - 담을 수 없다 — 이스케이프를 만들면 6.0 의 "구분자만으로 파싱한다" 를 폐기해야 하고, `K105`·`K115` 의 처방("이름을 고친다")은 kang 이 정하지 않는 이름에는 통하지 않는다
  - 담을 필요가 없다 — (1) rev 핀은 kang 심볼의 성질이므로 해제 대상은 코드 자리가 아니라 kang 심볼이고, 같은 심볼을 가리키는 자리를 전부 갱신하는 것은 v1 `bless` 가 이미 하는 일이다. (2) `inspect` 의 위치 보고는 `파일:줄` 이며 ADR-0003:28 이 `location` 을 명시적 예외로 두었다
  - V0003 §4:69 의 `kang bless <코드 위치>` 를 `kang bless <문서> --import <kang 심볼>` 로 정정했다 — 그 문장이 다음 사람을 코드 주소로 유인한다
- [x] 회귀: 오늘 통과하는 이름 전부가 여전히 통과 (`결제 v1.2 개요`·`` `C#` ``·`` `무료! 상품` ``·계층 keyword)
  - 출하 바이너리로 다섯 줄 전부 실측 exit 0 — `결제 v1.2 개요`(build·show) / `` `C#` ``(build·refs) / `` `무료! 상품` ``(build·bless 왕복) / `docs/B.결제수단.카드`(build·refs) / `v1.2/a#b/pay.kang`(build·list→show 왕복)
  - **기존 테스트 둘이 픽스처 때문에 깨졌고 둘 다 정당한 거부였다** — `## 결제의 방법 // 메모` 와 `## 참고 http://a.com 문서 // uncoded` 는 헤딩 **이름**에 `/` 가 든 경우다. 실측: 후자를 `bless` 로 가리키면 주소가 "문서 `docs/A#참고 http:/a` 의 keyword `com 문서`" 로 읽혀 **사용자가 쓰지 않은 심볼**에 대해 "import 대상 심볼이 없습니다" 가 나오고, 실제 topic 에 닿는 주소는 존재하지 않는다. 앞의 것은 K115 가 이름을 `// 메모` 까지 담아 보고하는지로 낱말 판정을 계속 재고, 뒤의 것은 같은 `split_modifier` 를 쓰는 한 줄 정의로 옮겼다
  - `cargo test` **309 → 314**, clippy·fmt exit 0

## Task 3 — 심볼 인덱스 산출

**B2 결정: `kang index` 별도 명령, 탭 구분 텍스트.**

**파일:** `src/index.rs` (신규), `src/main.rs`

```rust
/// 심볼 인덱스를 한 줄 하나씩 쓴다. 소비자가 의존성 없이 파싱할 수 있어야 한다.
/// 형식: {종류}\t{rev}\t{주소}\n
pub fn write_index(project: &Project, table: &SymbolTable, out: &mut impl Write) -> io::Result<()>;
```

- [x] `kang index <경로>` 명령 — error 가 있으면 쓰지 않고 exit 1 (스펙 6절 핵심 규칙)
  - `error_상태에서는_인덱스를_쓰지_않는다` 가 exit 1 · stdout 빔 · **파일 부재**를 단언
- [x] 주소는 **`ImportAddress::parse` 가 되받을 수 있는 형태**여야 한다 — 왕복 테스트로 못박는다
  - `인덱스가_낸_주소를_다른_명령이_받는다` 가 **모든** 주소를 종류별로 `refs`·`show`·`bless` 에 넣는다.
    exception 은 종료 코드로 못 가른다 — `bless` 가 "주소 형식 오류" 와 "그 import 없음" 에 같은 2 를 쓴다.
    메시지로 갈랐고 **그 사실은 Task 11 로 이월**한다
- [x] `--help` 에 추가하고 **양방향 게이트**를 통과시킨다
  - `index` 는 미구현 절에 없었으므로 명령 목록에만 한 줄 추가. `inspect` 만 남은 상태 유지
- [x] 원자적 쓰기 — `bless.rs` 의 임시 파일 + `rename` 규약을 재사용한다. **사본을 만들지 말고 공용 함수로 올린다**
  - `쓰기_원자적으로` 를 `pub` 으로 올리고 **임시 확장자를 인자로** 바꿨다 (`with_extension` 이 `.kang` 아닌 대상에서 이름을 뒤틀지 않게). `심볼_주소` 도 `pub` 으로 — 주소 조립 사본을 넷째로 만들지 않기 위해
- [x] **뮤테이션으로 원자성 테스트의 하중을 확인한다**
  - 첫 버전은 **공허했다.** `쓰기_원자적으로` → `fs::write` 로 바꿔도 통과했다(읽기 전용 디렉토리에선 둘 다 실패). **성공한 인덱스를 먼저 만들어 두고 실패한 재작성 뒤 바이트 불변을 단언**하도록 고쳤다 — 제자리 쓰기는 기존 파일을 덮어써 성공하므로 두 구현이 갈린다. 같은 뮤테이션 재실행 **FAILED**
- [x] `.gitignore` 결정 (J2)
  - **커밋한다.** gitignore 하면 2단 부트스트랩이 되어 Task 6 에서 기여자가 순서를 기억해야 한다.
    드리프트는 CI 가 `kang index` 재생성 + `git diff --exit-code` 로 막는다.
    결정론적 출력이 전제이므로 `write_index` 가 문서 경로로 정렬한다 — **두 번 돌려 SHA-256 동일 실측**.
    **게이트 자체는 Task 6 에서 붙인다** — 저장소에 실제 `.kang` 문서가 없어(Task 9) 지금 넣으면 아무것도 검사하지 않는 게이트가 되고 그것이 "덮였다" 로 읽힌다
- [x] **심볼 이름에 탭이 들어가면 형식이 깨진다** — 스펙 6.0 의 금지 문자에 탭을 넣을지 판정한다 (J1)
  - **새 금지를 만들지 않는다.** 실측: 탭·BEL 이 든 이름은 `build` 0 / `show` 0(YAML 파싱 OK) / `refs` 0 으로 **오늘 합법**이다. 금지하면 오늘 통과하는 입력을 거부하게 되고 그것이 이 저장소의 최악 실패 종류다.
    대신 **가변 길이 필드를 마지막에 둔다** — `splitn(3, '\t')` 가 이름 안의 탭을 그대로 살리고 파서는 여전히 3줄이다. 사용자 결정의 근거 셋(별도 명령·탭 구분·의존성 0 파서)이 전부 유지된다.
    미리보기의 `주소\t종류\trev` 에서 순서가 바뀐 것이 이 판정의 결과다

**수행 내역** — 컨트롤러가 직접 구현(서브에이전트가 API 529 로 두 번 중단). 테스트 314 → 320.
`exception` 의 rev 가 선언 topic 의 rev 와 같은 것을 실측 확인(스펙 4.8).
리포트: `.superpowers/sdd/V0004-kang-code-binding-and-dogfooding/task-3-report.md`

## Task 4 — 워크스페이스 분할

**B1 결정을 구현한다. 코드 이동만이며 동작 변경 0 이어야 한다.**

- [x] `Cargo.toml` 을 워크스페이스로, 기존 크레이트를 `crates/kang/` 으로 이동 (`git mv`)
  - `src`·`tests`·`Cargo.toml` 셋을 `git mv` 로 옮겼다. 루트 `Cargo.toml` 은 `[workspace]` 뿐이다
- [x] `crates/kang-macros/` 생성 — `proc-macro = true`
  - 의존성 `proc-macro2`·`quote`·`syn` 을 선언하고 모듈 주석만 두었다. 매크로 본체는 Task 5 다.
    **이 크레이트가 먼저 서는 이유는 워크스페이스가 의존성 목록과 함께 성립하는지 확인하는 것**이다
- [x] **`cargo test` 320개가 전부 그대로 통과해야 한다.** 하나라도 깨지면 이동이 아니라 변경이다
  - 320 통과 (check 121, cli 99, parse 83, yaml 14, hash 3). clippy `--all-targets -D warnings` 0, `fmt --check` 0
- [x] `release.yml`·`ci.yml`·`README.md` 의 경로 갱신
  - **바뀔 것이 거의 없었다.** 워크스페이스의 `target/` 이 루트에 그대로 있어 `release.yml:37` 의
    `mv target/<트리플>/release/kang` 이 유효하고, `ci.yml`·`README` 의 `cargo` 호출은 워크스페이스 루트에서 돈다.
    `cargo build --release --target <트리플>` 에 **`-p kang` 을 명시**했다 — 지금은 없어도 되지만
    Task 5 가 `kang` → `kang-macros` 의존을 만들면 무엇을 빌드하는지 분명해야 한다.
    크로스 빌드(`x86_64-apple-darwin`)를 실제로 돌려 바이너리 자리를 확인했다. `actionlint` 0
- [x] V0001 §10.1 의 크레이트별 목록과 `Cargo.toml` 들이 일치하는지 확인 — **목록에 없는 의존성이 들어오면 그 자체가 결함이다.** CI 에서 잴 방법을 정한다
  - 두 `Cargo.toml` 이 §10.1 과 일치한다. **CI 에서 재는 방법은 Task 5 로 넘긴다** —
    지금은 매크로가 의존성을 쓰지 않아 `cargo tree` 기반 검사가 무엇을 잡는지 판단할 근거가 없다

**수행 내역** — 컨트롤러가 직접 구현. 테스트 320 유지(변경 0). 루트에 있던 빈 쓰레기 디렉토리 `$SP/v2b` 도 함께 치웠다.

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

- [x] 인덱스 경로 결정 — 환경 변수 `KANG_INDEX` 인가 `build.rs` 가 넘기는가
  - **판정: 둘 다 아니라 `KANG_INDEX` 하나를 매크로가 읽고, 없으면 `CARGO_MANIFEST_DIR` 부터 위로 훑어 `.kang/index.tsv` 를 찾는다.** `build.rs` 는 값을 넘기지 않고 **추적만** 붙인다
  - 소비자의 `build.rs` 가 `cargo::rustc-env` 로 넘기는 안을 채택하지 않았다 — 소비자는 다른 프로젝트이고 kang 이 그들의 `build.rs` 를 쓸 수 없다. `.cargo/config.toml` 의 `[env]` 두 줄이 같은 일을 하며 코드가 0줄이다
  - 위로 훑는 이유: 매크로는 **워크스페이스 루트를 모른다**. `CARGO_MANIFEST_DIR` 은 크레이트 디렉토리이고 인덱스는 보통 워크스페이스 루트에 있다
- [x] **인덱스 부재 시 warn, `KANG_REQUIRE_INDEX=1` 이면 컴파일 에러** (B3 결정)
  - warn 은 `eprintln!` 이며 `OnceLock` 안에서 내므로 **속성 개수와 무관하게 한 번**이다. 안정판에 `proc_macro_diagnostic` 이 없어 rustc warning 으로는 낼 수 없다
  - `필수()` 는 `KANG_REQUIRE_YAML` 선례를 따라 **값이 아니라 존재**를 본다
  - **꼬리 문장이 모드마다 다르다.** warn 은 "이 빌드에서 kang 속성은 검증되지 않습니다", error 는 "`KANG_REQUIRE_INDEX` 가 켜져 있어 … 컴파일 에러입니다" — 에러로 세운 빌드에 "통과합니다" 를 붙이면 진단이 거짓이 된다. 첫 구현이 그 거짓을 말했고 테스트로 잡았다
- [x] `build.rs` 의 `cargo:rerun-if-changed` 로 인덱스를 추적 — **문서를 바꾸면 재빌드되는 것을 실제로 확인한다**(수동으로 `touch` 하지 않고)
  - `crates/kang-macros/build.rs` 신규. 지시 셋: `rerun-if-changed=$KANG_INDEX`, `rerun-if-env-changed=KANG_INDEX`, `rerun-if-env-changed=KANG_REQUIRE_INDEX`
  - **셋 전부를 뮤테이션으로 확인했다.** 파일 추적 제거 → 인덱스가 바뀌어도 `Finished in 0.01s` 로 캐시 통과(테스트 FAILED). 환경 변수 추적 둘도 각각 제거하니 대응 단계가 FAILED
  - 손으로도 확인: `.kang` 문서 한 줄을 고치고 `kang index` 를 다시 돌리자 `cargo build` 가 재컴파일하며 exit 101
  - **ponytail 천장:** 관례 경로로 찾는 경우는 파일 추적이 붙지 않는다. 의존성의 build script 는 자기를 의존하는 크레이트의 manifest 디렉토리를 받지 못한다. `.cargo/config.toml` 의 `[env]` 로 `KANG_INDEX` 를 주면 붙는다 (Task 6 이 그 방법을 쓴다)
- [x] 심볼 부재 → 컴파일 에러. 메시지가 **스펙 5.1.1 의 세 요소**(무엇이 틀렸나 / 어디인가 / 어떻게 고치나)를 담는가. `kang bless` 를 짝지을 수 있는가
  - **`kang bless` 는 짝지을 수 없다.** 심볼이 없으면 붙일 핀 자체가 없다. 처방은 둘이고 둘 다 참이다 — `[shell] kang index '<절대 경로>'`(인덱스가 낡았을 때, 그대로 실행 exit 0 확인) / `[edit] 주소를 고치거나 심볼을 선언`(주소가 틀렸을 때)
  - 위치는 `syn::Error::new(주소.span(), …)` 로 **주소 리터럴에** 붙였다. rustc 가 `src/main.rs:17:15` 과 밑줄을 그린다 — 진단이 좌표를 직접 적지 않으므로 ADR-0003 을 지킨다
  - **`compile_error!` 대신 `syn::Error::to_compile_error()` 를 쓴 이유가 이것이다.** 전자는 호출 자리 전체에만 붙어 어느 속성인지 가리키지 못한다
  - **종류 어긋남을 갈라 냈다.** `#[kang::keyword("docs/pay#…")]` 처럼 구분자와 속성 이름이 어긋나면 "없습니다" 는 참이지만 쓸모가 없다. 주소만 맞는 줄을 한 번 더 찾아 `` `#[kang::topic("docs/pay#결제의 방법", rev = "75318c")]` 로 바꾸세요 `` 를 낸다 — 인덱스의 핀까지 넣은 완성 속성이다
- [x] rev 불일치 → 컴파일 에러 + `kang bless` 처방. **그 처방을 그대로 복사해 실행하면 실제로 낫는지 확인한다**(V0002 가 세운 fix 계약)
  - **`kang bless` 를 처방하지 않았다. 실측으로 낫지 않는다.** `kang bless docs/pay --import 'docs/pay#결제의 방법'` → **exit 2**, "그 문서에 이 import 가 없습니다", `src/main.rs` 무변경, 같은 에러 2건 잔존
  - 근거: `bless` 는 **문서의 import 줄**을 고치는 명령이고 코드 속성의 핀은 `.rs` 파일에 있다. V0003 §4 가 "코드 쪽 주소를 새로 만들지 않는다" 고 정했으므로 `bless` 에 코드 자리를 지목할 수단이 없다
  - 채택한 처방: `[edit] 이 속성의 rev = "36ff45" 을 rev = "75318c" 으로 바꾸세요`. **그대로 적용해 exit 0 을 실측했다** (아래 왕복)
  - 줄 번호를 쓰지 않고 심볼과 두 핀 값으로 자리를 지정하므로 ADR-0003 을 지킨다
- [x] 매크로가 원본 아이템을 그대로 반환하는지 — 확장 결과를 `cargo expand` 없이 확인할 방법을 정한다
  - **성공 경로는 `Ok(()) => item` 한 표현식이다.** 토큰이 뒤틀릴 수 있는 자리는 진단을 붙이는 경로 하나뿐이고, `진단을_붙여도_원본_아이템은_그대로다` 가 제네릭·`where`·`derive` 가 든 아이템으로 `ends_with(원본)` 을 단언한다
  - **함수·구조체·상수·`impl` 블록 넷**에 붙인 픽스처를 `cargo run` 으로 돌려 네 값(`7`·`derive(Clone)` 왕복·`9`·`11`)을 단언한다. 토큰이 갈리면 컴파일이 깨지거나 값이 달라진다
  - `cargo expand` 는 §10.1 밖이라 쓰지 않았다

## Task 6 — kang 자신의 코드에 매크로 적용

**사용자 목표 (b).** 자기 저장소가 첫 사용자다.

- [x] `crates/kang/src/*.rs` 의 주요 함수에 `#[kang::topic(...)]` 을 붙인다 — **먼저 Task 9 가 그 topic 들을 `.kang` 문서로 만들어야 한다.** 순서 의존이므로 Task 9 뒤에 온다
  - **속성 넷.** `ast.rs` 의 `SymbolKind` ← keyword `심볼`(정의가 "keyword·topic·exception 세 종류뿐" 이고 배리언트가 정확히 그 셋), `hash.rs` 의 `rev` ← keyword `rev 핀`(그 값을 계산하는 함수), `bless.rs` 의 `ImportAddress` ← ADR-0003(그 ADR 의 결정이 이 타입 자체다), `show.rs` 의 `show` ← ADR-0002(결정문이 "kang show 는 평탄화한다")
  - **안 붙인 자리 셋.** ADR-0001(묘비 없음)·`CONTEXT#Negative Boundaries`·`CONTEXT#무엇을 keyword 로 선언하나` 는 **부정 결정 또는 사람에게 주는 지침**이라 구현하는 코드가 없다. rename 원자성을 근거로 `check.rs` 에 ADR-0001 을 붙일 수는 있으나 그것은 ADR 의 *근거*이지 *결정*이 아니다 — 거짓 결합이므로 붙이지 않았다. **문서를 늘려 자리를 만들지도 않았다**
  - 의존성 판정: `crates/kang` 이 `kang-macros` 를 path 의존한다. **V0001 §10.1 에 "워크스페이스 내부 크레이트끼리의 path 의존은 외부 의존성이 아니다" 를 한 문장 추가**했다 — 공급망도 빌드 시간도 늘지 않는다
  - **`use kang_macros as kang;` 로 모듈 안에서만 별명을 준다.** 의존성을 `kang` 으로 개명하는 소비자 방식(Task 5 의 픽스처)은 이 크레이트 이름이 이미 `kang` 이라 쓸 수 없다 — 통합 테스트의 `use kang::ast::…` 가 두 크레이트 사이에서 갈린다. 별명 덕에 진단이 처방하는 `#[kang::…]` 문면이 이 저장소에서도 그대로 참이다
  - **`crates/kang/src` 의 로직은 한 줄도 바뀌지 않았다.** 속성과 `use` 만 얹었고 테스트 335 가 그대로 통과한다 (V0003 §5 의 "런타임 비용 0" 약속이 실측으로 지켜졌다)
- [x] 부트스트랩 순서를 `xtask` 나 `Makefile` 로 못박는다 (B3)
  - **`Makefile`.** `xtask` 는 크레이트가 하나 늘고 §10.1 의 목록이 셋으로 갈리는데, 여기서 할 일은 셸 두 줄이다
  - `index` 는 `KANG_INDEX=/kang-bootstrap-no-index` 로 돌린다. **인덱스가 낡으면 매크로가 컴파일을 세우므로 인덱스를 낼 바이너리를 만들 수 없고, B3 의 "부재 시 warn" 이 그 순환을 끊는다.** `.cargo/config.toml` 의 `[env]` 는 이미 환경에 있는 값을 덮지 않아(`force` 없음) 이 지정이 이긴다
  - **`.cargo/config.toml` 의 `[env] KANG_INDEX` 가 필수다** — 관례 경로(위로 훑기)에는 재빌드 추적이 붙지 않으므로, 없으면 문서를 고쳐도 `cargo build` 가 캐시로 통과한다 (Task 5 의 뮤테이션 실측)
- [x] **문서를 고치면 컴파일이 깨지고, `bless` 후 통과하는 것을 손으로 왕복한다**
  - ADR-0002 본문의 한 낱말(`커진다`→`불어난다`)을 고쳐 왕복했다. `make index-check` exit 2(드리프트) → `cargo build` **exit 101** + `[edit] 이 속성의 rev = "622fe2" 을 rev = "83c764" 으로 바꾸세요` → 처방 그대로 적용 → exit 0
  - **되돌리는 방향도 같은 왕복이다.** 문서를 되돌리면 인덱스도 되돌아가고 소스의 핀이 낡아 exit 101 이 다시 난다. 처방을 적용해 exit 0. 저장소는 커밋 시점과 같은 바이트로 돌아왔다
  - **`bless` 가 아니라 `[edit]` 이다** — Task 5 가 실측으로 확인했다. `bless` 는 문서의 import 줄을 고치는 명령이고 코드 속성의 핀은 `.rs` 에 있다
  - **`bless` 를 쓰지 않는 왕복에서 드러난 사실 하나:** 문서를 고쳐도 `kang index` 를 다시 돌리지 않으면 `cargo build` 가 통과한다(추적 대상이 인덱스 파일이므로). CI 의 드리프트 게이트가 그 창을 닫는 유일한 장치다
  - 인덱스를 치우면 warn + exit 0, `KANG_REQUIRE_INDEX=1` 이면 **exit 101** (속성 넷 각각에 진단)
- [x] CI 에서 `KANG_REQUIRE_INDEX=1` 로 돌린다
  - `ci.yml` 에 스텝 하나(`make index-check`)와 `env` 셋을 더했다. `actionlint` exit 0
  - **`KANG_REQUIRE_INDEX` 를 job 전역으로 두면 `make index` 부트스트랩이 스스로 막힌다** — 실측 `exit 2`. 그래서 스텝마다 준다
  - **드리프트 게이트가 이빨을 갖는다** — 문서를 고치고 인덱스를 갱신하지 않은 상태에서 `make index-check` 가 exit 2 와 `-topic 622fe2 … / +topic 83c764 …` 를 냈다
  - 게이트를 cargo 스텝보다 **앞에** 둔 이유: 어긋난 채로 오면 cargo 도 빨개지지만 그 진단은 "인덱스를 다시 내라" 를 한 단계 멀리서 말한다
  - **`release.yml` 에는 켜지 않았다.** B3 는 "CI 와 릴리즈에서 켠다" 였다 — 그 절반이 열려 있다. 근거: 인덱스가 커밋되고 `ci.yml` 이 드리프트를 막으므로 태그가 가리키는 커밋은 이미 드리프트 없음이고, remote 가 없어 릴리즈 워크플로를 실제로 돌려 확인할 수 없다. **판단이지 완료가 아니므로 열린 항목으로 남긴다**

**수행 내역** — 테스트 **335 유지**(변경 0), clippy `--all-targets -D warnings` 0, `fmt --check` 0, `actionlint` 0, `kang build` exit 0.
커밋 `baa8fd5`. 리포트: `.superpowers/sdd/V0004-kang-code-binding-and-dogfooding/task-6-report.md`

## Task 7 — TypeScript 타입 생성

**V0003 §5.** 데코레이터는 런타임 구성물이라 `tsc` 가 검증하지 못하므로 타입 시스템으로 민다.

```typescript
// .kang/generated.ts — kang 이 생성한다. 손으로 고치지 않는다.
export interface KangTopics {
  "docs/A#결제의 방법": "a3f9c1";
}
declare function kangTopic<K extends keyof KangTopics>(topic: K, rev: KangTopics[K]): MethodDecorator;
```

- [x] `kang index --ts <경로>` 인가 별도 명령인가 결정
  - **별도 명령 `kang types <경로>`.** Task 3 의 근거는 "`build` 는 파일을 쓰지 않는다" 이므로 경계는 **읽기/쓰기**이고 두 안 모두 그 경계의 옳은 쪽에 있다 — 그 선례는 둘을 가르지 못한다. 가른 것은 **플래그 가드**다. `main.rs` 의 `제자리_플래그` 는 "플래그처럼 보이는 것을 위치 인자로 삼켜 `--help` 라는 이름의 파일을 만든" 실제 버그의 산물이고, `--ts` 를 넣으면 그 규칙이 셋으로 는다. `types` 는 그 가드를 **한 줄도 건드리지 않는다**
  - "같은 원천" 은 명령 이름이 아니라 코드가 보장한다 — `index.rs` 의 `순회` 하나를 `write_index`·`write_types` 가 공유하고, 뮤테이션(타입 핀만 5자리로)이 `타입의_핀이_인덱스의_핀과_같다` 를 깼다
  - 쓰기 경로(compile → error면 안 씀 → mkdir → 원자적 쓰기)는 `main.rs` 의 `산출` 하나로 합쳤다. `인덱스` 를 복제하면 원자성 규약이 두 벌이 된다
- [x] 존재하지 않는 topic → `keyof` 제약 위반으로 **타입 에러**인 것을 실제 `tsc` 로 확인
  - tsc 5.9.3 실측: `error TS2345: Argument of type '"docs/없는문서#없는 정책"' is not assignable to parameter of type 'keyof KangTopics'.` exit 2
- [x] 낡은 rev → 리터럴 타입 불일치로 **타입 에러**인 것을 확인
  - `error TS2345: Argument of type '"000000"' is not assignable to parameter of type '"622fe2"'.` exit 2
- [x] **한글 심볼 이름이 TS 식별자·리터럴에서 문제 없는지** 확인 — 문자열 리터럴 키라 괜찮을 것이나 실측한다
  - 문제 없다. 저장소 자신의 문서로 낸 타입(한글·공백·`#`·`/`·혼용 스크립트)이 `tsc --strict` exit 0 이고, 데코레이터를 붙인 코드가 **실제로 실행**된다(`node out/run.js` → `실행됨`)
  - **큰따옴표와 역슬래시는 이스케이프해야 한다** — 오늘 topic 이름에 합법이고(스펙 6.0 이 금지하지 않는다) 벗기면 `tsc` 가 `TS1131`·`TS1005` 로 깨진다. 탭·U+2028·U+2029 도 `\uXXXX` 로 낸다. `yaml::scalar` 를 재사용하지 않았다 — YAML 큰따옴표 스칼라에는 JS 에 없는 이스케이프(`\a`·`\N`·`\L`)가 있어 그쪽 규칙이 하나 늘면 여기서 조용히 문법 오류가 난다
- [x] 생성 파일의 헤더에 "생성물이며 손으로 고치지 않는다" 를 명시
  - `// kang 이 생성한 파일입니다. 손으로 고치지 않습니다 — 다음 \`kang types\` 가 덮어씁니다.`
- [x] **topic 만 낸다** (범위 판정)
  - 데코레이터가 붙는 자리는 클래스·메소드·접근자·프로퍼티이고(V0003 §3) 메소드가 구현하는 것은 정책이다. keyword 는 용어 정의, exception 은 정책의 구멍이므로 코드가 "구현" 하는 대상이 아니다. V0003 §5 가 적은 선언도 `KangTopics` 하나다. 그 둘을 가리키는 코드는 V0003 §3 의 주석 폴백과 `kang inspect --ci` 가 담당하며 `write_index` 가 이미 세 종류를 전부 낸다
- [x] **`declare` 가 아니라 본문 있는 함수로 낸다** (V0003 §5 스니펫 정정)
  - V0003 §5 는 `declare function kangTopic` 이라 적었으나, `export interface` 가 있는 파일은 모듈이므로 `declare` 는 (a) `export` 없이는 import 조차 되지 않고 (b) `export declare` 로 해도 컴파일된 JS 에 그 export 가 없어 `undefined` 를 받고 첫 데코레이터 적용에서 터진다. **존재하지 않는 것을 존재한다고 선언하는 파일을 kang 이 만들면 안 된다** — 진단이 참만 말해야 하는 것과 같은 규칙이다. `export function ... { return () => {}; }` 로 낸다
- [x] **`experimentalDecorators` 가 필요하다** (Task 8 의 제약으로 넘긴다)
  - V0003 §5 의 `MethodDecorator` 는 레거시 데코레이터 타입이다. TS 5 의 기본인 표준(Stage 3) 데코레이터로는 `TS1241`(런타임이 2인자로 부르는데 데코레이터가 3인자를 기대한다)·`TS1270` 이 난다. `experimentalDecorators: true` 에서 타입 체크·컴파일·실행이 전부 통과한다. **생성 타입을 두 모드 겸용으로 늘리지 않았다** — V0003 §5 가 `MethodDecorator` 를 적었고, 바꾸는 것은 설계 원천의 결정이다
- [x] **저장소에 `.kang/generated.ts` 를 커밋하지 않는다**
  - 이 저장소에 TS 코드가 없다. 소비자 없는 생성물은 썩는 것 말고 할 일이 없고, 드리프트 게이트를 붙여도 **아무것도 검사하지 않는 게이트**가 된다 — Task 3 J2 가 인덱스 CI 게이트를 Task 6 으로 미룬 것과 같은 이유다. 산출물과 그 게이트는 실제 소비자가 있는 `examples/ts-consumer`(Task 8)에 둔다. 결정론은 확인했다 — 두 번 돌려 SHA-256 동일

## Task 8 — 예시 TypeScript 구현체 + 문서 충실도 테스트

**사용자 목표 (c).** kang 의 실제 소비자는 **Rust 툴체인이 없는 프로젝트**다.

**파일:** `examples/ts-consumer/` (신규)

- [x] 최소 TS 프로젝트 — `.kang` 문서 2~3개 + 그 정책을 구현한 코드
  - 문서 2개(`docs/refunds.kang` 환불 가능 기간, `docs/proration.kang` 부분 환불의 계산), 코드 1개(`src/refund.ts` 메소드 둘에 핀 둘), 게이트 1개(`test.mjs`), `package.json`·`tsconfig.json`·`package-lock.json`·커밋된 `.kang/generated.ts`
  - **두 문서가 서로 import 하지 않는다.** import 주소는 루트 상대이고 이 문서들은 두 루트(예시 자신과 그것을 담은 kang 저장소)에 동시에 속하므로, `docs/glossary` 는 한쪽에서 맞고 다른 쪽에서 `K002` 다 — 실측했다. 각 문서가 자기 keyword 를 선언해 스스로 완결되게 두었고, 그래서 저장소 루트의 `kang build` 도 exit 0 이다
  - 같은 이유로 `npm test` 가 문서만 임시 git 저장소로 복사해 그 안에서 (1)(2)를 돈다. 그러지 않으면 생성 타입에 kang 자신의 topic 다섯이 섞인다 — 「저장소의 `.kang` 문서를 쓰지 않는다」를 어긴다
- [x] `kang` 바이너리를 **릴리즈 아티팩트로 받아** 쓰는 형태(소스 빌드 금지) — README 의 curl 경로가 실제로 동작하는지 이 예시가 검증한다
  - **예시는 kang 을 빌드하지 않는다** — `$KANG` 또는 PATH 에서 찾는다. `cargo` 를 부르는 자리가 예시 안에 하나도 없으므로 "소스 빌드 금지" 는 지켰다
  - **curl 경로는 검증하지 못했다.** remote 가 없어 릴리즈가 없다. 우회를 만들지 않고 그 사실을 적었다 — 이 항목은 「완료 조건」의 태그 푸시와 함께 remote 가 붙은 뒤로 남는다. 로컬 빌드 바이너리를 `$KANG` 으로 넘겨 돌렸고 CI 도 그렇게 한다
- [x] `npm test` 가 (1) `kang build` (2) 타입 생성 (3) `tsc --noEmit` (4) 문서 충실도 검사를 순서대로 돈다
  - `noEmit` 은 `tsconfig.json` 에 두고 `tsc -p .` 로 부른다. 플래그를 양쪽에 두면 어느 쪽이 이기는지 읽는 사람이 확인해야 한다
  - `experimentalDecorators: true` (Task 7 우려 1), `typescript` 5.9.3 을 devDependency 로 박음 (우려 3)
- [x] **문서 충실도 검사** — `kang inspect` 가 v2 이므로 이 태스크에서 만들 범위를 정한다. 최소선: 코드가 참조하는 topic 이 전부 실재하고 rev 가 일치하는가. `inspect` 본체(죽은 정책 판정·참조 전파)는 별도 태스크로 가른다
  - **최소선 위에 더한 것은 하나다 — (3)이 공허하지 않았는지 본다.** (4)는 코드의 `(topic, rev)` 짝을 생성 타입과 tsc 없이 대조한다. 최소선과 같은 것을 보지만 경로가 다르다: tsc 가 생성 타입을 프로그램에 넣지 못하면(코드가 import 를 잃거나 `include` 가 빗나가면) 애노테이션은 아무 검사도 받지 않고 통과하는데, **(3)만으로는 그 사실이 보이지 않는다.** 이 저장소가 공허한 게이트에 두 번 물렸다(Task 3 J2, Task 7 우려 2)
  - 실측으로 하중을 확인했다 — `kangTopic` 을 지역 선언으로 바꿔 tsc 를 공허하게 만들고 핀을 낡게 두면 (3)은 통과하고 **(4)가 잡는다.** 핀을 전부 지우면 (4)가 "검사한 것이 없습니다" 로 잡는다
  - **더하지 않은 것:** 죽은 정책 판정(문서의 topic 을 아무 코드도 구현하지 않음)은 `inspect` 본체이고 NOT in scope 다. 주석 폴백, `exception` 미구현, `uncoded` 상태 기계도 같다. **없는 검사에 이름을 붙이지 않았다**
- [x] **정책 문서를 고치면 `npm test` 가 실패하는 것을 실제로 확인한다.** 이것이 이 태스크의 유일한 성공 기준이다
  - 「30일」→「14일」한 낱말: `src/refund.ts(24,39): error TS2345: Argument of type '"cdd44d"' is not assignable to parameter of type '"fb0ff7"'.` exit 2. **진단이 새 rev 를 그 자리에서 말한다**
  - 그 rev 로 코드의 핀을 고치면 exit 0. 문서와 핀을 되돌려도 exit 0. **문서를 고치지 않으면 실패하지 않는다 — 거짓 양성 없음**
  - 없는 topic 을 가리키는 코드: `TS2345 ... is not assignable to parameter of type 'keyof KangTopics'` exit 2

**수행 내역** — `examples/ts-consumer/` 신설. `npm test` 왕복 8항목 실측(통과 → 문서 수정 실패 → 핀 갱신 통과 → 되돌림 통과 → 없는 topic 실패 → 하중 둘 → 거짓 양성 없음).
`cargo test` **340 passed**, clippy `--all-targets -D warnings` 0, `fmt --check` 0, `make index-check` exit 0, 저장소 루트 `kang build` exit 0.
예시 문서가 저장소 코퍼스에도 속하므로 `.kang/index.tsv` 에 심볼 5개가 늘었다 — **rev 는 두 루트에서 같고 주소만 다르다**(`cdd44d`·`970bc8`).
CI 에 붙였다(`cargo build` → `npm ci` → `npm test`, setup-node 없음). `node_modules` 를 최상위 `.gitignore` 에 넣었다 — `/` 를 붙이지 않으면 kang 의 순회도 같은 줄로 그 디렉토리를 건너뛴다.
리포트: `.superpowers/sdd/V0004-kang-code-binding-and-dogfooding/task-8-report.md`

## Task 9 — 도그푸딩 이관

**사용자 목표 (a).** 최종 리뷰 우선순위 5이며 **Task 6 의 선행**이다.

- [x] 이관 대상과 순서 결정 — `plans/`·`docs/adr/`·`CONTEXT.md` 중 무엇을 먼저. **스펙 자신(`V0001`)을 `.kang` 으로 옮기는 것이 가장 강한 도그푸딩이지만 순환 위험이 있다** — 스펙이 깨지면 컴파일러의 진실 원천이 사라진다
  - **옮긴 것:** `CONTEXT.kang`(용어집, keyword 16 + topic 2, import 0) 과 ADR 셋(`docs/adr/000{1,2,3}-*.kang`, 각 topic 1, import 11). 용어집이 뿌리, ADR 이 소비자인 한 방향이다
  - **ADR 은 한 파일이 한 topic 이다.** Context/Decision/Consequences 를 각각 topic 으로 쪼개면 (1) 서로 독립이 아니라 한 논증이므로 스펙 4.5 의 "완결성을 갖는 독립 단위" 에 어긋나고 (2) 고정 절 이름이 세 파일에서 같아 `iknow` 삼각 12개가 필요해진다
  - **안 옮겼다: 스펙(`V0001`).** 순환 위험이 아니라 쓸모가 이유다. 617줄이 문법 표·코드 펜스·예시 이름이고 그 안의 백틱은 대부분 아직 존재하지 않는 예시 심볼이라 이관하면 전부 미해결 심볼이다. `kang show` 가 원문보다 나아지는 자리가 없다
  - **안 옮겼다: 플랜 셋.** `- [ ]` 체크리스트가 topic 밖 내용이라 `K112` 가 거부하고, TODOS→WIPS→DONES 는 kang 이 모델하지 않는 생애주기다
  - **안 옮겼다: `README.md`.** GitHub 에서 렌더되어야 하고 내용이 curl·셸 명령이다. `kang show` 로 읽을 대상이 아니다
  - **용어집의 마크다운 링크는 산문으로 강등됐다** — 본문에서 문서 전체를 가리키는 주소 문법이 kang 에 없다(스펙 3절이 그 사실을 스스로 적고 있다)
- [x] 루트의 추적되지 않는 `kang init` 산출물 넷 처리 결정 (`.claude/`·`AGENTS.md`·`CLAUDE.md`·`docs/example.kang`)
  - **추적한다: `.claude/skills/kang/SKILL.md`·`AGENTS.md`·`CLAUDE.md`.** 저장소의 에이전트 진입점이고, 추적하지 않으면 새 클론에서 사라진다
  - **지웠다: `docs/example.kang`.** 자기 `description` 이 "실제 정책으로 바꾸세요" 라고 말하는 자리표시자이고 이제 그 자리를 `CONTEXT` 와 ADR 이 채웠다. `kang list`·`keywords`·`index` 에 가짜 keyword 로 섞이던 것도 함께 사라졌다
- [x] **참조 병합 천장 재측정** — `check.rs` 의 마커가 담은 "충돌 0건" 은 **마크다운 코퍼스**에서 잰 값이다. 실제 `.kang` 코퍼스가 유일한 재측정 수단이고, 이번에는 **측정 스크립트를 저장소에 남긴다**(Task 12 가 안 남겨 재측정이 일회성으로 끝났다)
  - `scripts/measure-corpus.py` 가 M1·M2·M3 을 한 번에 내고 커밋 sha 를 함께 찍는다
  - `f8fb55e` 실측: 문서 4, 263줄, 백틱 조각 2개 이상 줄 30, **선언된 계층 keyword 0**, 근접 위험 0, **충돌 0**
  - **충돌 0의 근거가 첫 측정보다 강해졌다.** 합칠 이름이 스코프에 있어야 병합이 일어나는데 계층 keyword 가 하나도 없으므로 **구조적으로 발화할 수 없다.** 마커의 승급 조건을 "계층 keyword 를 선언한 코퍼스에서 충돌 1건" 으로 좁혔다
- [x] `K114`(topic 뒤 import) 의 실제 발화 빈도 측정 — 산문이 많은 코퍼스가 그 진단의 첫 실사용이다
  - **자연 발화 0.** 마크다운에는 `import` 줄이 없으므로 이관만으로는 절대 발화하지 않는다. 사람이 `import` 를 잘못 둘 때만 난다. 손으로 유발해 보니 진단과 fix 는 정확했다(오탐 아님)
  - **`K115` 는 133회 발화했다.** 규칙을 모른 채 옮긴 마크다운의 최대 진단이며 **전부 정당하다** — 산문의 백틱 대부분이 도메인 용어가 아니라 파일 경로다(`src/check.rs`·`plans/TODOS/V0001-...`). 이관 비용의 본체가 여기다
  - 파싱 단계 진단 173건(K104 3 / K105 4 / K108 22 / K111 2 / K112 9 / K115 133). 파싱 error 가 검사 단계를 막으므로 **둘째 물결(K001)이 보이지 않는다** — 상한은 코드 펜스 밖 백틱 조각 1615, 서로 다른 이름 683
- [x] 성능 실측 — 지금 모든 수치는 합성 픽스처 값이다
  - `build` 428ms / `show` 437ms / `index` 433ms
  - **전부 순회 비용이다.** `kang --help` 2.2ms, 같은 코퍼스를 곁가지 없는 저장소로 옮겨 `build` 하면 2.3ms. `resolve.rs` 의 `수집` 이 `.gitignore` 를 보지 않아 `target/`(79,384 파일 / 957MB)을 통째로 훑는다
  - 그 함수의 `ponytail:` 마커가 승급 조건을 "순회가 실제로 느려지면 `git ls-files`" 로 적어 두었고 **그 조건이 실측으로 충족됐다**(186배). 수정은 `crates/` 밖 태스크로 넘긴다

## Task 10 — show 스키마를 소비자 계약으로 정본화

**최종 리뷰 우선순위 3. Task 7·8 의 선행** — TS 클라이언트가 첫 줄에서 부딪히는 것이 포인터 재조립 규약이다.

- [x] keyword 포인터 비대칭을 스펙 6.4 에 명문화 — 포인터는 `{path}.{name}`, 전개 자리는 `name`/`path` 분리
  - 스펙 6.4 「출력 규칙」 뒤에 한 문단을 두었다. 종류마다 다르다는 사실, `{path}.{name}` 되맞춤, 비대칭이 의도인 이유(스키마 예시의 `name: 결제수단.카드` 와 갈라지지 않기 위해), `path` 가 없으면 4.4 `iknow` 로 합법인 동명 심볼과 구별할 수 없다는 것
  - **스키마 예시도 고쳤다.** 예시가 `keywords` 항목에서 `path` 를 빼고 있었으므로 **스펙만 읽고는 포인터를 되조립할 수 없었다.** 구현은 처음부터 냈다. 두 자리(최상위 `keywords`, `references.keywords`)에 `path` 를 넣고, 포인터 한 줄이 실제로 어떻게 보이는지 예시에 넣었다
  - **`show.rs` 의 rustdoc 은 고칠 것이 없었다** — 컨트롤러 지시의 전제가 낡았다. V0002 최종 리뷰 Minor 1 은 그 리뷰의 fix 단계에서 이미 닫혔고(`.superpowers/sdd/V0002-kang-v1-implementation/final-review-fix-report.md` 의 Minor 1 항목이 "코드는 그대로" 로 기록), 지금 `참조_묶음` 의 rustdoc 은 종류별 차이와 `{path}.{name}` 을 정확히 적고 있다. **남은 것은 스펙뿐이었고 그것이 이 항목의 실질이다**
- [x] **js-yaml·go-yaml 파싱 게이트** — 지금 검증은 PyYAML·Ruby Psych 경로만 돌았다. YAML 1.2 파서에서 `=`·`true` 의 해석이 다를 수 있다
  - **js-yaml 4.3.1**(CORE/DEFAULT/JSON 스키마)과 **go-yaml v3.0.1** 로 저장소 자신의 `show` 출력 6개 + 적대적 픽스처 1개를 실제 파싱했다. **전부 통과.** 두 파서에서 이름·경로·정의가 문자열로, `uncoded`·`pending` 이 bool 로 돌아온다
  - **1.1 과 1.2 는 실제로 갈린다** — `=`(1.1 은 문서 전체 거부 / 1.2 는 문자열), `no`·`on`·`off`·`yes`(1.1 은 bool / 1.2 는 문자열). **갈리는 모든 표기에서 `scalar` 가 이미 인용한다**(`타입으로_읽히는_이름은_인용된다` 가 `no` 를 못박고 있다). 즉 1.1 용 인용 규칙이 1.2 의 상위집합이므로 **코드 변경 0**
  - 게이트에 하중이 있다 — 인용을 벗기면 `description` 이 boolean·number 로 돌아오고, `카드: 결제` 는 문서 전체가 파싱되지 않는다
  - **의존성 판정: §10.1 의 대상이 아니다.** 그 절은 표 머리가 "크레이트 / 허용 의존성" 이며 워크스페이스 크레이트의 Cargo 의존성을 열거한다. npm devDependency 는 크레이트의 의존성이 아니다. 다만 **이 태스크는 어느 쪽으로도 늘리지 않았다** — 게이트를 스크래치패드에서 돌렸고 `crates/`·저장소 어디에도 파일을 남기지 않았다. 상시 게이트는 node 가 이미 있는 `examples/ts-consumer`(Task 8)의 `npm test` 에 둔다
- [x] 빈 절을 생략하는가 `[]` 로 내는가 확정 (V0002 M7 이월) — **실제 소비 코드(Task 8)를 보고 정한다**
  - **생략한다.** 구현이 이미 그렇게 하고 있었고(`show.rs` 의 모든 `seq` 호출이 `is_empty` 로 가려져 있어 `Emitter::seq` 의 `[]` 가지는 `show` 에서 도달 불가) 이제 스펙 6.4 가 규칙으로 적는다. 항상 있는 키는 `path` 하나다
  - **근거는 실제로 쓴 소비 코드다.** 소비자가 무는 비용은 `?? []` 한 번이었고, 실측한 7개 문서의 절 구성이 전부 달랐다(`topics` 만 / `referencingKeywords`+`topics` / `keywords`+`topics` / 넷) — 어차피 부재를 다뤄야 한다. 반대쪽 비용은 조회 한 번마다 절 여섯 개가 정보 없는 `[]` 로 나가는 것이고 **`show` 의 독자는 그 토큰을 매번 읽는 LLM** 이다. 스펙 6.4 가 이미 `covers` 에 대해 생략을 정해 두었으므로 규칙을 균일하게 만드는 쪽이 특례를 없앤다
  - **컨트롤러 지시 정정:** "그 소비 코드를 이 태스크가 만든다(Task 7)" 는 틀렸다. Task 7 의 산출물(`generated.ts`)은 `show` YAML 을 읽지 않고 심볼 순회에서 나온다. `show` 를 소비하는 것은 Task 8 이며 플랜 원문이 그렇게 적었다. 그래서 판정을 미루지 않고 **J2 게이트를 그 소비 코드로 써서** 닫았다

**수행 내역 (Task 10·7 함께)** — Task 10 이 Task 7 의 선행이고 둘이 같은 계약(포인터·스키마·핀)을 다루므로 한 태스크로 돌았다.
`cargo test` **336 → 340**, clippy `--all-targets -D warnings` 0, `fmt --check` 0, `kang build` exit 0.
`tsc` 5.9.3 로 셋을 잡았고(없는 topic·낡은 rev·이스케이프 벗김), js-yaml 4.3.1·go-yaml v3.0.1 로 `show` 출력 7개를 파싱했다.
리포트: `.superpowers/sdd/V0004-kang-code-binding-and-dogfooding/task-10-7-report.md`

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
- [x] **`examples/ts-consumer` 의 `npm test` 가 통과하고, 정책 문서를 고치면 실패한다** (Task 8. `kang` 획득은 로컬 빌드 바이너리 — 릴리즈 curl 경로는 remote 가 붙은 뒤로 남는다)
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
