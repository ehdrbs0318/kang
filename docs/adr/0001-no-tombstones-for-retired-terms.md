# ADR-0001: 폐기된 용어에 묘비를 남기지 않는다

**Load when:** deprecated / `@deprecated` / 폐기 표현 수단을 kang 에 넣자는 제안이 올 때. 또는 개념 rename 시 옛 이름을 어떻게 할지 물을 때.

- **Status**: accepted
- **Date**: 2026-08-05
- **Spec**: `plans/*/V0001-kang-language-design.md` §2 설계 원칙

## Context

kang 의 창립 원칙 중 하나는 "최신 버전만 존재한다" 이다. 그런데 실제 정책 문서 코퍼스(`~/Project/ax-conta/docs/policies/domain-model.md`)는 폐기된 용어를 지우지 않고 묘비로 남기고 있었다.

```
| ~~WorkflowAsset~~ (구) | entity | → WorkflowFile + FileRevision +
  WorkflowFileEvent 로 분해 (2026-05-12 WS Files cutover, ADR-0027) |
| ~~WorkflowSpace~~ (구) | aggregate | WorkflowSpaceService facade 폐지 |
```

묘비를 지지하는 논거는 실질적이다. 폐기된 이름은 아직 살아있는 코드와 옛 문서와 사람의 기억 속에 있다. 그 이름으로 검색한 에이전트에게 "그런 개념 없음" 이라고 답하면, 에이전트는 없다고 결론 내리고 새로 만든다. 그게 정확히 kang 이 막으려는 SoT 분열이다.

## Decision

**묘비를 두지 않는다. 폐기된 용어는 지운다.** 언어에 deprecated 표현 수단을 만들지 않는다.

근거는 kang 에서 rename 이 원자적이라는 점이다. 심볼 이름을 바꾸면 그것을 참조하는 모든 문서가 그 자리에서 컴파일 에러가 나고, 같은 변경 안에서 전부 갱신된다. **낡은 이름이 살아남을 시간 창이 존재하지 않는다.**

마크다운이 묘비를 필요로 했던 것은 강제 장치가 없어서다. rename 후에도 옛 이름을 쓰는 문서가 조용히 살아남기 때문에, 그 문서를 읽는 사람을 위한 리다이렉트가 필요했다. kang 에는 그 상태가 존재할 수 없다.

히스토리는 git 이 담당한다.

## Consequences

### Easier

- 어느 시점에 문서를 읽어도 유효한 정의가 정확히 하나다. "이게 아직 유효한가" 를 물을 필요가 없다.
- 언어 표면이 줄어든다. deprecated 상태 기계도, 묘비 조회 규칙도, "묘비를 import 하면 어떻게 되나" 도 없다.

### Harder

- 코드베이스나 외부 시스템에 남은 옛 이름으로 검색하면 아무것도 안 나온다. kang 밖에서 rename 을 추적하는 것은 git log 의 몫이다.
- rename 의 비용이 영향 범위에 비례한다. 50곳이 참조하는 개념의 이름을 바꾸면 50곳이 깨진다. 의도된 마찰이다.

## Alternatives considered

**묘비 문법 (`keyword X moved-to Y`)** — 정의 없이 대체 대상만 가리키고, import 는 금지하되 `kang keywords` 조회에는 노출하는 안. 원칙과 양립하고 검색 연속성을 준다. 기각한 이유는 rename 원자성이 이미 문제를 해결하기 때문이다. 시간 창이 없는데 리다이렉트를 둘 이유가 없고, 묘비가 있으면 지우지 않을 핑계가 생긴다.
