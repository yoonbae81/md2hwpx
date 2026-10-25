# md2hwpx 컴파일러 아키텍처

마크다운 → HWPX 컴파일 파이프라인의 실제 구조. 모든 함수·타입·파일 경로는
`src/`의 실제 소스 기준이며, 항목마다 출처를 표시했다. 문법 요소별 스키마는
[visual-element-schemas.md](visual-element-schemas.md)에, 사용자용 사용법은
README.md에 있다.

> 이 프로젝트는 별도의 hp 조립 엔진과 통합되어 있지 않다. `md2hwpx`는
> 템플릿 하나(`web/public/template.hwpx`)를 복제해 블록을 채우는
> 템플릿 기반 컴파일러다. hp의 native 조립·문맥별 줄 폭 API는 사용하지
> 않는다. 다만 기존 표 렌더러의 내용 기반 열 너비 재분배는 유지한다.

## 파이프라인 전체

진입점은 `src/compile/mod.rs`의 `compile_hwpx_labeled`(CLI용)와
`compile_hwpx_bytes`(Wasm/에디터용)다. 파일 I/O는 없고 바이트만 주고받는다.

```
source_md ─→ source::parse_source_str_labeled   (title, Vec<Block>)
template  ─→ hwpx::read_template_parts_bytes    (section0/header 루트)
                ↓
          template::build_section               (section0 루트 재구성)
                ↓
          postprocess::apply_markdown_bold      (**굵게** → run 분할)
          postprocess::apply_parenthesis_size   (붙은 괄호 축소)
          postprocess::apply_attached_asterisk  (붙은 별표 → 빨간 윗첨자)
                ↓  (run 재구성이 모두 끝난 뒤)
          linefit::apply_line_fit               (15pt 문단 자간 보정)
                ↓
          xmltree::serialize(section은 항상, header는 바뀐 경우에만)
          hwpx::rewrite_hwpx_bytes              (zip 복제 + section0/header 교체)
          hwpx::validate_hwpx_bytes             → Report { title, block_count, variants, validation }
```

## 1. 소스 해석 — `src/compile/source.rs`

- 공개 타입은 `Block = (String, String)`(종류, 본문)뿐이다. 노드 트리를
  만들지 않는다.
- `parse_source_str_labeled` → BOM 제거, CRLF 통일, `python_splitlines`로
  줄 분리 → `parse_source`가 `SourceParser` 상태 기계로 줄을 판별한다.
- `SourceParser`의 핵심 상태: `title`/`fallback_title`(제목 후보),
  `bullet_items`(불릿 자리: 블록 인덱스·들여쓰기·글리프),
  `box_open`(열린 박스), `highlight_delimiters`(하이라이트 경계 쌍).
- 블록 종류는 `blocks.push` 호출에서 확인할 수 있다: `title-variant`/
  `title-text`(명령 줄), `heading`/`heading_sub`/`heading_default`,
  `depth1~3`(끝에 `finish_bullets`가 종류를 확정), `asterisk`, `reference`,
  `box`, `highlight`, `table`. 표·박스·하이라이트 값은 JSON으로 인코딩된다.
- 불릿 깊이는 상대 순위다: `finish_bullets`가 들여쓰기 값을 오름차순으로
  세워 가장 얕은 것을 depth1로 매기고(최대 depth3), "보이는 하위 항목"
  글리프(`○`/`ㅇ`, 본문에 남는 글리프)는 같은 들여쓰기 형제 아래에서 한
  단계 더 깊게 본다.
- 하이라이트 `---`와 가로선 `---`는 같은 문자열(`dialect::HIGHLIGHT` ==
  `dialect::HRULE`)을 쓰며, 5줄 창 규칙(`HIGHLIGHT_WINDOW`, 짝 사이 5줄
  이내 + 빈 줄만 있지 않음)로만 구별한다. 문서 첫 줄의 `---`는 절대
  하이라이트 경계가 아니다.

## 2. 템플릿 카탈로그와 section 재구성 — `src/compile/template.rs`

템플릿은 HWP 메모(`hp:fieldBegin type="MEMO"` ~ `hp:fieldEnd`)로
prototype을 표시한다. 표식 텍스트는 `patterns.rs`의 `TEMPLATE_MARKER_RE`
(`[[category]]`, `[[category:component]]`)로 읽는다.

