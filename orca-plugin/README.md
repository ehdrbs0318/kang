# kang Orca 플러그인

이 플러그인은 kang 문서(`.kang`)와 그래프 인덱스(`.kang/graph.kang.json`)를 Orca 에서 읽기 전용으로 보여 준다. 플러그인 id 는 `kang` 이다.

## 구성

- `src/worker.ts`: 플러그인의 worker 다. custom editor 의 요청 두 개에 답한다.
  - `kang.locate`: 열린 파일의 원문으로 workspace 와 문서를 찾는다. 그 workspace 의 그래프, 정의, stale 문서 목록을 돌려준다.
  - `kang.open`: 인덱스에 있는 문서 `<path>.kang` 을 Orca 의 편집기 탭으로 연다.
- `src/locate.ts`: 문서 해시 계산과 workspace 찾기다. 해시 규칙은 `crates/kang/src/hash.rs` 와 같다.
- `src/protocol.ts`: worker 와 custom editor 가 함께 쓰는 요청 타입이다.
- `src/render/`: `.kang` 파일의 읽기 전용 렌더링 화면이다. Orca 가 `.kang` 파일을 이 custom editor 로 연다.
  - `parse.ts`: kang 문법을 marked 확장으로 그리고, 본문 참조를 `kang build` 가 쓴 scope 에서 찾는다.
  - `main.ts`: 본문을 그리고 `kang.locate` 답으로 참조에 링크를 단다.
  - `style.css`: Orca 테마의 `--orca-*` 변수로 색을 정한다.
- `src/graph/`: `.kang.json` 그래프 인덱스의 읽기 전용 그래프 화면이다. Orca 가 `.kang.json` 파일을 이 custom editor 로 연다.
  - `model.ts`: 인덱스를 화면 모델로 바꾼다. 문서마다 짧은 이름, 폴더, 역할(허브, 고립), 들어오고 나가는 연결, 선언한 keyword 를 모은다.
  - `panel.ts`: 머리줄(통계, 폴더 색 범례, 검색, 전체 보기), 왼쪽 문서 목록, 오른쪽 상세 패널을 만든다.
  - `main.ts`: `kang.locate` 답을 `cose` 레이아웃으로 그린다. 점에 마우스를 올리면 이웃만 밝게 남긴다. 점을 누르면 상세 패널에 고정하고, 두 번 누르면 그 문서를 연다. 화살표에 마우스를 올리면 심볼 목록을 보여 준다.
  - 화면 구성은 [GEPA](https://github.com/gepa-ai/gepa) Workbench 를 참조했다. 자세한 내용은 `plans/DONES/V0007-graph-view-redesign.md` 에 있다.
- `src/frame.ts`, `src/frame.css`: 두 화면이 함께 쓰는 스타일 넣기와 안내 문구다.

원격 workspace 는 지원하지 않는다. worker 는 로컬 파일만 읽는다.

## 개발

먼저 저장소 루트에서 `kang build` 를 실행해 그래프 인덱스를 만든다. 다음 명령은 `orca-plugin/` 에서 실행한다.

```sh
orca plugin types .          # .orca/types/ 에 타입 파일을 쓴다
orca plugin test . --json    # src/*.test.ts 를 실행한다
npx -y -p typescript@5.6.3 tsc -p . --noEmit
orca plugin validate . --json
```

## 라이선스

`vendor/marked.esm.js` 는 marked 15.0.12 의 ESM 빌드를 그대로 복사한 파일이다. marked 는 MIT 라이선스다 (Copyright (c) 2011-2025, Christopher Jeffrey).

`vendor/cytoscape.esm.min.mjs` 는 cytoscape 3.31.0 의 ESM 빌드를 그대로 복사한 파일이다. cytoscape 는 MIT 라이선스다 (Copyright (c) The Cytoscape Consortium).
