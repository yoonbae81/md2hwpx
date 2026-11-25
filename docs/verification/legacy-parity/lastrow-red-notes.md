# 표 마지막 행 SOLID 아래 테두리 — RED 증거 노트

- `render_table`(src/compile/render.rs:578)은 모든 데이터 행을 `body_template = template_rows[1]`(중간 원형, 777)만 클론하므로, 원형의 마지막 tr(아래 SOLID 테두리, 999)이 어떤 출력 행에도 쓰이지 않고 사라진다.
- 두 테스트 모두 단언 실패로 RED: `render_table_last_data_row_uses_bottom_prototype`는 [11,777,777] vs 기대 [11,777,999], `render_table_single_data_row_uses_bottom_prototype`는 [11,777] vs 기대 [11,999]로 떨어졌다(컴파일 에러 아님, exit 101).
- hp 참조(src/hp_blocks.rs fill_table, 862-891행)는 마지막 데이터 행(N==1 포함)에 원형의 마지막 tr을, 나머지에 template_rows[1]을 쓰므로 GREEN 구현 시 이 규칙으로 render_table을 고친다(이번 슬라이스는 구현 금지).
