# kang

kang 은 문서 컴파일러다. `.kang` 문서에 키워드를 선언하고, 다른 문서의 키워드는
`import` 로 가져와 쓴다. `kang build` 는 다음 세 가지를 컴파일 에러로 알린다.

- 선언하지 않은 키워드를 참조한 곳
- 같은 키워드를 두 번 정의한 곳
- 참조한 원문이 바뀐 곳

한 개념은 정확히 한 파일이 정의한다. 다른 파일은 그 개념을 항상 `import` 로 명시해서
참조한다. kang 의 주 사용자는 사람이 아니라 다른 프로젝트의 LLM 에이전트다. 에이전트는
원본 파일을 읽지 않는다. `kang show` 로 필요한 문서나 토픽만 YAML 로 받는다.
프로젝트 루트는 git 저장소 루트이고, 설정 파일은 없다.

## 설치

GitHub Releases 에서 미리 빌드된 바이너리를 받는다.

```sh
curl -fsSL "https://github.com/ehdrbs0318/kang/releases/latest/download/kang-$(uname -m | sed s/arm64/aarch64/)-$(uname -s | sed 's/Darwin/apple-darwin/;s/Linux/unknown-linux-gnu/')" -o kang && chmod +x kang
```

바이너리 이름은 `kang-<타깃 트리플>` 이다. 타깃 트리플은 `aarch64-apple-darwin`,
`x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` 네 개다.
Windows 빌드는 없다.

Rust 툴체인이 있으면 소스에서 빌드할 수도 있다.

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
  kang index <경로>                    심볼 인덱스 생성 (탭 구분)
  kang types <경로>                    TypeScript 타입 생성 (topic)
  kang --help                          이 도움말
  kang --version                       버전

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
  2  사용법 오류, 또는 환경 오류 (git 저장소가 아님, 문서를 읽지 못함, 출력을 쓰지 못함)
  3  아직 구현되지 않은 기능 (kang inspect)
```

## 시작하기

빈 디렉토리에서 명령 세 개를 실행한다.

```sh
git init
kang init
kang build
```

`git init` 을 먼저 실행한다. `kang init` 은 git 저장소가 없어도 동작하지만, `kang build` 는
git 저장소 루트를 프로젝트 루트로 쓰기 때문이다.

`kang init` 은 파일 네 개를 만든다.

- `.claude/skills/kang/SKILL.md`: 에이전트가 읽는 사용법. 사용법은 이 파일에만 있다.
- `AGENTS.md`, `CLAUDE.md`: 위 스킬 파일을 가리키는 진입점.
- `docs/example.kang`: 첫 문서 템플릿.

`AGENTS.md` 나 `CLAUDE.md` 가 이미 있으면 kang 은 파일을 덮어쓰지 않고 kang 섹션만 끝에
더한다. kang 섹션이 이미 있으면 아무것도 하지 않는다.

## TypeScript 프로젝트에서 쓰기

`examples/ts-consumer/` 는 Rust 툴체인 없이 kang 을 쓰는 가장 작은 예제다.

1. `kang types` 가 문서의 topic 과 rev 핀을 TypeScript 리터럴 타입으로 출력한다.
2. 코드는 `@kangTopic("docs/refunds#환불 가능 기간", "cdd44d")` 처럼 자신이 구현한 정책을
   가리킨다.
3. 정책 원문이 바뀌면 rev 가 달라지고 `tsc` 컴파일이 실패한다. 진단에 새 rev 가 나온다.
4. 문서를 다시 읽고, 코드의 핀을 새 rev 로 고친다.

`npm install` 다음에 `npm test` 를 실행하면 `kang build`, `kang types`, `tsc`, 핀 대조를
차례로 실행한다. `kang` 바이너리는 PATH 나 `$KANG` 에서 찾는다. 위의 「설치」대로 받아 두면
된다. 예제가 데코레이터를 쓰므로 `tsconfig.json` 에 `experimentalDecorators` 가 필요하다.

## 스펙

언어 명세와 CLI 계약은 [`plans/TODOS/V0001-kang-language-design.md`](plans/TODOS/V0001-kang-language-design.md) 에 있다.
