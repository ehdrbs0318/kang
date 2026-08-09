# CHANGELOG

이 파일은 kang 의 릴리즈 기록이다. 버전의 진실 원천은 `crates/kang/Cargo.toml` 이고,
여기 적힌 번호는 그것을 따른다. 형식은 [Keep a Changelog](https://keepachangelog.com/ko/1.1.0/),
버전은 [Semantic Versioning](https://semver.org/lang/ko/) 을 따른다.

## [0.1.1] - 2026-08-09

### Added

- `kang --version` — 버전을 낸다. 릴리즈 바이너리를 curl 로 받은 소비자에게는 이것이
  자기가 쥔 계약이 무엇인지 아는 유일한 통로다. 저장소 바깥에서도 답한다 — 갓 받은
  바이너리를 치는 곳은 kang 프로젝트 안이 아니기 때문이다.

  0.1.0 이 이것 없이 나갔다. 아래 「소비자 계약」은 0.x 동안 계약이 바뀔 수 있다고
  적었는데, 정작 어느 계약인지 물을 방법이 그 릴리즈에 없었다.

  출력 형식 `kang <버전>` 한 줄은 아래 계약 표면에 더해진다 — 소비자가 파싱한다.

## [0.1.0] - 2026-08-09

첫 릴리즈. 문서 컴파일러 kang 의 v1 전부다.

### Added

**언어와 파서**

- `.kang` 문서에서 `keyword` 로 개념을 선언하고 `topic` 으로 문단을 묶는다. frontmatter 로
  문서 수준 메타데이터를 적는다.
- 다른 문서의 개념을 `import` 로 당겨 쓴다. 주소는 프로젝트 루트 기준이라 어디서 읽어도
  같은 것을 가리킨다.
- `exception`·`cover`·수식어(modifier)로 개념 사이의 관계를 적는다.
- keyword 정의 안의 백틱은 참조로 수집된다. 개념을 언급하면 그것이 곧 참조다.
- 계층 keyword — `결제.환불` 처럼 점으로 계층을 만들면 상위가 같은 파일에 있어야 한다.

**검사**

- 미선언 참조, 중복 정의, 미해결 import 를 컴파일 에러로 낸다.
- import 그래프의 순환을 검출한다.
- rev 핀 — 참조한 원문의 해시를 문서에 적어 두고, 원문이 바뀌면 빌드가 그 사실을 말한다.
  정규화 규칙이 공백·줄바꿈 차이를 흡수한다.
- exception 상태 기계 — 예외가 선언·해소·재선언되는 순서가 유효한지 검사한다.
- topic 밖의 내용을 `K112` 로 거부한다. 모든 산문은 어떤 topic 에 속해야 한다.
- 진단은 스펙 5.1.1 형식으로 나온다 — 코드, 위치, 원인, 그리고 실행 가능한 `fix` 줄.

**CLI**

- `kang build` — 프로젝트 전체를 검사한다. 4문서 263줄에서 3ms.
- `kang show` — 문서·토픽을 YAML 로 낸다. 소비자인 LLM 에이전트가 원본을 훑지 않고
  필요한 것만 받아 가는 통로다.
- `kang list`·`kang keywords`·`kang refs` — 프로젝트의 문서·개념·참조를 훑는다.
- `kang bless` — 원문이 정당하게 바뀌었을 때 rev 핀을 갱신한다.
- `kang init` — 프로젝트를 시작하고 에이전트용 스킬 파일을 낸다.
- `kang index` — proc-macro 와 TypeScript 가 읽을 심볼 인덱스를 `{종류}\t{rev}\t{주소}`
  TSV 로 낸다. 가변 길이인 주소를 마지막에 둬서 이름에 탭이 들어가도 소비자가 오독하지 않는다.
- `kang types` — topic 을 TypeScript 리터럴 타입으로 낸다.

**코드 결속**

- `kang-macros` — Rust 코드에 `#[kang::topic("docs/A#…", rev = "a3f9c1")]` 을 붙이면
  빌드 타임에 인덱스를 읽어 그 topic 이 실재하고 rev 가 맞는지 검증한다. `keyword`·`covers` 도
  같은 형태다.
- TypeScript 쪽은 `kangTopic(topic, rev)` 와 `KangTopics` 의 `keyof` 제약으로 같은 것을 한다.
- kang 자신의 코드에 이 매크로가 붙어 있다. 컴파일러가 자기 문서에 묶여 있다.

**인프라**

- CI 가 PR 과 `main` 푸시에서 `make index-check`, `cargo test`, `clippy`, `fmt`,
  그리고 `examples/ts-consumer` 의 `npm test` 를 돌린다. TypeScript 소비자 예시가
  `kang types` 산출물을 실제로 `tsc` 에 태우는 상시 게이트다.
- `v*` 태그를 밀면 macOS·Linux 각 두 아키텍처, 총 4타깃 바이너리가 GitHub Releases 에
  올라간다. **실증했다** — 버림 태그로 워크플로를 돌려 4타깃 전부 초록, 아티팩트 이름이
  README 의 curl 이 만드는 네 조합과 일치, 그 curl 을 그대로 실행해 바이너리를 받고
  `--help` 가 도는 것까지 확인했다.
- `scripts/measure-corpus.py` — 참조 병합 천장을 실제 코퍼스에서 재는 스크립트.

### 소비자 계약

이 넷이 다른 프로젝트가 의존하는 표면이다. 0.x 동안 바뀔 수 있고, 바뀌면 여기 적는다.

- **`kang show` 의 YAML 스키마** — 에이전트가 파싱하는 형태.
- **종료 코드** — `0` 성공, `1` 컴파일 error 존재, `2` 사용법 오류 또는 환경 오류(git
  저장소가 아님, 문서를 읽지 못함), `3` 아직 구현되지 않은 기능. 에이전트는 이 코드로
  분기한다 — `1` 과 `2` 를 나눠야 "문서를 고쳐라" 와 "명령·환경이 잘못됐다" 가 갈린다.
- **진단 형식** — 코드(`K0xx`/`K1xx`), 위치, 원인, `fix`.
- **심볼 인덱스 TSV** — `{종류}\t{rev}\t{주소}`.

`kang inspect` 는 v2 기능이라 아직 없다. 부르면 exit 3 과 함께 그렇다고 말한다 —
없는 것을 있는 척하지 않는다.

[0.1.1]: https://github.com/ehdrbs0318/kang/releases/tag/v0.1.1
[0.1.0]: https://github.com/ehdrbs0318/kang/releases/tag/v0.1.0
