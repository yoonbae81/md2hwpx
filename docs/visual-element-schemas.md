# md2hwpx 시각 요소 스키마

md2hwpx(`compile` 모듈)가 소스에서 인식하는 블록 종류와 판정 규칙.
모든 항목은 실제 소스 기준이며 출처 파일을 표시했다. 파이프라인 전체는
[compiler-design.md](compiler-design.md)에, 사용자용 문법 문서는
README.md와 `web/public/prompt.md`에 있다.

## 블록 종류

`source.rs`의 `SourceParser`가 내보내는 `Block = (String, String)` 종류.

| 종류 | 소스 구문 | 값 형식 | 렌더러 |
|---|---|---|---|
| `title-variant` | `[[붙임]]` / `[[외부제목]]` | 변형 이름(`attachment`/`external`) | — (title 변형 선택) |
| `title-text` | `[[붙임]] 제목` 이어 쓴 텍스트 | 셀 대체 텍스트 | — (표지 셀 기록) |
| `heading` | `## 1. 제목`, 평문 `1. 제목` | 원문 줄 | 조건부 heading_default 폴백 후 `heading()` |
| `heading_sub` | `### 아무 텍스트`, `2.1 소제목` | 원문 줄(번호 보존) | `heading_sub()` |
| `heading_default` | `## 일반 제목`, `#### 4단 제목` | 원문 줄 | `heading()` |
| `depth1`~`depth3` | 불릿 항목(깊이는 상대 순위) | 항목 본문 | `render_labeled_item()` |
| `asterisk` | 들여쓴 `* 보조표기`, `- * …`, `- \* …` | 본문 | `set_note_item()` |
| `reference` | `※ 각주`(들여쓰기 무관), `- ※ …` | 본문 | `set_note_item()` |
| `box` | `[[박스]] 제목` + 들여쓴 불릿 항목 | JSON 행 배열 | `render_box()` |
| `highlight` | `---` 한 쌍 사이의 목록 | JSON 문자열 배열 | `render_highlight()` |
| `table` | 파이프 표(헤더+구분선 쌍) | JSON 행 배열 | `render_table()` |

문서 제목은 특수 블록이 아니라 파서 반환값의 `title`이다: 맨 첫 비목록
일반 줄, `# 제목`, `[제목]`/`제목:` 표기 중 가장 먼저 나온 것이 제목이
되고, 전부 없으면 첫 비어있지 않은 줄에서 불릿 글리프를 벗겨 폴백으로
쓴다(`try_markdown_heading`/`try_title_marker`/`parse` 말단).

## 헤딩 판정 (`source.rs`)

- `#`(1단)은 제목이 이미 있으면 일반 본문으로 흘려보낸다.
- `##`/`###`/`####`는 `markdown_core`로 `**굵게**` 한 겹을 벗긴 뒤:
  `###`(3단) 또는 `N.M` 복합 번호(`SUB_NUM_RE`) → `heading_sub`,
  `##` + `N.` 단일 번호(`NUM_RE`) → `heading`, 그 외 → `heading_default`.
- 평문 줄도 같은 순서로 `N.M`/`N.`을 검사한다(`try_numbered_heading`).
  `heading`의 첫 본문은 제목 폴백 후보가 된다.
- `heading_default`는 `[[heading]]`/`[[heading:default]]` 메모가 같은
  prototype 키로 정규화된다(`memo_marker`).
- `render_block`은 bare `heading`을 직접 처리하지 않는다. 전용 heading
  prototype이 없고 heading_default가 있을 때만 `apply_heading_fallback`이
  블록을 재지정한다. 폴백 없이 도달하면 오류이며 범용 heading 지원을 뜻하지 않는다.

## 불릿 깊이와 글리프 (`source.rs`, `glyph.rs`)

- 들여쓰기 폭: 공백 1칸 = 1, 탭 = 4칸 탭 스톱(`indent_width`).
- 깊이 확정은 문서 끝에 한 번: 서로 다른 들여쓰기 값의 오름차순 상대
  순위가 depth1~3(최대 3단, `finish_bullets`).
- "보이는 하위 항목" 글리프(`○`/`ㅇ`, 원문자 번호, 주석 글리프 —
  `is_visible_sub_item`): 같은 들여쓰기의 다른 기호 바로 아래에서 오면
  한 단계 더 깊게, 연달아 오면 같은 단계를 유지한다.
