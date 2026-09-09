# hwpx (compile & parse)

마크다운과 HWPX 문서 간의 양방향 상호 변환 도구 모음(Rust 단일 프로젝트)이다.
마크다운을 HWPX 보고서로 컴파일하는 `compile`(`md2hwpx`) 모듈과 HWPX를 마크다운으로 역변환하는 `parse`(`hwpx2md`) 모듈이 한 프로젝트에 통합되어, `compile → parse → compile` 왕복 변환 안정성을 공유 엔진(`shared`)으로 보장한다.

## 프로젝트 레이아웃

| 경로 | 역할 |
| --- | --- |
| `src/compile/` | 마크다운 + `template.hwpx` → HWPX 컴파일러 (코어 라이브러리) |
| `src/parse/` | HWPX → 마크다운 파서 및 변환기 (코어 라이브러리) |
| `src/shared/` | 두 엔진이 공유하는 공용 모듈 (XML DOM, 글리프, dialect, error, zip 읽기) |
| `src/bin/md2hwpx.rs` | `md2hwpx` CLI 진입점 바이너리 |
| `src/bin/hwpx2md.rs` | `hwpx2md` CLI 진입점 바이너리 |
| `web/` | 단일 통합 웹 애플리케이션 (2페이지 SPA, Vite + Svelte 5 + Tailwind 4) |
| `web/wasm/` | 웹앱을 위한 단일 결합 Wasm 바인딩 (`compile_hwpx`, `convert_hwpx`) |

Rust 라이브러리로도 사용할 수 있다: `hwpx::compile_hwpx_bytes`, `hwpx::compile_hwpx_labeled`, `hwpx::hwpx_to_markdown_bytes` (`src/lib.rs`에서 re-export).

## 빌드 및 테스트 (CLI)

```sh
# 릴리스 바이너리 동시 빌드 (target/release/md2hwpx, target/release/hwpx2md)
cargo build --release

# 워크스페이스 전체 단위 테스트
cargo test
```

### CLI 실행

```sh
# compile (md2hwpx): 마크다운 소스 + 템플릿 → HWPX 컴파일
./target/release/md2hwpx source.txt template.hwpx compiled.hwpx
# source·template·output은 모두 선택 인자(기본값 source.txt, template.hwpx,
# output 생략 시 문서 제목으로 파일명 자동 생성). 템플릿 변형 선택:
./target/release/md2hwpx 보고서.md --variant title=external

# parse (hwpx2md): HWPX 문서 → 마크다운 변환
./target/release/hwpx2md sample.hwpx sample.md
# output을 생략하면 <입력줄기>.md로 저장한다
```

두 명령 모두 변환 결과를 stdout에 JSON `Report`로 출력하고, `-h`/`--help`와 `-v`/`--version`(빌드 날짜) 옵션을 지원한다. 종료 코드는 정상 0, 경고(validation errors·손상 표 복구 등) 1, 변환 오류 2이며, 오류 메시지는 한 줄 영어(`error: ...`)로 나온다.

주요 소스 방언 토큰: 강조 블록 `===`(위아래로 감싼 목록), 명령 `[[박스]]`, `[[붙임]]`, `[[외부제목]]`, 가로선 `---`(별도 출력 요소는 만들지 않음).

## 웹 개발 (web/)

웹앱은 1개의 애플리케이션 안에 2개의 페이지(`MD2HWPX`, `HWPX2MD`)가 통합된 SPA(Single Page Application)이며, 브라우저에서 100% 로컬(Wasm)로 실행되어 서버로 문서를 전송하지 않는다.

```sh
cd web
npm ci
npm run dev     # 개발 서버 http://localhost:8292/ (포트 8292 단일 실행)
```

- **접속 주소**:
  - `http://localhost:8292/hwpx/compile`: 마크다운 → HWPX 컴파일러 (`md -> hwpx`)
  - `http://localhost:8292/hwpx/parse`: HWPX → 마크다운 역변환기 (`hwpx -> md`)
  - 루트(`http://localhost:8292/` 또는 `/hwpx/`) 접속 시 자동으로 `/hwpx/compile`로 리다이렉트된다.
- 상단 헤더의 `MD2HWPX ⇄ HWPX2MD` 워드마크를 클릭하면 페이지 새로고침 없이 즉시 전환되며, 에디터에 작성 중이던 마크다운 및 변환 상태가 그대로 보존된다.
- **Wasm 재빌드**:
  ```sh
  wasm-pack build web/wasm --target web --release --out-name hwpx_wasm --out-dir ../src/lib/wasm
  wasm-opt -Oz --enable-bulk-memory-opt --enable-nontrapping-float-to-int --enable-sign-ext --strip-debug --strip-producers web/src/lib/wasm/hwpx_wasm_bg.wasm -o web/src/lib/wasm/hwpx_wasm_bg.wasm
  rm web/src/lib/wasm/.gitignore
  ```
  산출물(`web/src/lib/wasm/`)은 저장소에 커밋된다. wasm-pack이 out-dir에 `*`만 담은 `.gitignore`를 자동 생성하므로
  이를 지우지 않으면 산출물이 커밋에서 누락되어 배포 빌드가 실패한다.

## 배포 (Vercel)

루트 `vercel.json` 단일 설정으로 웹앱을 빌드하여 `web/dist`로 배포한다:

- **배포 주소**: `https://md2hwpx.vercel.app/hwpx/compile` (역변환: `/hwpx/parse`)
- **SPA 라우팅**: 모든 서브패스 요청이 `index.html`로 rewrite되어 직접 URL 접근 및 새로고침을 지원한다.
- **캐시 정책**: `/assets/*` 경로는 1년 immutable 캐시가 적용된다.

## 수동 왕복 스모크 검증

커밋된 예제 픽스처를 이용해 양방향 CLI 변환 수렴성을 수동 확인한다:

```sh
./target/release/md2hwpx web/public/example.md web/public/template.hwpx /tmp/rt.hwpx
./target/release/hwpx2md /tmp/rt.hwpx /tmp/rt.md
./target/release/md2hwpx /tmp/rt.md web/public/template.hwpx /tmp/rt2.hwpx
```

위 체인이 에러 없이 종료되고 블록 형상이 손실 없이 유지되는지 점검한다.

## 라이선스

Apache-2.0 (`LICENSE`)