- `read_template_catalog` → `Catalog { title, prototypes, components }`.
  - `title` 계열: 변형 문단(`[[title:_default]]`,
    `[[title:attachment]]`, `[[title:external]]`).
  - `heading` 계열: `heading_default`는 `memo_marker`가 `heading:default`로
    정규화되어 prototype 키 `heading_default`가 된다. `[[heading:sub]]`는
    `heading_sub`가 된다.
  - 구성요소 메모(`[[depth1:bullet]]`, `[[box:title]]` 등)는 run의
    charPrIDRef를 얻는 데만 쓰인다.
- `build_section` 순서: 카탈로그 읽기 → `normalize_blocks`(`major` →
  `heading`) → prototype 복제(`deep_copied_prototypes`) → 간격 접두 복제
  (`interval_prefixes`) → title 변형 선택·삽입(`install_title`) → 표지 셀에
  제목 기록(`write_title_cell`) → 제목 폴백(`apply_heading_fallback`) →
  필요 prototype 확인(`ensure_prototypes_exist`) → 메모 표식 제거
  (`remove_variant_markers`) → 미사용 직계 문단 정리(`prune_direct_paragraphs`)
  → 구성요소 스타일 수집(`collect_component_styles`) → `render_blocks`.
- 렌더링은 블록마다 간격 접두 문단 + prototype 복제본을 section0 끝에
  붙이는 방식이다(`render_blocks`). 기본 서식은 prototype/MEMO에서 오지만,
  표 열 너비는 내용에 따라 재분배되며 최소 폭 계산에는 4000 unit 상수가
  쓰인다. 줄 맞춤의 경계도 별도 상수다. 모든 기하 값의 원본 보존을 뜻하지 않는다.
- `replace_template_dates`가 표지 날짜를 `today_korean()`으로 바꾼다.

## 3. 블록 렌더러 — `src/compile/render.rs`

`render_block`이 종류별로 분기한다. 공통 유틸: `SAMPLE_TEXT`(상수
`"샘플"`) 자리 치환, `ensure_run_count`, `invalidate_layout_cache`
(줄배치 캐시 `hp:linesegarray` 제거).

- `heading_default`/`heading_sub`: prototype 텍스트의 `#.`과 `샘플`을 번호·본문으로
  교체. `**굵게**` 표식은 `bold_sub`로 벗겨 내므로 이 경로에서는
  해당 표식이 후처리의 부분 굵게 서식으로 전달되지 않는다.
  `heading_sub`는 표 prototype이면 셀의 `샘플`을 교체하고 번호를
  본문에 보존한다.
- 파서의 `heading` 블록은 `apply_heading_fallback` 조건(전용 heading
  prototype이 없고 heading_default가 있음)을 만족할 때 heading_default로
  바뀐다. `render_block`에는 bare `heading` 분기가 없어, 폴백 없이 도달하면
  `unsupported block kind` 오류가 난다. 이번 정리에서 동작을 확장하지 않았다.
- `render_labeled_item`(depth1~3): 문단을 5 run으로 나눠 글머리/간격/
  `(라벨)`/간격/본문을 채운다. 출력 불릿은 `glyph::split_marker`가
  결정한다(원문자 번호는 번호가 불릿, 주석 글리프는 불릿 없음, 나머지는
  템플릿 불릿).
- `set_note_item`(asterisk/reference): 3 run으로 글머리/간격/본문.
- `render_box`: 박스 표의 첫 `샘플` 자리에 제목, 항목 문단을
  `fill_box_item`으로 채우고 항목 수만큼 복제해 `hp:subList`에 끼운다.
- `render_highlight`: 항목 하나당 셀 문단 하나(경계 보존). 표 prototype이
  아니면 `render_single_run`으로 한 문장으로 합친다.
- `render_table`: prototype의 1행(헤더)·2행(바디)을 템플릿으로 삼아
  입력 행 수만큼 복제, 열 수에 맞춰 셀을 늘리고/준다.
  `prepare_column_widths`가 셀 너비를 내용에 따라 재분배하고
  (`optimize_column_widths`), `fill_cell`이 주소/너비/본문을 채운다.
  병합 셀(rowSpan/colSpan)은 만들지 않는다 — 파이프 표에 병합은 없다.

## 4. run 후처리 — `src/compile/postprocess.rs`

`RunRestyler` 트레이트(`scans_run`/`is_mixed_run`/`split_node`/
`derive_ids`/`restyle_mixed_run`)을 공유하는 3단계다. 공통 엔진
`apply_run_restyler`가 일치 구간을 찾아 `split_run`으로 run을 자르고
파생 charPr을 배정한다.

