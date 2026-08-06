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
.PHONY: index index-check

# 인덱스를 문서의 현재 내용으로 다시 낸다.
index:
	KANG_INDEX=/kang-bootstrap-no-index cargo run -q -p kang -- index .kang/index.tsv

# 문서와 커밋된 인덱스가 어긋났는지 본다. CI 가 이것으로 드리프트를 막는다 (Task 3 J2).
# `write_index` 가 문서 경로로 정렬하므로 두 번 돌리면 같은 바이트다.
index-check: index
	git diff --exit-code .kang/index.tsv
