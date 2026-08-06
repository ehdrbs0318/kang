#!/usr/bin/env python3
"""도그푸딩 코퍼스 실측. V0004 Task 9 의 M1·M2·M3 을 한 번에 낸다.

`check.rs` 의 `이름_분할` 마커가 담은 참조 병합 천장 수치(M1)와, 마크다운을 규칙을
모른 채 그대로 옮겼을 때 나는 진단의 분포(M2), 그리고 실제 코퍼스에서의 명령 소요
시간(M3)을 낸다. 마커를 갱신할 때 이 스크립트를 다시 돌리고 출력의 커밋 sha 를 함께
적는다.

    python3 scripts/measure-corpus.py

`kang` 바이너리는 `KANG` 환경변수로, 없으면 `target/release/kang` 을 쓴다.
"""

import os
import re
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

루트 = Path(
    subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        capture_output=True, text=True, check=True,
    ).stdout.strip()
)
KANG = Path(os.environ.get("KANG") or 루트 / "target" / "release" / "kang")
if not KANG.exists():
    sys.exit(f"kang 바이너리가 없습니다: {KANG} — cargo build --release 를 먼저 돌리세요")

# 백틱 쌍 하나. 이스케이프한 백틱(\`)은 심볼이 아니므로 앞이 역슬래시면 제외한다.
백틱 = re.compile(r"(?<!\\)`([^`]+)(?<!\\)`")
# keyword 선언의 머리. `:` 앞의 백틱 이름 개수가 2 이상이면 계층 keyword 다.
선언 = re.compile(r"^keyword\s+(.*?):")


def 백틱_조각(글: str):
    """코드 펜스 밖의 백틱 쌍을 (이름, 줄 번호) 로 낸다."""
    조각 = []
    펜스 = False
    # 줄 단위로 훑으며 코드 펜스 안을 건너뛴다 (스펙 4.2 — 펜스 안은 심볼이 아니다).
    for 번호, 줄 in enumerate(글.splitlines(), 1):
        if 줄.lstrip().startswith("```"):
            펜스 = not 펜스
            continue
        if 펜스:
            continue
        조각 += [(이름, 번호) for 이름 in 백틱.findall(줄)]
    return 조각


def m1():
    """참조 병합 천장 재측정 — .kang 코퍼스."""
    # `.kang` 은 확장자이자 인덱스 산출 디렉토리 이름이므로 파일만 남긴다.
    파일들 = [p for p in sorted(루트.glob("**/*.kang")) if p.is_file() and "target" not in p.parts]
    줄수 = 조각2줄 = 계층 = 근접위험 = 충돌 = 0
    스코프 = set()
    조각별 = {}
    # 1차 훑기 — 선언된 이름을 모두 모은다. 병합은 이 집합에서 해석되어야 일어난다.
    for p in 파일들:
        글 = p.read_text()
        줄수 += len(글.splitlines())
        for 줄 in 글.splitlines():
            머리 = 선언.match(줄)
            if 머리:
                이름들 = 백틱.findall(머리.group(1))
                스코프.add(".".join(이름들))
                if len(이름들) >= 2:
                    계층 += 1
        조각별[p] = 백틱_조각(글)
    # 2차 훑기 — 같은 줄에서 이어지는 조각 쌍마다 합친 이름이 스코프에 있는지 본다.
    for p, 조각 in 조각별.items():
        줄별 = {}
        for 이름, 번호 in 조각:
            줄별.setdefault(번호, []).append(이름)
        for 번호, 이름들 in 줄별.items():
            if len(이름들) >= 2:
                조각2줄 += 1
            # 인접 쌍을 훑는다. 합친 이름이 스코프에 있으면 왼쪽 최장이 그것을 택한다.
            for 앞, 뒤 in zip(이름들, 이름들[1:]):
                합 = f"{앞}.{뒤}"
                if any(n.startswith(f"{앞}.") for n in 스코프):
                    근접위험 += 1
                if 합 in 스코프 and 앞 in 스코프 and 뒤 in 스코프:
                    충돌 += 1
                    print(f"  충돌: {p.relative_to(루트)}:{번호} — {앞} + {뒤} → {합}")
    print(f"M1 참조 병합 천장 (.kang 코퍼스, 파일 {len(파일들)})")
    print(f"  줄 수                 {줄수}")
    print(f"  백틱 조각 2개 이상 줄 {조각2줄}")
    print(f"  선언된 계층 keyword   {계층}")
    print(f"  근접 위험             {근접위험}")
    print(f"  실제 충돌             {충돌}")