| 순서 | restyler | 파생 charPr |
|---|---|---|
| 1 | `MarkdownBold` — `**굵게**` 구간 | `bold_char_pr_ids`: `hh:bold` 추가 |
| 2 | `AttachedParenthesis { delta: -200 }` — 앞 글자에 붙은 괄호 | `scaled_char_pr_ids`: height −200 |
| 3 | `AttachedAsterisk` — 붙은 `*` | `superscript_red_char_pr_ids`: `textColor=#FF0000` + `hh:supscript` |

파생 charPr은 `derived_char_pr_ids`가 시그니처(id 속성을 제외한
직렬화, `char_pr_signature`)로 중복 생성을 막는다. 새 charPr이
charProperties에 추가되면 `header.xml`을 다시 쓴다. 붙은 별표(`*`의
의미 변화)가 run 분할과 섞이지 않도록 이 3단계가 모두 끝난 뒤에
`linefit`이 실행된다.

## 5. 줄 맞춤 — `src/compile/linefit.rs`

`apply_line_fit`은 15pt(휴먼명조, charPr height=1500) 문단이 한 줄
폭(`LINE_WIDTH_UNITS` = 48188, 상수)을 1~6글자(`MAX_OVER_CHARS`) 넘쳤을
때 자간을 줄여 줄 수를 유지한다. 폭 계산은 `metrics::text_width`
(HWP unit 실측 테이블). 15pt id 집합과 charPr 변형 캐시는 루프 밖에서
한 번 구축하고, base id 순서는 run 등장 순서를 유지해 결정적 출력을
보장한다.

## 6. 공용 규칙 — `src/shared/`

- `glyph.rs`: `BULLET_GLYPHS`와 `GlyphRole { Bullet, CircledNumber, Note }`
  분류의 단일 출처. `☞`는 Note(본문에 남고 불릿은 비움), ①~⑳는
  CircledNumber(번호가 출력 불릿)다.
- `dialect.rs`: 소스 문법 토큰 상수(`HIGHLIGHT`, `HRULE`, `BOX`, `ATTACH`,
  `EXTERNAL`, `ATTACH_VARIANT`, `EXTERNAL_VARIANT`). `patterns.rs`의
  `dialect_tokens_are_documented` 테스트가 README/prompt.md/example.md와의
  드리프트를 감시한다.
- `xmltree.rs`: quick-xml 기반 최소 DOM. 자식은 요소뿐, 텍스트는
  `text`/`tail`. fwSpace는 파싱 시점에 일반 공백으로 흡수된다.
- `zip_read.rs`: 패키지 크기 검사, DTD 거부, section 번호 정렬.
- `error.rs`: 한 줄 영어 오류 메시지용 `AppError`.

## 7. 패키지 I/O와 검증 — `src/compile/hwpx.rs`

- `read_template_parts_bytes`: zip 크기 검사 + DTD 거부 후
  `Contents/section0.xml`과 `Contents/header.xml`만 파싱해 돌려준다.
- `rewrite_hwpx_bytes`: 템플릿 패키지를 항목 단위로 복제하며 section0과
  (바뀐 경우에만) header를 새 XML로 교체한다. mimetype 등 나머지 항목은
  원본 그대로다.
- `validate_hwpx_bytes`: mimetype 첫 항목·STORED 검사, 필수 항목
  (`HWPX_REQUIRED_PARTS`) 확인, charPrIDRef ↔ charPr id 교차 검증,
  섹션 내용 스캔 → `Validation { format, size_bytes, para_count, sections,
  title, warnings, errors }`. 종료 코드 규약은 두 CLI가 같다: 0=성공
  (warnings만 있으면 0이고 경고는 stdout JSON `Report`로 노출된다),
  1=검증 errors와 컴파일/I-O 등 처리 실패, 2=사용법·인자 오류다.
  warnings/errors 상세는 `Report`로 노출된다.

## 8. CLI와 Wasm

- `src/bin/md2hwpx.rs`(CLI 엔트리) → `src/compile/cli.rs`(인자 해석,
  변형 `--var key=value`, 충돌 검사) → `compile_hwpx_labeled`.
- 웹은 `web/wasm` 크레이트가 같은 `compile_hwpx_bytes`를 노출한다
  (`web/src/lib/wasm.ts`의 `ensureWasm`).
