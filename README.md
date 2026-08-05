# kang

kang 은 문서 컴파일러다. `.kang` 문서에서 키워드를 선언하고 다른 문서의 키워드를
`import` 로 당겨 쓰면, `kang build` 가 미선언 참조·중복 정의·참조한 원문이 바뀐 자리를
컴파일 에러로 낸다. 한 개념의 주인은 정확히 한 파일이며, 참조는 항상 명시적이다.
소비자는 사람이 아니라 다른 프로젝트의 LLM 에이전트다 — 에이전트는 원본을 훑지 않고
`kang show` 로 필요한 문서·토픽만 YAML 로 받아 간다. 프로젝트 루트는 git 저장소
루트이고 설정 파일은 없다.

## 설치

GitHub Releases 에서 미리 빌드된 바이너리를 받는다. `OWNER` 는 이 저장소의 GitHub
소유자로 바꾼다.

```sh
curl -fsSL "https://github.com/OWNER/kang/releases/latest/download/kang-$(uname -m | sed s/arm64/aarch64/)-$(uname -s | sed 's/Darwin/apple-darwin/;s/Linux/unknown-linux-gnu/')" -o kang && chmod +x kang
```

올라가는 이름은 `kang-<타깃 트리플>` 이고 트리플은 `aarch64-apple-darwin`,
`x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` 넷이다.
Windows 빌드는 없다.

Rust 툴체인이 있으면 소스에서 빌드해도 된다.

```sh
cargo build --release
```

## 명령

```
kang — 문서 컴파일러

명령:
  kang init                            에이전트 진입점과 첫 문서 생성
  kang build                           컴파일 및 검증
  kang bless <문서> --import <심볼>    rev 핀 갱신·삽입
  kang list [경로]                     문서 목록과 description
  kang keywords [경로]                 키워드 목록
  kang refs <키워드>                   키워드를 참조하는 topic
  kang show <문서|토픽>                문서/토픽 조회 (YAML)
  kang --help                          이 도움말

아직 구현되지 않은 명령 (부르면 종료 코드 3 이며, 다른 방법이 없습니다):
  kang inspect                         코드 대조 (v2)

인자 문법:
  인자에 백틱을 쓰지 않습니다. 경로는 / , 키워드는 . , topic 은 # , exception 은 ! 로 잇습니다.
  공백이 있는 이름은 셸 인용이 필요합니다.

  kang refs docs/A.결제
  kang show 'docs/A#결제의 방법'
  kang bless docs/B --import 'docs/A.결제 수단'

종료 코드:
  0  성공
  1  컴파일 error 존재
  2  사용법 오류, 또는 환경 오류 (git 저장소가 아님)
  3  아직 구현되지 않은 기능 (kang inspect)
```

## 시작하기

빈 디렉토리에서 세 명령이면 끝난다. `kang init` 은 git 저장소를 요구하지 않지만
`kang build` 는 프로젝트 루트를 git 저장소 루트로 삼으므로 `git init` 이 먼저다.

```sh
git init
kang init
kang build
```

`kang init` 은 네 파일을 만든다 — `.claude/skills/kang/SKILL.md`(에이전트가 읽는
사용법의 유일한 사본), `AGENTS.md`·`CLAUDE.md`(그것을 가리키는 진입점),
`docs/example.kang`(첫 문서 템플릿). 기존 `AGENTS.md`·`CLAUDE.md` 는 덮어쓰지 않고
섹션만 덧붙이며, 이미 있으면 건너뛴다.

## 스펙

언어 명세와 CLI 계약은 [`plans/TODOS/V0001-kang-language-design.md`](plans/TODOS/V0001-kang-language-design.md) 에 있다.
