#!/usr/bin/env python3
"""도그푸딩 문서 모음을 측정한다. V0004 Task 9 의 M1, M2, M3 을 한 번에 출력한다.

- M1: `check.rs` 의 `split_names` 마커에 적은 참조 병합 한계 수치
- M2: 마크다운을 kang 규칙에 맞추지 않고 그대로 옮겼을 때 나오는 진단의 분포
- M3: 실제 문서 모음에서 명령 하나에 걸리는 시간

마커를 갱신할 때는 이 스크립트를 다시 실행하고, 출력에 나온 커밋 sha 를 함께 적는다.

    python3 scripts/measure-corpus.py

`kang` 바이너리는 `KANG` 환경 변수가 가리키는 파일을 쓴다. 변수가 없으면 `target/release/kang` 을 쓴다.
"""

import os
import re
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

root = Path(
    subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        capture_output=True, text=True, check=True,
    ).stdout.strip()
)
KANG = Path(os.environ.get("KANG") or root / "target" / "release" / "kang")
if not KANG.exists():
    sys.exit(f"kang 바이너리가 없습니다: {KANG} — cargo build --release 를 먼저 실행하세요")

# 백틱 쌍 하나. 이스케이프한 백틱(\`)은 심볼이 아니므로 앞이 역슬래시면 제외한다.
BACKTICK = re.compile(r"(?<!\\)`([^`]+)(?<!\\)`")
# keyword 선언의 앞부분. `:` 앞의 백틱 이름이 2개 이상이면 계층 keyword 다.
KEYWORD_HEAD = re.compile(r"^keyword\s+(.*?):")


def backtick_segments(text: str):
    """코드 펜스 밖의 백틱 쌍을 (이름, 줄 번호) 목록으로 돌려준다."""
    segments = []
    in_fence = False
    # 줄 단위로 읽으며 코드 펜스 안을 건너뛴다. 스펙 4.2 에 따라 펜스 안은 심볼이 아니다.
    for line_no, line in enumerate(text.splitlines(), 1):
        if line.lstrip().startswith("```"):
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        segments += [(name, line_no) for name in BACKTICK.findall(line)]
    return segments


def m1():
    """참조 병합 한계를 .kang 문서 모음에서 다시 측정한다."""
    # `.kang` 은 확장자이면서 인덱스를 만드는 디렉토리 이름이다. 그래서 파일만 남긴다.
    files = [p for p in sorted(root.glob("**/*.kang")) if p.is_file() and "target" not in p.parts]
    line_count = multi_segment_lines = hierarchical = near_risk = collisions = 0
    scope = set()
    segments_by_file = {}
    # 첫 번째 읽기: 선언된 이름을 모두 모은다. 병합은 합친 이름이 이 집합에 있을 때만 일어난다.
    for p in files:
        text = p.read_text()
        line_count += len(text.splitlines())
        for line in text.splitlines():
            head = KEYWORD_HEAD.match(line)
            if head:
                names = BACKTICK.findall(head.group(1))
                scope.add(".".join(names))
                if len(names) >= 2:
                    hierarchical += 1
        segments_by_file[p] = backtick_segments(text)
    # 두 번째 읽기: 같은 줄에서 이어지는 조각 쌍마다, 합친 이름이 스코프에 있는지 확인한다.
    for p, segments in segments_by_file.items():
        by_line = {}
        for name, line_no in segments:
            by_line.setdefault(line_no, []).append(name)
        for line_no, names in by_line.items():
            if len(names) >= 2:
                multi_segment_lines += 1
            # 이웃한 쌍을 확인한다. 합친 이름이 스코프에 있으면, 왼쪽부터 가장 긴 이름을 고르는 규칙이 그 이름을 고른다.
            for left, right in zip(names, names[1:]):
                joined = f"{left}.{right}"
                if any(n.startswith(f"{left}.") for n in scope):
                    near_risk += 1
                if joined in scope and left in scope and right in scope:
                    collisions += 1
                    print(f"  충돌: {p.relative_to(root)}:{line_no} — {left} + {right} → {joined}")
    print(f"M1 참조 병합 한계 (.kang 문서 모음, 파일 {len(files)})")
    print(f"  줄 수                 {line_count}")
    print(f"  백틱 조각 2개 이상 줄 {multi_segment_lines}")
    print(f"  선언된 계층 keyword   {hierarchical}")
    print(f"  근접 위험             {near_risk}")
    print(f"  실제 충돌             {collisions}")


