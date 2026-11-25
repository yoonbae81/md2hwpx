# 슬라이스 L GREEN — 마지막 데이터 행 원형 선택 수정

## 변경 요약
- `src/compile/render.rs`의 `render_table` (수정 후 593~595행, 611~619행):
  - `body_template`을 `first_data_template`(= `template_rows[1]`, 기존과 동일)과
    `last_data_template`(= `template_rows`의 마지막 tr) 두 원형으로 분리.
  - 출력 데이터 행 i(1..=N, 헤더가 0행): i == N(마지막 출력 데이터 행, N==1 포함)이면
    마지막 원형 tr을, 그 외(1..N-1)는 첫 데이터 원형 tr을 deep_clone.
    (hp 참조 `src/hp_blocks.rs` fill_table 862-891행과 동일 규칙.)
  - 헤더만 있는 입력(rows.len()==1)은 데이터 행이 없어 기존 동작 그대로.
  - 이 외의 로직(열 수 적응, 너비 재분배, cellAddr/cellSz 채우기,
    rowCnt/colCnt 갱신)은 변경 없음.

## 테스트 결과 (종료 코드는 .exit 파일로 캡처, 파이프 없음)
- `cargo test --lib render_table` → exit 0
  - `render_table_single_data_row_uses_bottom_prototype` ok
  - `render_table_last_data_row_uses_bottom_prototype` ok
  - 2 passed; 0 failed (171 filtered out)
- `cargo test` (전체) → exit 0
  - lib: 173 passed (기준 171 + 신규 2), 0 failed — 회귀 없음
  - e2e: 6 passed, 0 failed
- `cargo build --release` → exit 0

## 증거 파일
- lastrow-green.out / lastrow-green.err / lastrow-green.exit
- lastrow-full-cargo-test.out / lastrow-full-cargo-test.err / lastrow-full-cargo-test.exit
- lastrow-release-build.out / lastrow-release-build.exit
- RED 증거(lastrow-red.*)는 수정 없이 그대로 보존됨.
