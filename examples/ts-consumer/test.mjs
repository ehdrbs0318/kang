// 이 예시의 게이트. `npm test` 가 이 파일 하나를 돌리고, 네 단계를 순서대로 본다.
//
//   (1) kang build   문서가 컴파일되는가
//   (2) kang types   topic 과 rev 를 TypeScript 리터럴 타입으로 낸다
//   (3) tsc          코드의 애노테이션이 그 타입과 맞는가 — 없는 topic 은 keyof 제약
//                    위반, 낡은 rev 는 리터럴 불일치로 여기서 걸린다
//   (4) 핀 대조      (3)이 검사할 것을 실제로 갖고 있었는지 tsc 없이 한 번 더 본다
//
// 정책 문서를 고치면 rev 가 바뀌고 (3)이 컴파일을 세운다. 진단이 새 rev 를 말하므로
// 코드의 핀을 그것으로 고치면 다시 통과한다.
//
// `kang` 바이너리는 `$KANG` 또는 PATH 에서 찾는다. 이 예시는 kang 을 소스에서 빌드하지
// 않는다 — 소비자에게 Rust 툴체인이 없다는 것이 이 예시의 전제다. 받는 방법은 저장소
// 루트 README 의 「설치」에 있다.

import { execFileSync } from "node:child_process";
import { cpSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const 여기 = dirname(fileURLToPath(import.meta.url));
const kang = process.env.KANG ?? "kang";

/**
 * 명령을 돌리고, 실패하면 그 종료 코드로 이 프로세스를 끝낸다.
 *
 * @param {string} 명령 - 실행할 프로그램
 * @param {string[]} 인자 - 인자 목록
 * @param {string} cwd - 실행할 디렉토리
 */
function 돌린다(명령, 인자, cwd) {
  console.error(`$ ${명령} ${인자.join(" ")}`);
  try {
    execFileSync(명령, 인자, { cwd, stdio: "inherit" });
  } catch (오류) {
    // 프로그램이 아예 실행되지 않으면 종료 코드가 없다. 그 사실을 그대로 말한다 —
    // "테스트 실패" 로 뭉치면 kang 이 없는 것과 문서가 틀린 것을 구분할 수 없다.
    if (오류.status === undefined) {
      console.error(`${명령} 을 실행하지 못했습니다 — ${오류.code ?? 오류.message}`);
      console.error(`  fix: kang 은 $KANG 또는 PATH 에서 찾습니다. tsc 는 npm install 이 놓습니다.`);
      process.exit(1);
    }
    process.exit(오류.status);
  }
}

/**
 * 문서를 임시 git 저장소로 옮겨 그 안에서 (1)(2)를 돌린다.
 *
 * kang 의 프로젝트 루트는 git 저장소 루트다(V0001 3절). 이 예시는 kang 저장소 안에
 * 있어서 여기서 kang 을 그냥 부르면 kang 자신의 문서까지 루트에 딸려 오고, git 저장소
 * 안에 git 저장소를 중첩해 커밋할 수는 없다. 그래서 문서만 임시 저장소로 복사한다.
 * **당신의 프로젝트에서는 이 함수가 필요 없다** — 프로젝트 루트에서 두 명령을 그냥 돈다.
 *
 * 같은 이유로 `docs/` 의 두 문서는 서로 import 하지 않는다. import 주소는 루트 상대이고
 * 이 문서들은 두 루트(여기와 kang 저장소)에 동시에 속하므로, 한쪽에서 맞는 주소는 다른
 * 쪽에서 K002 다. 각 문서가 자기 keyword 를 선언해 스스로 완결되게 두었다.
 */
function 문서를_컴파일하고_타입을_낸다() {
  const 작업 = mkdtempSync(join(tmpdir(), "kang-ts-consumer-"));
  try {
    // 문서 경로가 topic 주소의 일부이므로 `docs/` 구조를 그대로 옮긴다.
    cpSync(join(여기, "docs"), join(작업, "docs"), { recursive: true });
    돌린다("git", ["init", "-q"], 작업);
    돌린다(kang, ["build"], 작업);
    돌린다(kang, ["types", join(여기, ".kang", "generated.ts")], 작업);
  } finally {
    rmSync(작업, { recursive: true, force: true });
  }
}

/**
 * 코드의 핀이 생성 타입에 그대로 있는지 본다.
 *
 * (3)과 같은 것을 보지만 tsc 를 거치지 않는다. tsc 가 생성 타입을 프로그램에 넣지
 * 못하면(코드가 import 를 잃거나 tsconfig 의 include 가 빗나가면) 애노테이션은 아무
 * 검사도 받지 않고 통과하는데, (3)만으로는 그 사실이 보이지 않는다.
 *
 * ponytail: 문자열 포함으로만 대조한다 — topic 이름에 `"` 나 `\` 가 들어가면 생성
 * 타입과 코드의 이스케이프 표기가 갈릴 수 있고 그때 거짓 실패한다. 그런 이름이 실제로
 * 생기면 생성 타입을 TS 로 읽어 `keyof` 로 대조하는 쪽으로 올린다.
 */
function 핀을_대조한다() {
  const 타입 = readFileSync(join(여기, ".kang", "generated.ts"), "utf8");
  const 코드 = readFileSync(join(여기, "src", "refund.ts"), "utf8");
  const 핀들 = [...코드.matchAll(/kangTopic\("(.+?)",\s*"(.+?)"\)/g)];

  // 핀이 하나도 없으면 (3)은 검사할 것이 없었다. 통과는 아무것도 증명하지 않는다.
  if (핀들.length === 0) {
    console.error("src/refund.ts 에 kang 핀이 하나도 없습니다 — tsc 가 검사한 것이 없습니다.");
    process.exit(1);
  }

  // 코드가 가리키는 (topic, rev) 짝을 순회하며 생성 타입의 줄과 견준다.
  for (const [, topic, rev] of 핀들) {
    if (!타입.includes(`"${topic}": "${rev}";`)) {
      console.error(`핀이 문서와 맞지 않습니다 — ${topic} rev ${rev}`);
      console.error(`  fix: .kang/generated.ts 의 그 topic 줄에 있는 rev 로 코드의 핀을 고치세요.`);
      process.exit(1);
    }
  }

  console.error(`핀 ${핀들.length}개가 문서와 일치합니다.`);
}

문서를_컴파일하고_타입을_낸다();
돌린다(join(여기, "node_modules", ".bin", "tsc"), ["-p", "."], 여기);
핀을_대조한다();
