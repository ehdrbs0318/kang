# CHANGELOG

이 파일은 kang 의 릴리즈 기록이다. 버전의 기준은 `crates/kang/Cargo.toml` 이고, 이 파일의
버전 번호는 그 파일을 따른다. 형식은 [Keep a Changelog](https://keepachangelog.com/ko/1.1.0/),
버전은 [Semantic Versioning](https://semver.org/lang/ko/) 을 따른다.

## [Unreleased]

## [0.2.0] - 2026-11-10

### Added

- `kang build` 가 성공하면 저장소 루트에 `.kang/graph.kang.json` 을 쓴다. 문서 목록, keyword 정의,
  import 관계가 들어 있다. 컴파일 error 가 있으면 쓰지 않는다. kang 이 만들지 않은 파일은
  덮어쓰지 않고 종료 코드 2 로 끝난다.

### Changed

- `kang init` 이 낡은 `.claude/skills/kang/SKILL.md` 를 바이너리의 내용으로 바꾼다. 내용이
  같으면 건너뛴다. 전에는 파일이 있으면 건너뛰어서, 새 바이너리를 받아도 스킬이 옛 판으로
  남았다. `AGENTS.md`, `CLAUDE.md` 는 전과 같이 덮어쓰지 않는다.
- 스킬이 문서를 항상 `.kang` 으로 쓰게 한다. 기존 마크다운 문서를 옮기는 규칙, 파일 문법,
  keyword 로 선언할 용어의 기준, import 순서, 진단의 fix 를 적용하는 순서를 더했다.
- `kang init` 의 `AGENTS.md`, `CLAUDE.md` 진입점 문구와 `--help` 문구를 쉬운 말로 고쳤다.

## [0.1.1] - 2026-08-09

### Added

- `kang --version`: 버전을 출력한다. 릴리즈 바이너리를 curl 로 받은 사용자는 이 명령으로만
  자신이 쓰는 계약의 버전을 알 수 있다. 저장소 밖에서도 동작한다. 방금 받은 바이너리는
  보통 kang 프로젝트 밖에서 실행하기 때문이다.

  0.1.0 에는 이 명령이 없었다. 아래 「소비자 계약」에는 0.x 동안 계약이 바뀔 수 있다고
  적었지만, 0.1.0 에는 지금 어느 계약을 쓰는지 확인할 방법이 없었다.

  출력 형식은 `kang <버전>` 한 줄이다. 사용자가 이 출력을 파싱하므로, 이 형식도 아래 계약에
  포함된다.

## [0.1.0] - 2026-08-09

첫 릴리즈. 문서 컴파일러 kang 의 v1 기능 전체다.

### Added

**언어와 파서**

- `.kang` 문서에서 `keyword` 로 개념을 선언하고 `topic` 으로 문단을 묶는다. 문서 단위의
  메타데이터는 frontmatter 에 적는다.
- 다른 문서의 개념은 `import` 로 가져와 쓴다. 주소는 프로젝트 루트 기준이다. 그래서 어느
  문서에서 읽어도 같은 대상을 가리킨다.
- `exception`, `cover`, 수식어(modifier)로 개념 사이의 관계를 적는다.
- kang 은 keyword 정의 안의 백틱을 참조로 모은다. 개념을 언급하면 그 언급이 곧 참조다.
- 계층 keyword: `결제.환불` 처럼 점으로 계층을 만든다. 상위 keyword 는 같은 파일에 있어야 한다.

**검사**

- 선언하지 않은 참조, 중복 정의, 해결되지 않은 import 를 컴파일 에러로 알린다.
- import 그래프의 순환을 찾는다.
- rev 핀: 참조한 원문의 해시를 문서에 적어 둔다. 원문이 바뀌면 빌드가 그 사실을 알린다.
  정규화 규칙이 공백과 줄바꿈 차이를 무시한다.
- exception 상태 기계: 예외를 선언하고, 해소하고, 다시 선언하는 순서가 올바른지 검사한다.
- kang 은 topic 밖의 내용을 `K112` 로 거부한다. 모든 본문은 어떤 topic 안에 있어야 한다.
- 진단은 스펙 5.1.1 형식을 따른다. 진단에는 코드, 위치, 원인, 실행할 수 있는 `fix` 줄이 있다.

**CLI**

- `kang build`: 프로젝트 전체를 검사한다. 문서 4개, 263줄에서 3ms 걸린다.
- `kang show`: 문서나 토픽을 YAML 로 출력한다. LLM 에이전트는 원본을 읽지 않고 이 명령으로
  필요한 내용만 받는다.
- `kang list`, `kang keywords`, `kang refs`: 프로젝트의 문서, 개념, 참조 목록을 보여 준다.
- `kang bless`: 원문이 의도대로 바뀌었을 때 rev 핀을 갱신한다.
- `kang init`: 프로젝트를 시작하고 에이전트용 스킬 파일을 만든다.
- `kang index`: proc-macro 와 TypeScript 가 읽는 심볼 인덱스를 `{종류}\t{rev}\t{주소}`
  형식의 TSV 로 출력한다. 길이가 일정하지 않은 주소를 마지막 열에 둔다. 그래서 이름에 탭이
  있어도 읽는 쪽이 열을 잘못 나누지 않는다.
- `kang types`: topic 을 TypeScript 리터럴 타입으로 출력한다.

**코드와 문서 연결**

- `kang-macros`: Rust 코드에 `#[kang::topic("docs/A#…", rev = "a3f9c1")]` 를 붙이면,
  빌드할 때 매크로가 인덱스를 읽는다. 그 topic 이 있는지, rev 가 맞는지 검사한다.
  `keyword`, `covers` 도 같은 형태다.
- TypeScript 에서는 `kangTopic(topic, rev)` 와 `KangTopics` 의 `keyof` 제약으로 같은 검사를 한다.
- kang 자신의 코드에도 이 매크로가 붙어 있다. kang 컴파일러의 코드가 kang 의 문서와 연결되어 있다.

**인프라**

- CI 는 PR 과 `main` 푸시마다 `make index-check`, `cargo test`, `clippy`, `fmt`,
  `examples/ts-consumer` 의 `npm test` 를 실행한다. `npm test` 는 `kang types` 의 출력을
  실제로 `tsc` 로 컴파일하는 상시 검사다.
- `v*` 태그를 푸시하면 macOS 와 Linux 의 두 아키텍처씩, 모두 4개 타깃의 바이너리가 GitHub
  Releases 에 올라간다. 이 과정은 실제로 확인했다. 테스트용 태그로 워크플로를 실행해서 다음을
  확인했다.
  - 4개 타깃의 빌드가 모두 성공했다.
  - 아티팩트 이름이 README 의 curl 명령이 만드는 네 조합과 같다.
  - 그 curl 명령을 그대로 실행해서 바이너리를 받았고, `--help` 가 동작했다.
- `scripts/measure-corpus.py`: 참조 병합의 상한을 실제 문서 모음에서 측정하는 스크립트.

### 소비자 계약

다른 프로젝트는 아래 네 가지에 의존한다. 0.x 동안 바뀔 수 있다. 바뀌면 이 파일에 적는다.

- **`kang show` 의 YAML 스키마**: 에이전트가 파싱하는 형식.
- **종료 코드**: `0` 성공, `1` 컴파일 error 존재, `2` 사용법 오류 또는 환경 오류(git
  저장소가 아님, 문서를 읽지 못함), `3` 아직 구현되지 않은 기능. 에이전트는 이 코드로 다음
  행동을 정한다. `1` 이면 문서를 고친다. `2` 면 명령이나 환경을 고친다.
- **진단 형식**: 코드(`K0xx`/`K1xx`), 위치, 원인, `fix`.
- **심볼 인덱스 TSV**: `{종류}\t{rev}\t{주소}`.

`kang inspect` 는 v2 기능이라 아직 없다. 실행하면 종료 코드 3 으로 끝나고, 아직 구현되지
않았다고 알린다. 없는 기능을 있는 것처럼 동작하지 않는다.

[0.1.1]: https://github.com/ehdrbs0318/kang/releases/tag/v0.1.1
[0.1.0]: https://github.com/ehdrbs0318/kang/releases/tag/v0.1.0
