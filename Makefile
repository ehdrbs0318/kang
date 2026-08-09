# 부트스트랩 순서를 사람이 기억하지 않게 못박는다 (V0004 B3).
#
# 순환은 이렇다: `.kang/index.tsv` 는 `kang` 바이너리가 만들고, 그 바이너리는 매크로가
# 붙은 코드를 컴파일해야 나오는데, 그 컴파일이 인덱스를 읽는다. 인덱스가 낡으면 매크로가
# 컴파일을 세우므로 인덱스를 낼 바이너리를 만들 수 없다.
#
# **B3 의 "인덱스 부재 시 warn" 이 그 순환을 끊는다.** `KANG_INDEX` 를 없는 경로로 두면 이
# 한 번의 빌드만 검증 없이 통과한다. `.cargo/config.toml` 의 `[env]` 는 이미 환경에 있는
# 값을 덮지 않으므로(`force` 를 켜지 않았다) 여기의 지정이 이긴다.
#
# `xtask` 를 쓰지 않았다 — 크레이트가 하나 늘고, V0001 §10.1 의 목록이 셋으로 갈리며,
# 여기서 할 일은 셸 두 줄이다.
#
# ponytail: `make index` 뒤의 첫 `cargo build` 는 kang·kang-macros 를 다시 컴파일한다
# (`rerun-if-env-changed=KANG_INDEX`). 약 3초이며, 아파지면 부트스트랩에 별도
# `CARGO_TARGET_DIR` 을 준다.
.PHONY: index index-check deps-check

# 인덱스를 문서의 현재 내용으로 다시 낸다.
index:
	KANG_INDEX=/kang-bootstrap-no-index cargo run -q -p kang -- index .kang/index.tsv

# 문서와 커밋된 인덱스가 어긋났는지 본다. CI 가 이것으로 드리프트를 막는다 (Task 3 J2).
# `write_index` 가 문서 경로로 정렬하므로 두 번 돌리면 같은 바이트다.
index-check: index
	git diff --exit-code .kang/index.tsv

# V0001 §10.1 의 의존성 허용 목록을 실제 Cargo.toml 과 대조한다.
#
# 그 절은 "목록에 없는 것은 쓰지 않는다" 를 규칙으로 세웠지만 재는 수단이 없었다 —
# V0004 Task 4 가 "CI 에서 잴 방법을 정한다" 를 Task 5 로 이월했고 Task 5 에 그 항목이
# 들어가지 않아 검사가 하나도 남지 않았다. 목록에 없는 의존성이 들어와도 아무것도 잡지
# 못하는 상태였다.
#
# 워크스페이스 내부 path 의존은 §10.1 이 명시적으로 제외하므로 세지 않는다.
# `cargo metadata` 대신 `cargo tree --depth 1` 을 쓴다 — jq 가 필요 없고, 재는 것이
# "이 크레이트가 직접 쓰는 외부 크레이트" 라서 깊이 1 이 정확히 그 정의다.
#
# ponytail: 전이 의존성은 재지 않는다. `Cargo.lock` 이 커밋되어 있어 새 전이 의존은
# diff 로 보인다. 전이까지 목록으로 묶어야 할 만큼 커지면 `cargo-deny` 를 들인다.
deps-check:
	@echo 'kang 의 외부 의존성 (허용: sha2)'
	@cargo tree --depth 1 -p kang -e normal --prefix none --no-dedupe 2>/dev/null \
	  | tail -n +2 | awk '{print $$1}' | grep -v '^kang-macros$$' | grep -v '^$$' | sort -u > /tmp/kang-deps.txt
	@printf 'sha2\n' > /tmp/kang-allow.txt
	@diff -u /tmp/kang-allow.txt /tmp/kang-deps.txt \
	  || { echo 'kang 의 의존성이 V0001 §10.1 목록과 다릅니다. 목록을 고치고 근거를 남기거나 의존성을 빼세요.'; exit 1; }
	@echo 'kang-macros 의 외부 의존성 (허용: proc-macro2, quote, syn)'
	@cargo tree --depth 1 -p kang-macros -e normal --prefix none --no-dedupe 2>/dev/null \
	  | tail -n +2 | awk '{print $$1}' | grep -v '^$$' | sort -u > /tmp/macros-deps.txt
	@printf 'proc-macro2\nquote\nsyn\n' | sort -u > /tmp/macros-allow.txt
	@diff -u /tmp/macros-allow.txt /tmp/macros-deps.txt \
	  || { echo 'kang-macros 의 의존성이 V0001 §10.1 목록과 다릅니다. 목록을 고치고 근거를 남기거나 의존성을 빼세요.'; exit 1; }
	@echo '의존성이 V0001 §10.1 목록과 일치합니다.'
