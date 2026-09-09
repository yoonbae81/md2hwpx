# AGENTS.md

hwpx (compile & parse) — 마크다운 ⇄ HWPX 상호 변환 Rust 단일 프로젝트.
마크다운을 HWPX 보고서로 컴파일하는 `compile`(CLI `md2hwpx`)과 HWPX를 마크다운으로 역변환하는 `parse`(CLI `hwpx2md`)가
한 프로젝트 내 `src/compile`, `src/parse` 모듈로 통합되어 있으며, 공용 코드는 `src/shared` 모듈로 공유하고 웹은
단일 앱 2페이지 SPA(`web/`)로 통합되어 같은 Wasm 엔진을 공유한다. 동작을 바꿀 때는
기존 출력과의 호환성과 단위 테스트가 기준이다. 사용법과 파이프라인 개요는 README.md가 기준이다.

## 명령

```sh
cargo build --release   # target/release/md2hwpx 및 target/release/hwpx2md
cargo test              # 워크스페이스 전체 단위 테스트
./target/release/md2hwpx workspace/source.txt workspace/template.hwpx workspace/compiled.hwpx   # md2hwpx 수동 검증
./target/release/hwpx2md workspace/sample.hwpx workspace/sample.md                              # hwpx2md 수동 검증

cd web && npm run dev   # 통합 개발 서버 http://localhost:8292/hwpx/compile 및 /hwpx/parse (포트 8292)
cd web && npm run build # web/dist 정적 산출물 빌드
```

- 웹용 공유 Wasm 바인딩은 `web/wasm` 크레이트. 재빌드:
  `wasm-pack build web/wasm --target web --release --out-name hwpx_wasm --out-dir ../src/lib/wasm` 뒤에
  `wasm-opt -Oz --enable-bulk-memory-opt --enable-nontrapping-float-to-int --enable-sign-ext --strip-debug --strip-producers web/src/lib/wasm/hwpx_wasm_bg.wasm -o web/src/lib/wasm/hwpx_wasm_bg.wasm` 뒤에
  `rm web/src/lib/wasm/.gitignore`.
  (wasm-pack 내장 wasm-opt는 비활성화되어 있고, 산출물 `web/src/lib/wasm/`은 커밋된다. wasm-pack이 out-dir에 `*`만 담은
  `.gitignore`를 자동 생성하므로 지우지 않으면 산출물이 커밋에서 누락되어 Vercel 빌드가 깨진다.)
- Wasm 타겟에서는 `regex`가 regex-lite로 교체된다(루트 크레이트 `Cargo.toml`의 타겟별 의존성). regex-lite 서브셋(리터럴 한글, `(?i)` ASCII, `\d`/`\s`, named group)을 벗어나지 않도록 한다.
- 저장소 루트의 `template.hwpx`, `source.txt` 및 수동 검증용 `.hwpx` 파일은 `workspace/` 아래에 두며, 이 디렉터리 전체가 `.gitignore`로 제외되어 있다. 픽스처 바이너리를 커밋하지 않는다.

## 파일별 책임

### Rust 크레이트 코어 (`src/`)
- `src/lib.rs` — 최상위 라이브러리 인터페이스 (compile/parse API re-export).
- `src/shared/xmltree.rs` — 최소 XML DOM(text/tail 모델, quick-xml 기반).
- `src/shared/glyph.rs` — 글리프(불릿 기호) 규칙의 단일 출처.
- `src/shared/dialect.rs` — 소스 방언 토큰 7상수 완전판(HIGHLIGHT, HRULE, BOX, ATTACH, EXTERNAL, ATTACH_VARIANT, EXTERNAL_VARIANT).
- `src/shared/error.rs` — 한 줄 오류 메시지용 `AppError`.
- `src/shared/zip_read.rs` — HWPX zip 패키지 크기 검사, DTD 거부, 섹션 번호순 정렬 및 테스트 픽스처(`test_fixtures::fixture_hwpx`).
- `src/compile/` — 마크다운 → HWPX 컴파일 코어:
  - `mod.rs` — 인메모리 공개 API(`compile_hwpx_bytes`, `compile_hwpx_labeled`, `parse_source_str`).
  - `source.rs` — 마크다운 소스 구문 해석(`parse_source`).
  - `template.rs` — MEMO 표식 카탈로그, 변형 선택, `build_section`.
  - `render.rs` — 블록별 렌더러(heading/depth/note/box/highlight/table).
  - `patterns.rs` — 공용 정규식 모음과 방언 토큰 문서화 검증 테스트(`dialect_tokens_are_documented`, README/prompt.md/example.md 드리프트 감시).
  - `postprocess.rs` — 컴파일 후처리(굵게, 괄호 축소, 붙은 별표).
  - `hwpx.rs` — zip 패키지 쓰기/검증(`validate_hwpx_bytes`, `Report`).
  - `cli.rs` — md2hwpx CLI 인자 해석 및 충돌 검사.
