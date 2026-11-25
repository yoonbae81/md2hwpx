# 슬라이스 S — `^` 윗첨자 구문 (RED → GREEN)

## 변경 파일·행 요약

### compile (마크다운 → HWPX)
- `src/compile/postprocess.rs`
  - `superscript_char_pr_ids()`(평문 윗첨자 charPr 파생 — `hh:supscript`만 추가,
    textColor 변경 없음. 기존 `superscript_red_char_pr_ids`(붉은 별표용)와 구분).
  - `Superscript` struct + `RunRestyler` 구현: `split_node`는 바이트 스캔으로
    `^`를 만나면 다음 문자가 없거나 공백/탭이면 리터럴 `^`(평문 유지),
    비공백이면 `^`를 떼고 다음 공백/탭/끝까지를 윗첨자 구간으로 표시.
    윗첨자 구간 안의 `^`(다음이 공백 아님)는 구간에 흡수. 구간은 `^` 바이트부터
    잡아 spans_to_pieces로 자른 뒤 윗첨자 조각 선두의 `^`를 뗀다.
  - `apply_superscript()` public 진입(apply_run_restyler 엔진 재사용).
  - 테스트 추가: `superscript_split_basic`, `superscript_literal_caret`,
    `superscript_multiple`, `superscript_run_has_supscript_char_pr`.
- `src/compile/mod.rs` (`compile_hwpx_labeled`)
  - 파이프라인 순서: bold → parenthesis → **superscript(신규)** → asterisk → linefit.
  - `superscript_changed`를 header.xml 포함 조건에 추가.

### parse (HWPX → 마크다운)
- `src/parse/hwpx.rs`
  - `SuperscriptCharPrs` struct(`sup` = hh:supscript 보유 id, `red` = 그중
    textColor=#FF0000, `is_plain_sup()` = sup && !red).
  - `read_superscript_charprs()` — header.xml의 charPr을 훑어 ID 수집
    (header.xml 없으면 기본값).
- `src/parse/parse.rs`
  - `ParseStats`에 `superscript: SuperscriptCharPrs` 필드(문단·표 셀 파싱에
    문맥으로 전달; 기본값은 빈 목록이라 기존 테스트 시그니처 불변).
  - `paragraph_parts()` — 연속한 평문 윗첨자 run 중 첫 run(텍스트를 실제 담은) 앞에
    `^` 복원. 붉은 별표 변형은 제외.
  - `sections_to_blocks()`이 superscript 인자를 받도록 시그니처 변경(호출 2곳 갱신).
- `src/parse/mod.rs`
  - `hwpx_to_markdown_bytes()`가 `read_superscript_charprs` → `sections_to_blocks` 전달.

### 테스트 (lib.rs)
- `superscript_run_has_supscript_char_pr` — 컴파일 산출 section0.xml에서 윗첨자 run이
  분리되고 그 charPr이 hh:supscript 보유 + textColor != #FF0000.
- `superscript_roundtrip_preserves_caret` — compile → hwpx2md 왕복에서 `^` 보존.

## 테스트 결과
- RED (`sup-red.*`, exit 101): 신규 8개 중 5개 실패(스텁 상태),
  3개 통과(리터럴 케어 경계 1건 + 기존 asterisk 회귀 2건).
- GREEN (`sup-green.*`, exit 0): 신규 8개 전부 통과.
- 전체 스위트 (`sup-full-cargo-test.*`, exit 0): lib 182 passed + e2e 6 passed,
  회귀 0.
- Release 빌드 (`sup-release-build.*`, exit 0).

## CLI 증거
- `docs/verification/legacy-parity/sup-cli-evidence.out`
  - 입력 `/tmp/sup_test.md`의 `이것은^윗첨자입니다.` → section0.xml에서
    `이것은`(charPr 21) / `윗첨자입니다.`(charPr 65) run 분리.
  - header.xml charPr 65 = `<hh:supscript />` 보유, textColor=#000000(평문 윗첨자,
    붉은 별표 변형 아님).
  - hwpx2md 역변환 결과 `- 이것은^윗첨자입니다.`로 `^` 보존 확인.
