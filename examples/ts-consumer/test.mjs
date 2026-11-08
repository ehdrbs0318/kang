// 이 예시의 검사. `npm test` 가 이 파일 하나를 실행하고, 네 단계를 순서대로 확인한다.
//
//   (1) kang build   문서가 컴파일되는가
//   (2) kang types   topic 과 rev 를 TypeScript 리터럴 타입으로 출력한다
//   (3) tsc          코드의 애노테이션이 그 타입과 맞는가. 없는 topic 은 keyof 제약
//                    위반으로, 낡은 rev 는 리터럴 불일치로 여기서 실패한다
//   (4) 핀 대조      (3)이 검사할 핀이 실제로 있었는지 tsc 없이 한 번 더 확인한다
//
// 정책 문서를 고치면 rev 가 바뀌고 (3)의 컴파일이 실패한다. 진단에 새 rev 가 나온다.
// 코드의 핀을 새 rev 로 고치면 다시 통과한다.
//
// `kang` 바이너리는 `$KANG` 또는 PATH 에서 찾는다. 이 예시는 kang 을 소스에서 빌드하지
// 않는다. 이 예시는 사용자에게 Rust 툴체인이 없다고 가정한다. 받는 방법은 저장소
// 루트 README 의 「설치」에 있다.

import { execFileSync } from "node:child_process";
import { cpSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const kang = process.env.KANG ?? "kang";

/**
 * 명령을 실행하고, 실패하면 그 종료 코드로 이 프로세스를 끝낸다.
 *
 * @param {string} command - 실행할 프로그램
 * @param {string[]} args - 인자 목록
 * @param {string} cwd - 실행할 디렉토리
 */
function run(command, args, cwd) {
  console.error(`$ ${command} ${args.join(" ")}`);
  try {
    execFileSync(command, args, { cwd, stdio: "inherit" });
  } catch (error) {
    // 프로그램이 아예 실행되지 않으면 종료 코드가 없다. 그 사실을 그대로 알린다.
    // "테스트 실패" 하나로 묶으면 kang 이 없는 것과 문서가 틀린 것을 구분할 수 없다.
    if (error.status === undefined) {
      console.error(`${command} 을 실행하지 못했습니다 — ${error.code ?? error.message}`);
      console.error(`  fix: kang 은 $KANG 또는 PATH 에서 찾습니다. tsc 는 npm install 이 놓습니다.`);
      process.exit(1);
    }
    process.exit(error.status);
  }
}

/**
 * 문서를 임시 git 저장소로 복사하고, 그 안에서 (1)(2)를 실행한다.
 *
 * kang 의 프로젝트 루트는 git 저장소 루트다(V0001 3절). 이 예시는 kang 저장소 안에
 * 있다. 그래서 여기서 kang 을 그대로 실행하면 kang 자신의 문서까지 프로젝트에 들어온다.
 * git 저장소 안에 git 저장소를 중첩해 커밋할 수도 없다. 그래서 문서만 임시 저장소로 복사한다.
 * **당신의 프로젝트에서는 이 함수가 필요 없다.** 프로젝트 루트에서 두 명령을 그대로 실행한다.
 *
 * 같은 이유로 `docs/` 의 두 문서는 서로 import 하지 않는다. import 주소는 루트 기준이다.
 * 이 문서들은 두 루트(이 디렉토리와 kang 저장소)에 함께 속한다. 그래서 한쪽에서 맞는
 * 주소가 다른 쪽에서는 K002 가 된다. 두 문서는 각자 자기 keyword 를 선언하고, 다른 문서 없이 읽힌다.
 */
function compileDocsAndWriteTypes() {
  const workDir = mkdtempSync(join(tmpdir(), "kang-ts-consumer-"));
  try {
    // 문서 경로가 topic 주소의 일부이므로 `docs/` 구조를 그대로 옮긴다.
    cpSync(join(here, "docs"), join(workDir, "docs"), { recursive: true });
    run("git", ["init", "-q"], workDir);
    run(kang, ["build"], workDir);
    run(kang, ["types", join(here, ".kang", "generated.ts")], workDir);
  } finally {
    rmSync(workDir, { recursive: true, force: true });
  }
}

/**
 * 코드의 핀이 생성된 타입에 그대로 있는지 확인한다.
 *
 * (3)과 같은 것을 확인하지만 tsc 를 거치지 않는다. tsc 가 생성된 타입을 읽지 못할 수 있다.
 * 예: 코드에서 import 가 빠졌거나, tsconfig 의 include 가 그 파일을 포함하지 않는다.
 * 그러면 애노테이션은 아무 검사도 받지 않고 통과한다. (3)만으로는 이 상황이 보이지 않는다.
 *
 * ponytail: 문자열이 들어 있는지로만 비교한다. topic 이름에 `"` 나 `\` 가 있으면 생성된
 * 타입과 코드의 이스케이프 표기가 달라질 수 있다. 그때는 맞는 핀도 실패한다. 그런 이름이
 * 실제로 생기면 생성된 타입을 TS 로 읽어 `keyof` 로 비교하도록 바꾼다.
 */
function checkPins() {
  const types = readFileSync(join(here, ".kang", "generated.ts"), "utf8");
  const code = readFileSync(join(here, "src", "refund.ts"), "utf8");
  const pins = [...code.matchAll(/kangTopic\("(.+?)",\s*"(.+?)"\)/g)];

  // 핀이 하나도 없으면 (3)이 검사한 것이 없다. 그때의 통과는 아무것도 증명하지 않는다.
  if (pins.length === 0) {
    console.error("src/refund.ts 에 kang 핀이 하나도 없습니다 — tsc 가 검사한 것이 없습니다.");
    process.exit(1);
  }

  // 코드가 가리키는 (topic, rev) 쌍마다 생성된 타입의 줄과 비교한다.
  for (const [, topic, rev] of pins) {
    if (!types.includes(`"${topic}": "${rev}";`)) {
      console.error(`핀이 문서와 맞지 않습니다 — ${topic} rev ${rev}`);
      console.error(`  fix: .kang/generated.ts 의 그 topic 줄에 있는 rev 로 코드의 핀을 고치세요.`);
      process.exit(1);
    }
  }

  console.error(`핀 ${pins.length}개가 문서와 일치합니다.`);
}

compileDocsAndWriteTypes();
run(join(here, "node_modules", ".bin", "tsc"), ["-p", "."], here);
checkPins();