def m2():
    """마크다운을 규칙에 맞추지 않고 그대로 옮기면 어떤 진단이 나오는지, 진단 코드 분포를 측정한다."""
    # CONTEXT.md 와 docs/adr/*.md 는 이미 .kang 으로 옮겼다. 그래서 지금 있는 마크다운만 측정한다.
    candidates = [
        p
        for p in [root / "CONTEXT.md", root / "README.md"]
        + sorted((root / "docs" / "adr").glob("*.md"))
        + sorted(root.glob("plans/*/*.md"))
        if p.is_file()
    ]
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        subprocess.run(["git", "init", "-q", str(tmp)], check=True)
        line_count = 0
        # 각 마크다운을 frontmatter 만 붙여 그대로 .kang 으로 옮긴다.
        for p in candidates:
            text = p.read_text()
            line_count += len(text.splitlines())
            target = tmp / p.relative_to(root).with_suffix(".kang")
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(f"---\ndescription: naive\n---\n\n{text}")
        result = subprocess.run(
            [str(KANG), "build"], cwd=tmp, capture_output=True, text=True
        )
    codes = re.findall(r"error\[(K\d+)\]", result.stdout + result.stderr)
    distribution = {c: codes.count(c) for c in sorted(set(codes))}
    print(f"\nM2 규칙 없이 옮긴 마크다운 (파일 {len(candidates)}, 줄 {line_count}) — exit {result.returncode}")
    print(f"  파싱 단계 진단 총계 {len(codes)}")
    for c, n in distribution.items():
        print(f"  {c}  {n}")
    # 파싱 error 가 있으면 검사 단계가 실행되지 않는다. 그래서 위 분포에 K001 이 없다.
    # 파싱 error 를 다 고친 뒤 나오는 다음 단계 진단의 수는 본문 백틱 조각 수를 넘지 않는다.
    segments = [name for p in candidates for name, _ in backtick_segments(p.read_text())]
    print(f"  (다음 단계 진단의 최댓값) 코드 펜스 밖 백틱 조각 {len(segments)}, 서로 다른 이름 {len(set(segments))}")


def m3():
    """실제 문서 모음에서 build, show, index 의 실행 시간 중앙값을 측정한다."""
    doc = subprocess.run(
        [str(KANG), "list"], cwd=root, capture_output=True, text=True, check=True
    ).stdout.splitlines()[0].split(":")[0]
    index_path = Path(tempfile.gettempdir()) / "kang-measure-index.tsv"
    commands = {
        "build": [str(KANG), "build"],
        "show": [str(KANG), "show", doc],
        "index": [str(KANG), "index", str(index_path)],
    }
    print(f"\nM3 성능 (5회 중앙값)")
    # 각 명령을 5회 실행해 중앙값을 구한다. 그 전에 한 번 실행한 결과는 캐시 영향이 있으므로 버린다.
    for name, args in commands.items():
        print(f"  {name:22} {median_ms(args, root):7.1f} ms")
    index_path.unlink(missing_ok=True)
    # 문서 처리 시간과 디렉토리 탐색 시간을 나눠 본다. 측정 당시 `collect`(resolve.rs) 는
    # .gitignore 를 보지 않았다. 그래서 `target/` 같은 큰 디렉토리가 있으면 탐색 시간이 대부분을 차지했다.
    print(f"  kang --help (기준선)   {median_ms([str(KANG), '--help'], root):7.1f} ms")
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        subprocess.run(["git", "init", "-q", str(tmp)], check=True)
        # .kang 문서만 옮긴 저장소. 문서는 같고, 탐색할 다른 파일이 없다.
        for p in [p for p in root.glob("**/*.kang") if p.is_file() and "target" not in p.parts]:
            target = tmp / p.relative_to(root)
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(p.read_bytes())
        print(f"  build (문서만 옮겨서)  {median_ms([str(KANG), 'build'], tmp):7.1f} ms")


def median_ms(args, cwd):
    """명령을 5회 실행한 시간의 중앙값을 밀리초로 돌려준다."""
    subprocess.run(args, cwd=cwd, capture_output=True)
    elapsed = []
    for _ in range(5):
        start = time.perf_counter()
        subprocess.run(args, cwd=cwd, capture_output=True, check=True)
        elapsed.append((time.perf_counter() - start) * 1000)
    return statistics.median(elapsed)


if __name__ == "__main__":
    sha = subprocess.run(
        ["git", "rev-parse", "--short", "HEAD"],
        capture_output=True, text=True, check=True,
    ).stdout.strip()
    dirty = subprocess.run(["git", "diff", "--quiet"], cwd=root).returncode != 0
    print(f"측정 시점 {sha}{' + 커밋되지 않은 변경' if dirty else ''}\n")
    m1()
    m2()
    m3()