- `src/parse/` — HWPX → 마크다운 역변환 코어:
  - `mod.rs` — 인메모리 공개 API(`hwpx_to_markdown_bytes`).
  - `dialect.rs` — 공용 방언 토큰 re-export shim(parse가 내보내는 토큰과 compile 소스 파서의 정합성 보증).
  - `hwpx.rs` — HWPX 패키지 읽기(섹션 정렬).
  - `parse.rs` — HWPX 파싱 코어. MEMO 제외 짝 맞춤(`beginIDRef` = `id`), 표 직렬화.
  - `postprocess/` — 문서 관습 복원 규칙(붙임 표지 → 제목 바 → 소제목 → 날짜 제거 → 박스 → 번호 소제목 → 불릿 2패스 → 각주).
  - `report.rs` — hwpx2md CLI가 stdout에 출력하는 JSON `Report`.
  - `cli.rs` — hwpx2md CLI 인자 해석 및 충돌 검사.
- `build.rs` — 빌드 날짜(`BUILD_DATE`) 주입과 Windows 실행 파일 버전 리소스 포함.
- `src/bin/md2hwpx.rs` — md2hwpx CLI 실행 엔트리포인트.
- `src/bin/hwpx2md.rs` — hwpx2md CLI 실행 엔트리포인트.

### 웹 및 바인딩 (`web/`)
- `web/wasm/` — 결합 Wasm 바인딩 크레이트. `compile_hwpx`와 `convert_hwpx`를 노출.
- `web/src/App.svelte` — 단일 웹앱 진입점. URL 감지 및 2페이지 SPA 전환(`M2hPage`, `H2mPage`) 관리.
- `web/src/pages/M2hPage.svelte` — md2hwpx 웹 화면 (에디터, 지침/템플릿 시트).
- `web/src/pages/H2mPage.svelte` — hwpx2md 웹 화면 (드롭/에디터 textarea, 변환 결과 표시, 듀얼 푸터).
- `web/src/components/SwapHeader.svelte` — 두 페이지 간 즉시 전환 스왑 헤더 컴포넌트.
- `web/src/lib/` — 웹 유틸 및 엔진:
  - `wasm.ts` — 단일 Wasm 엔진 로딩 헬퍼(`ensureWasm`).
  - `theme.svelte.ts` — 테마 순환(시스템/라이트/다크)과 두 페이지가 같이 읽는 반응형 모듈 `$state`.
  - `clipboard.ts` — 클립보드 복사 유틸.
  - `sanitize.ts` — 파일명 정규화(`sanitizeFilename`).
  - `storage.ts` — 초안 및 템플릿 로컬 저장소.
  - `instruction.ts` — 지침 마크다운 렌더러.
  - `download.ts` — HWPX/마크다운 파일명 빌더 및 다운로드 실행.
- `web/public/` — 번들 자산(`template.hwpx`, `prompt.md`, `instruction.md`, `example.md`).

## 공용 코드 및 컴포넌트 단일화

- **src/shared 모듈로 단일화**: 이전의 파일 수동 동기화 의무는 폐지되었다. `xmltree`, `glyph`, `dialect`, `error`, zip 읽기 헬퍼는 모두 `src/shared` 모듈이 단일 출처이며, 모듈 간 정합성은 `cargo test`가 빌드/테스트 시점에 자동으로 검증한다.
- **단일 웹앱 2페이지 구조**: 별도 앱 분리 배포를 폐지하고 `web/` 단일 앱에서 2개 페이지를 호스팅한다. `SwapHeader` 워드마크 클릭 시 SPA 라우팅으로 즉시 전환되며 작성 중인 문서 상태가 유지된다.

## 변경 시 지킬 규칙

- XML 요소 비교는 접두사가 아니라 네임스페이스 URI(`HP`, `HH`)로 한다. 요소 동일성은 `Rc::ptr_eq`.
- 트리 모델은 Python ElementTree와 같다: 자식은 요소뿐, 텍스트는 `text`/`tail`. fwSpace는 파싱 시점에 일반 공백으로 흡수된다.
- `parse`의 `collect_paragraphs`는 절대 `hp:tbl` 내부로 내려가지 않는다. 표는 `paragraph_parts`가 그 자리에서 직렬화한다.
- 한국어 길이 상한(소제목 80자, 번호 소제목 본문 60자)은 문자 수(`chars().count()`)로 센다.
- 후처리가 내보내는 토큰(`[[박스]]`, `[[붙임]]`, `- ` 불릿, `N. 제목`)은 `compile` 소스 파서가 다시 먹는 토큰과 정합해야 한다.
- 스타일 값을 코드에 하드코딩하지 않는다. `compile`의 레이아웃은 `template.hwpx`의 prototype/메모 범위에서 온다.
- 종료 코드 규약: 컴파일/변환 오류 2, 경고(validation errors 또는 손상 표 복구) 1, 정상 0. 오류 메시지는 한 줄 영어(`error: ...`)로 유지하고 파일 경로는 입력 원문 그대로 출력한다.

## 검증 습관

- 규칙 회귀는 `cargo test`의 워크스페이스 단위 테스트가 지킨다.
- 수동 왕복 스모크:
  `./target/release/md2hwpx web/public/example.md web/public/template.hwpx /tmp/rt.hwpx`
  `./target/release/hwpx2md /tmp/rt.hwpx /tmp/rt.md`
  `./target/release/md2hwpx /tmp/rt.md web/public/template.hwpx /tmp/rt2.hwpx`
  로 블록 종류 손실이나 오류가 없는지 확인한다.
- `workspace/` 산물은 커밋하지 않는다.