def m2():
    """마크다운을 규칙 없이 그대로 옮기면 무엇이 나는가 — 진단 코드 분포."""
    후보 = (
        [루트 / "CONTEXT.md", 루트 / "README.md"]
        + sorted((루트 / "docs" / "adr").glob("*.md"))
        + sorted(루트.glob("plans/*/*.md"))
    )
    with tempfile.TemporaryDirectory() as 임시:
        임시 = Path(임시)
        subprocess.run(["git", "init", "-q", str(임시)], check=True)
        줄수 = 0
        # 각 마크다운을 frontmatter 만 붙여 그대로 .kang 으로 옮긴다.
        for p in 후보:
            글 = p.read_text()
            줄수 += len(글.splitlines())
            대상 = 임시 / p.relative_to(루트).with_suffix(".kang")
            대상.parent.mkdir(parents=True, exist_ok=True)
            대상.write_text(f"---\ndescription: naive\n---\n\n{글}")
        결과 = subprocess.run(
            [str(KANG), "build"], cwd=임시, capture_output=True, text=True
        )
    코드 = re.findall(r"error\[(K\d+)\]", 결과.stdout + 결과.stderr)
    분포 = {c: 코드.count(c) for c in sorted(set(코드))}
    print(f"\nM2 규칙 없이 옮긴 마크다운 (파일 {len(후보)}, 줄 {줄수}) — exit {결과.returncode}")
    print(f"  파싱 단계 진단 총계 {len(코드)}")
    for c, n in 분포.items():
        print(f"  {c}  {n}")
    # 파싱 error 가 있으면 검사 단계가 돌지 않으므로 위 분포에 K001 이 없다. 파싱을 다
    # 고친 뒤 오는 둘째 물결의 크기는 본문 백틱 조각 수가 상한이다.
    조각 = [이름 for p in 후보 for 이름, _ in 백틱_조각(p.read_text())]
    print(f"  (둘째 물결 상한) 코드 펜스 밖 백틱 조각 {len(조각)}, 서로 다른 이름 {len(set(조각))}")


def m3():
    """성능 실측 — 실제 코퍼스에서 build·show·index 의 중앙값."""
    문서 = subprocess.run(
        [str(KANG), "list"], cwd=루트, capture_output=True, text=True, check=True
    ).stdout.splitlines()[0].split(":")[0]
    인덱스 = Path(tempfile.gettempdir()) / "kang-measure-index.tsv"
    명령 = {
        "build": [str(KANG), "build"],
        "show": [str(KANG), "show", 문서],
        "index": [str(KANG), "index", str(인덱스)],
    }
    print(f"\nM3 성능 (5회 중앙값)")
    # 각 명령을 5회 돌려 중앙값을 낸다. 첫 회는 캐시 편향이 있으므로 버린다.
    for 이름, 인자 in 명령.items():
        print(f"  {이름:22} {중앙값(인자, 루트):7.1f} ms")
    인덱스.unlink(missing_ok=True)
    # 문서 처리 비용과 순회 비용을 가른다. `수집`(resolve.rs) 은 .gitignore 를 보지 않아
    # `target/` 같은 큰 디렉토리가 있으면 순회가 비용을 지배한다.
    print(f"  kang --help (기준선)   {중앙값([str(KANG), '--help'], 루트):7.1f} ms")
    with tempfile.TemporaryDirectory() as 임시:
        임시 = Path(임시)
        subprocess.run(["git", "init", "-q", str(임시)], check=True)
        # .kang 문서만 옮긴 저장소 — 같은 코퍼스, 순회할 곁가지가 없다.
        for p in [p for p in 루트.glob("**/*.kang") if p.is_file() and "target" not in p.parts]:
            대상 = 임시 / p.relative_to(루트)
            대상.parent.mkdir(parents=True, exist_ok=True)
            대상.write_bytes(p.read_bytes())
        print(f"  build (문서만 옮겨서)  {중앙값([str(KANG), 'build'], 임시):7.1f} ms")


def 중앙값(인자, cwd):
    """명령을 5회 돌린 소요 시간의 중앙값을 밀리초로 낸다."""
    subprocess.run(인자, cwd=cwd, capture_output=True)
    걸림 = []
    for _ in range(5):
        시작 = time.perf_counter()
        subprocess.run(인자, cwd=cwd, capture_output=True, check=True)
        걸림.append((time.perf_counter() - 시작) * 1000)
    return statistics.median(걸림)


if __name__ == "__main__":
    sha = subprocess.run(
        ["git", "rev-parse", "--short", "HEAD"],
        capture_output=True, text=True, check=True,
    ).stdout.strip()
    더러움 = subprocess.run(["git", "diff", "--quiet"], cwd=루트).returncode != 0
    print(f"측정 시점 {sha}{' + 커밋되지 않은 변경' if 더러움 else ''}\n")
    m1()
    m2()
    m3()