- `BULLET_GLYPHS`(일반 불릿): `- * + ○ ㅇ ◯ ● ◦ • · ∙ ∘ ▪ ▫ ‣ ⁃ □ ■ ◆ ◇ ▶ ▷ ⊙ ☞`.
  단 `☞`는 `GlyphRole::Note`로 분류된다 — **일반 불릿이 아니며** 출력
  불릿을 비우고 본문 앞에 남는다. `*`는 뒤에 공백이 올 때만 목록으로
  본다(`**굵게**`와 구별, `bullet_split`).
- `GlyphRole`: `Bullet`(템플릿 불릿으로 정규화, 본문에 남지 않음),
  `CircledNumber`(①~⑳, 번호가 곧 출력 불릿, 본문에 남음),
  `Note`(☞, 출력 불릿 없음, 본문에 남음). 본문에 남는 글리프의 출력
  결정은 `split_marker`가 한다.

## 노트(주석/보조표기) (`source.rs` `handle_body_line`, `render.rs`)

- `※`로 시작하는 줄은 들여쓰기와 무관하게 `reference`다.
- `- ※ 본문`은 `reference`, `- * 본문`/`- \* 본문`은 `asterisk`
  (`NOTE_MARKER_RE`). 분류 기준은 본문 끝의 별표가 아니라 앞쪽 주석
  표식이다. `- ※ 본문*`는 `reference`이며 붙은 별표는 후처리 대상이다.
- 들여쓴 `*`(`indent > 0`)는 명시적 보조표기 `asterisk`다.
- 렌더링(`set_note_item`)은 3 run(글머리/간격/본문)이고 글머리 자리에는
  템플릿 `[[asterisk:bullet]]`/`[[reference:bullet]]` 메모의 불릿 문자가
  온다.
- 이후 `postprocess::apply_attached_asterisk`가 글자에 붙은 `*`를 빨간
  윗첨자(`textColor=#FF0000` + `hh:supscript`) run으로 분할한다.

## 박스 (`source.rs`, `render.rs`)

- 명령 줄: `[[박스]]`/`[[box]]`(단일 괄호와 `**` 감쌈도 허용,
  `BOX_RE`). 제목이 JSON 첫 행이 된다.
- 바로 아래의 더 깊은 들여쓰기 불릿 항목을 모아(`try_box_item`,
  `close_box`) 행 배열로 갈아끼운다. 항목 본문은 `bullet_item_body`
  규칙을 적용한다.
- `render_box`: 박스 표의 첫 `샘플` 자리에 제목, 항목 문단을 항목
  수만큼 복제해 `hp:subList`에 끼운다. 항목이 없으면 견본 문단을 비운다.

## 하이라이트 (`source.rs` `SourceParser::new`, `handle_delimiter_line`)

- `---`는 가로선과 같은 문자열이다. 닫는 `---`가 열린 `---`에서 5줄
  이내(`HIGHLIGHT_WINDOW`)이고 사이에 빈 줄만 있지 않으면 하이라이트
  경계 쌍이고, 그 밖의 `---`는 가로선(무시)으로 남는다.
- 문서 첫 줄의 `---`는 하이라이트 경계가 될 수 없다(front matter
  펜스/가로선 전용). 짝이 없는 일반 `---`는 무시된다. 내부 상태가
  하이라이트를 연 채로 끝나면 `unterminated highlight block` 오류를 낸다.
- 항목은 `- `/`* `/`※ ` 접두를 벗긴 본문의 JSON 배열로 인코딩되고,
  `render_highlight`가 항목 하나당 셀 문단 하나를 만들어 경계를
  보존한다. 표 prototype이 없는 템플릿이면 한 문장으로 합친다
  (`render_single_run`).

## 표 (`source.rs` `try_table`, `render.rs` `render_table`)

- 파이프 표는 헤더 행 + 구분선 행(`:---:` 등 `TABLE_SEP_CELL_RE`) 쌍이
  조건이다. 2열 이상, 열 수가 행마다 같아야 하고, 이후 `|` 포함 연속
  행을 데이터로 모은다. 셀 병합 표기는 문법에 없다.
- `render_table`: prototype 1행을 헤더 템플릿, 2행을 바디 템플릿으로
  입력 행 수만큼 복제한다. 열이 템플릿보다 많으면 마지막 셀 너비를
  나눠 셀을 추가하고(`prepare_column_widths`), 적으면 뒷 셀을 제거한다.
  남는 템플릿 문단은 비운다(`fill_cell`).

## 굵게 (`render.rs` `bold_sub`, `postprocess.rs`)

- `heading()`/`heading_sub()` 경로는 `bold_sub`로 표식을 벗긴다.
  이 경우 표식에 의한 부분 굵게 정보는 후처리에 전달되지 않는다.
