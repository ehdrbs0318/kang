import { kangTopic } from "../.kang/generated";

/** 한 번의 결제가 덮는 기간(일). 이 예시는 월 구독 하나만 판다. */
const subscriptionDays = 30;

/** 환불 요청에 대한 결정. `approved` 가 거짓이면 `amount` 는 0 이다. */
export type RefundDecision = { approved: boolean; amount: number };

/**
 * 환불 요청을 정책대로 결정한다.
 *
 * 각 메소드의 `@kangTopic` 에는 그 메소드가 구현하는 정책의 주소와, 구현할 때 읽은
 * 원문의 rev 핀이 들어 있다. 정책 원문이 바뀌면 핀이 맞지 않아 `tsc` 컴파일이 실패한다.
 */
export class RefundService {
  /**
   * 접수된 환불 요청을 받아 줄지 결정한다.
   *
   * @param elapsedDays - 결제일부터 접수일까지 지난 날 수
   * @param paidAmount - 원래 결제한 금액
   * @param usedDays - 구매자가 이미 사용한 날 수
   * @returns 승인 여부와 돌려줄 금액
   */
  @kangTopic("docs/refunds#환불 가능 기간", "cdd44d")
  decide(elapsedDays: number, paidAmount: number, usedDays: number): RefundDecision {
    // 정책이 정한 기간은 30일이다. 31일째부터는 거절한다. 정산 여부는 묻지 않는다.
    // 판매자에게서 대금을 회수하는 절차 때문에 구매자를 기다리게 하지 않는 것이 정책이다.
    if (elapsedDays > 30) {
      return { approved: false, amount: 0 };
    }

    return { approved: true, amount: this.refundAmount(paidAmount, usedDays) };
  }

  /**
   * 사용하지 않은 기간에 해당하는 금액만 계산한다.
   *
   * @param paidAmount - 원래 결제한 금액
   * @param usedDays - 이미 사용한 날 수. 하루라도 쓴 날은 하루 전체로 센다
   * @returns 돌려줄 금액. 전체 기간을 다 썼으면 0
   */
  @kangTopic("docs/proration#부분 환불의 계산", "970bc8")
  refundAmount(paidAmount: number, usedDays: number): number {
    // 하루 단위로 자르고, 하루라도 쓴 날은 하루 전체로 센다. 정책은 결제 당일(usedDays 0)도
    // 하루를 쓴 것으로 센다. 그래서 최솟값은 1 이다.
    const countedDays = Math.max(1, Math.ceil(usedDays));

    return Math.floor((paidAmount * Math.max(0, subscriptionDays - countedDays)) / subscriptionDays);
  }
}
