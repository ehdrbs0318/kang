// kang 이 생성한 파일입니다. 손으로 고치지 않습니다 — 다음 `kang types` 가 덮어씁니다.
// 없는 topic 은 keyof 제약 위반, 낡은 rev 는 리터럴 불일치로 tsc 가 잡습니다.

export interface KangTopics {
  "docs/proration#부분 환불의 계산": "970bc8";
  "docs/refunds#환불 가능 기간": "cdd44d";
}

export function kangTopic<K extends keyof KangTopics>(
  topic: K,
  rev: KangTopics[K],
): MethodDecorator {
  return () => {};
}