- 표지 제목(`write_title_cell`)과 박스 제목(`render_box`)은 텍스트를
  그대로 넣는다. 해당 경로의 유효한 `**…**` 표식은 뒤의 굵게 후처리가 처리한다.
- 항목 본문처럼 run 단위로 남는 텍스트의 `**…**`는 후처리
  `apply_markdown_bold`가 run을 분할하고 `hh:bold`가 추가된 파생
  charPr을 배정한다. 이때 만들어진 새 charPr은 시그니처가 같으면
  재사용된다(`derived_char_pr_ids`).

## 템플릿 MEMO 표기 (`template.rs`, `patterns.rs`)

- prototype 표식은 템플릿 안의 `hp:fieldBegin type="MEMO"` ~
  `hp:fieldEnd` 범위이고, 표식 텍스트는 `TEMPLATE_MARKER_RE`로 읽는다:
  `[[title|heading|heading_default|depth1|depth2|depth3|asterisk|reference|box|highlight|table(:component)?]]`.
- prototype 키: `heading` 메모는 이름을 붙여 변형을 만든다
  (`heading_default` = `[[heading]]`/`[[heading:default]]`/`[[heading_default]]`,
  `heading_sub` = `[[heading:sub]]`). 이름 없는 `[[box]]`는 표 전체
  prototype이고 `[[box:title]]`/`[[box:bullet]]`/`[[box:body]]`는
  구성요소 run 표식이다.
- 필수 구성요소(`component_requirements`): depth1~3은
  `bullet`/`keyword`/`body`, asterisk/reference는 `bullet`/`body`,
  box는 `title`/`bullet`/`body`. 각 구성요소 run은 `charPrIDRef`를
  가져야 한다(`component_styles`).
- `[[*:bullet]]` 메모 run의 첫 비공백 문자가 출력 불릿 문자
  (`bullet_text`)다.
- title 변형 이름은 이 템플릿 기준 `_default`(일반), `attachment`
  (`[[붙임]]`), `external`(`[[외부제목]]`)이다
  (`dialect::ATTACH_VARIANT`/`EXTERNAL_VARIANT`, 표기는
  `[[title:_default]]` 등). 요청이 없으면 템플릿 순서상 첫 변형이
  선택된다(`select_template_variant`).

## run 후처리 순서 (`mod.rs`, `postprocess.rs`, `linefit.rs`)

컴파일 마지막에 순서대로 적용된다. 처음 3개만 `RunRestyler` 구현이고
`apply_line_fit`은 별도 모듈이다.

1. `apply_markdown_bold` — `**굵게**` → bold 파생 charPr + run 분할
2. `apply_parenthesis_size` — 앞 글자에 붙은 괄호 → height −200 (2pt) 축소
3. `apply_attached_asterisk` — 붙은 `*` → 빨간 윗첨자
4. `linefit::apply_line_fit` — 15pt 문단 줄넘침(1~6글자) 자간 보정

## hwpx2md 역변환 대응 (`src/parse/`)

컴파일 산물은 `parse::hwpx_to_markdown_bytes`로 역변환된다.
공용 후처리 `postprocess_markdown`(`src/parse/postprocess/mod.rs`)의
규칙 순서는 계약이다: 붙임 표지 표 → 장표 제목 바 표 → 소제목 1×1 표 →
표지 날짜 제거 → 박스/하이라이트 복원(`box.rs`) → 번호 소제목 승격 →
불릿 2패스(`bullets.rs`) → 각주 asterisk 들여쓰기(`footnote.rs`) →
빈 줄 컴팩션(`spacing.rs`). 데이터 표는 병합 셀(colSpan/rowSpan)을
보존한 HTML `<table>`로, 단순 표는 파이프 표로 배출된다
(`parse/parse.rs` `table_to_markdown`).

## 확장 가이드

새 블록 종류를 추가할 때 실제로 손대는 곳:

1. `source.rs` — 줄 판별(`handle_body_line` 또는 새 `try_*`)과 블록 push
2. `template.rs` — `component_requirements`에 필수 구성요소 등록
3. `render.rs` — `render_block` 분기와 렌더러 함수
4. `patterns.rs` — 새 MEMO 표기가 필요하면 `TEMPLATE_MARKER_RE`에 category 추가
5. 글리프가 관련되면 `shared/glyph.rs` — `BULLET_GLYPHS`/`classify` 한 줄 추가
6. `#[test]`로 파서+렌더러 왕복 검증 — 규칙 회귀는 `cargo test`가 지킨다
