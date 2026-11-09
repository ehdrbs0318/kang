# V0007 — kang 그래프 화면 개편

Orca kang 플러그인의 그래프 화면(`orca-plugin/src/graph/`)을 고친다. 사람이 문서 사이의 연결을 한눈에 알아보고, 화면 안에서 문서를 살펴보고 열 수 있게 한다.

**관련 문서:** `plans/DONES/V0005-orca-kang-plugin.md` (플러그인 설계), `plans/DONES/V0006-orca-kang-plugin-implementation.md` (첫 구현)

---

## 왜 고치는가

지금 그래프 화면은 점 이름에 문서의 전체 경로를 쓴다. `examples/ts-consumer/docs/proration` 처럼 이름이 길어서 이름끼리 겹친다. 어느 폴더의 문서인지 색으로 구분되지 않는다. 점을 누르면 바로 문서가 열리기 때문에, 그래프 안에서 문서의 설명이나 연결을 살펴볼 수 없다.

## GEPA 에서 가져온 것

[GEPA](https://github.com/gepa-ai/gepa) 의 후보 트리 화면(`src/gepa/visualization.py`)과 GEPA Workbench(`docs/docs/workbench.md`)를 참조했다.

| GEPA | kang 그래프 화면 |
| --- | --- |
| 상단 머리줄에 run 이름, 통계(Agg score, Calls), 범례(Best, Pareto Front, Other) | 상단 머리줄에 문서 수, 연결 수, keyword 수, 폴더 색 범례 |
| 왼쪽 Candidates 목록. 점수와 역할 표시(★ best, ◆ pareto) | 왼쪽 문서 목록. 폴더별로 묶고 들어오는 연결 수와 역할 표시(★ 허브, ○ 고립) |
| 점에 마우스를 올리면 후보 전문과 메타 정보를 보여 주는 툴팁 | 점에 마우스를 올리면 경로와 description 툴팁. 이웃 점과 연결만 밝게 남기고 나머지는 흐리게 |
| 점을 누르면 툴팁을 고정해서 스크롤할 수 있음. 바깥을 누르면 풀림 | 점을 누르면 오른쪽 상세 패널에 고정. 빈 곳을 누르면 풀림 |
| 역할 배지(BEST, PARETO, SEED)와 역할별 색 | 역할 배지(허브, 고립)와 허브 점의 테두리 |
| 상세 카드의 Open 버튼 | 상세 패널의 "문서 열기" 버튼. 점을 두 번 눌러도 열림 |

GEPA 의 점수, Pareto 프런트, 시간 그래프는 가져오지 않는다. kang 인덱스에는 그에 맞는 값이 없다.

## 목표

- 점 이름은 경로의 마지막 부분이다. `-` 와 `_` 는 공백으로 바꾸고, 점 아래에서 여러 줄로 감싼다. 48자보다 길면 줄이고 `…` 를 붙인다. 전체 경로는 툴팁과 상세 패널에서 본다.
- 점 색은 문서가 있는 폴더로 정한다. 같은 폴더의 문서는 같은 색이다. 머리줄에 폴더별 색 범례가 있다.
- 점 크기는 들어오는 연결 수로 정한다. 들어오는 연결이 가장 많은 문서가 허브다. 연결이 하나도 없는 문서는 고립이다.
- 화살표 굵기는 import 한 심볼 수로 정한다. 화살표에 마우스를 올리면 심볼 목록이 보인다.
- 왼쪽 목록이나 그래프에서 문서를 고르면 오른쪽 상세 패널에 다음이 보인다.
  - 이름, 전체 경로, 역할 배지, description
  - 이 문서가 import 하는 문서와 심볼
  - 이 문서를 import 하는 문서와 심볼
  - 이 문서가 선언한 keyword 와 정의
  - "문서 열기" 버튼
- 상세 패널의 다른 문서 이름을 누르면 그 문서를 고른다.
- 검색 칸에 글자를 넣으면 이름, 경로, description 에 그 글자가 있는 문서만 목록에 남고 그래프에서 강조된다.
- "전체 보기" 버튼으로 그래프 전체가 화면에 들어오게 한다.
- Orca 의 밝은 테마와 어두운 테마에서 모두 읽을 수 있다.

## 목표가 아닌 것

- 인덱스 형식(`graph/1`)이나 worker 의 요청 계약 바꾸기. 화면에 필요한 값은 지금 `kang.locate` 답에 모두 있다.
- 그래프 화면에서 문서 고치기.
- 레이아웃 고르기 기능. `cose` 하나만 쓴다.

## 실패로 보는 것

- 상세 패널이 다른 문서의 keyword 나 연결을 보여 준다.
- stale 안내가 사라진다.
- 문서 수가 늘어도 점 이름이 서로 겹쳐 읽을 수 없다.

---

## 인터페이스

`src/graph/model.ts` 는 DOM 을 쓰지 않는 순수 함수만 둔다.

- `shortLabel(path: string, max = 48): string`
- `folderOf(path: string): string`. 루트 문서는 `'/'` 다.
- `buildModel(docs: GraphDoc[], edges: GraphEdge[], definitions: Record<string, string>): GraphModel`
  - `GraphModel = { nodes: DocNode[]; edges: GraphEdge[]; folders: string[] }`
  - `DocNode = { path; label; folder; folderIndex; description; role: 'hub' | 'orphan' | 'normal'; incoming: Link[]; outgoing: Link[]; keywords: { name; definition }[] }`
  - `Link = { path: string; symbols: string[] }`
  - keyword 의 주인은 `definitions` 키 앞부분과 가장 길게 맞는 문서 경로다.
- `matches(node: DocNode, query: string): boolean`. 대소문자를 가리지 않는다.
- `toElements(model: GraphModel): ElementDefinition[]`

`src/graph/panel.ts` 는 왼쪽 목록, 상세 패널, 머리줄의 DOM 을 만든다. `src/graph/main.ts` 는 cytoscape 와 이벤트를 잇는다.

---

## 체크리스트

- [x] `model.ts` 의 함수와 테스트
  - 확인: `orca plugin test` 97개 통과. `shortLabel`, `folderOf`, `buildModel`, `matches`, `toElements` 테스트 7개를 새로 썼다.
- [x] `panel.ts` 와 `main.ts`, `style.css` 로 화면 구성
  - 확인: 머리줄에 문서 6, 연결 3, keyword 19 와 폴더 3개의 색 범례가 보인다. 루트 폴더는 "저장소 루트" 로 보인다.
- [x] 점 마우스 올림, 점 누름, 두 번 누름, 빈 곳 누름, 목록 누름, 검색, 전체 보기 동작
  - 확인: `orca plugin shot --editor ... --act` 로 실제 마우스와 키 입력을 보냈다.
  - 점 누름: 상세 패널에 CONTEXT 의 description, 가져가는 문서 3개와 심볼, keyword 16개가 보였다. 이웃이 아닌 고립 문서는 흐려졌다.
  - 목록 누름: refunds 를 고르자 상세 패널이 바뀌었다. 점이 이미 화면 안에 있어서 그래프는 움직이지 않았다.
  - 검색: `adr` 을 넣자 목록에 4개만 남고 고립 문서 2개가 흐려졌다.
  - Esc: 고르기가 풀렸다.
  - 두 번 누름: proration 점을 두 번 누르자 `proration.kang` 탭이 열렸다.
  - 문서 열기 버튼: `CONTEXT.kang` 탭이 열렸다.
  - 마우스가 점에서 그래프 영역 밖으로 나가면 tooltip 이 사라졌다.
- [x] `orca plugin test`, `tsc`, `orca plugin validate` 통과
- [x] 설치한 플러그인으로 Orca 창에서 화면과 동작을 눈으로 확인
  - 확인: 어두운 테마와 밝은 테마에서 모두 확인했다.
- [x] `orca-plugin/README.md` 의 그래프 설명 갱신
