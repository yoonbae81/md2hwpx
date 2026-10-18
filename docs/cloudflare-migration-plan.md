# Cloudflare 이전 계획 및 완료 기준

상태: dev/design 역할의 코드 기반 병렬 검토를 반영한 실행 계획. 운영 배포 승인이나 이전 완료를 뜻하지 않는다.
기준 코드: main `15f0a2c`. 대상: Vercel에서 제공하는 `web/` 정적 웹앱.

## 1. 목표와 범위

- Cloudflare Workers Static Assets로 호스팅을 이전한다. Markdown → HWPX 및 HWPX → Markdown 변환은 기존 브라우저 WASM으로 유지한다.
- UI 재설계, Rust 엔진 변경, 서버 변환 API, Cloudflare Workflows/Queues/D1/R2 도입은 제외한다.
- `xcv.kr/hwpx/compile`, `/hwpx/parse` 유지가 권장안이다. 현재 gateway 구현/관리 레포/라우팅은 아직 확인하지 않았으므로 운영 전환 전 확인을 필수 게이트로 둔다.
- 별도 workers.dev 주소에서 먼저 파일럿을 검증한다. 파일럿에서 사용한 저장소는 운영 origin으로 이전되지 않는다.
- 이번 산출물은 계획 문서다. DNS, gateway, Vercel, Cloudflare 운영 설정은 변경하지 않는다.

## 2. 확인한 현황

- `README.md`, `AGENTS.md`: 두 화면 단일 SPA, 브라우저 내 변환, 커밋된 WASM 사용.
- `web/vite.config.ts`: base `/hwpx/`, outDir `dist/hwpx`. 최종 파일은 `web/dist/hwpx/index.html`, `/hwpx/assets/*`, `/hwpx/template.hwpx` 형태다.
- `vercel.json`: 배포 루트 `web/dist`, 두 화면 fallback은 `/hwpx/index.html`, 레거시 주소 리다이렉트와 CSP/보안 헤더 및 hashed assets 1년 immutable 설정.
- `web/src/App.svelte`: history API로 화면 전환, 두 화면을 숨김 처리하여 인메모리 상태 유지. 직접 진입과 popstate 지원.
- `web/src/lib/wasm.ts`: 공유 초기화 및 초기화 실패 후 재시도 지원.
- `web/package.json`: build/dev/preview만 존재한다. `npm test`는 현재 없으므로 완료 기준으로 임의 사용하지 않는다.
- 추적된 `.github` workflow, Wrangler 설정은 이번 확인에서 없었다.

## 3. 배포 구조와 핵심 위험

```text
GitHub → 검증/웹 빌드 → Workers Static Assets → 사용자 브라우저(WASM 변환)
xcv.kr 기존 gateway → /hwpx 경로만 Cloudflare 대상 전환(구현 확인 후)
```

### 경로 보존

Cloudflare SPA fallback의 기본 대상은 자산 루트 `/index.html`이다. 현재 파일은 `/hwpx/index.html`이므로 `assets.directory=web/dist`와 `single-page-application`만 설정해서 끝내면 안 된다.

권장 파일럿: 현재 빌드 구조를 보존하고 `_redirects`의 상대 200 proxy 규칙으로 `/hwpx/compile`, `/hwpx/parse`를 `/hwpx/index.html`로 제공한다. 명시적 두 화면과 기존 별칭을 먼저 처리하고, `/hwpx/assets/*`를 삼키는 광역 fallback은 넣지 않는다. 대안은 배포 staging 루트에 index.html을 두는 구조이지만 경로 중복/캐시 동작을 별도 검증해야 한다. Worker 코드 추가는 정적 규칙으로 해결할 수 없을 때만 검토한다.

`_headers`, `_redirects`는 업로드 자산 루트에 있어야 한다. `web/public`에 넣으면 현재 outDir 특성상 `web/dist/hwpx`로 복사되므로 배포 준비 스크립트가 `web/dist` 루트에 규칙 파일을 배치하도록 한다.

### 정책 보존

- `vercel.json`의 CSP를 그대로 이식한다. `script-src`의 `'wasm-unsafe-eval'`을 제거하지 않는다.
- immutable은 content-hashed `/hwpx/assets/*`에만 적용한다. index.html과 고정 이름 template/prompt/instruction/example은 재검증 대상으로 유지한다.
- 기존 gateway가 헤더를 덮어쓰는지 최종 사용자 URL에서 확인한다.
- gateway가 `/hwpx` prefix를 제거하지 않도록 전달 계약을 기록한다. 기존 gateway가 도메인 전체 라우트를 소유한다면 별도 `/hwpx/*` Worker route 추가로 우회시키지 말고, 기존 upstream 전환을 우선한다. 리다이렉트에 내부 backend 호스트가 노출되지 않아야 한다.
- 정적 자산 요청은 공식 문서상 무료·무제한, 자산 저장 추가 비용 없음. 별도 gateway/Worker 코드 실행 및 CI 요금까지 0원으로 보장하지 않는다.

## 4. 작업 순서

### P0 — 운영 경계 및 기준선 확보

담당: dev / 검수: plan.

- 현재 실제 서비스 origin, gateway 레포·규칙·배포 권한, 기존 Vercel 배포 ID를 기록한다.
- 기존 서비스의 경로별 HTTP 응답, 헤더, 브라우저 두 화면 변환 기준선을 저장한다.
- 기존 주소를 유지할 수 있는지 확인한다. 불가능하면 origin 변경과 사용자 로컬 데이터 영향은 사용자 승인 후 진행한다.

완료: 기준선 보고서, gateway 변경 대상과 복구 방법, 유지할 URL 목록이 기록되어 있다. 미확인 gateway를 가정하여 바로 전환하지 않는다.

### P1 — 정적 배포 설정 구현

담당: dev.

- Node와 Wrangler 버전을 고정하고 Wrangler 개발 의존성/lockfile 및 배포 설정을 추가한다.
- 자산 루트는 `web/dist`, base는 `/hwpx/` 유지. 규칙 파일 생성/복사 스크립트를 추가한다.
- 루트/레거시 리다이렉트와 두 화면 HTML proxy, 보안·캐시 헤더를 이식한다.
- Rust 및 WASM은 이번 호스팅 이전에서 재설계하지 않고 커밋된 산출물을 사용한다. CI WASM 재빌드는 후속 작업으로 분리한다.

완료: clean checkout에서 `cargo test`, `npm --prefix web ci`, `npm --prefix web run build`가 성공한다. 로컬 Wrangler에서 아래 HTTP 행렬이 통과한다. 로컬 성공은 운영 성공과 구분한다.

### P2 — 독립 파일럿

담당: dev / UX 검수: design.

- 운영 경로를 바꾸지 않고 workers.dev에 배포한다.
- 실제 HTTPS 주소에서 HTTP/브라우저/다운로드/네트워크 검증을 실행한다.
- 산출물은 배포 URL, commit SHA, 배포 ID, 테스트 로그, 브라우저 확인 결과로 남긴다.

완료: 아래 모든 기능·보안 검증 통과. 하나라도 실패하면 P3 진입 금지.

### P3 — 자동 배포와 운영 전환

담당: dev / 승인: 사용자.

- 권장 CI는 GitHub Actions 하나로 통일한다. Rust 테스트와 웹 빌드 성공 후에만 배포하고, production environment 승인 및 동시 배포 직렬화를 둔다.
- 브라우저 스모크도 운영 승격 차단 게이트로 둔다. 자동화된 스모크와 design 수동 검수 증거를 연결하며, 빌드 성공만으로 승격하지 않는다.
- PR 검증은 운영 배포 없이 실행한다. fork PR에 배포 secret을 제공하지 않는다. Cloudflare Git 자동 배포와 이중 운영하지 않는다.
- 최소 권한 배포 token은 CI secrets에 등록하고 문서/레포/채팅에 남기지 않는다.
- 파일럿 통과 후 기존 gateway에서 `/hwpx` 경로만 전환한다. xcv.kr 전체 DNS나 다른 앱 라우트를 바꾸지 않는다.
- 최종 사용자 URL에서 전체 검증을 재실행하고, 전환 직후와 다음 날 확인한다. Vercel은 최소 7일 복구 대상으로 유지한다(기간은 계획 제안).

완료: 전환 전후 origin 동일 여부, 실제 CF 배포 제공 여부, 다른 경로 무영향, 자동 배포 테스트 기록 확보. 사용자 승인 전에는 운영 전환하지 않는다.

### P4 — 문서와 정리

담당: dev / 최종 확인: plan.

- README/AGENTS의 Vercel 전용 설명과 자산 배치 설명을 새 운영 방식에 맞춘다.
- 검증 로그와 롤백 절차를 `docs/`에 남긴다.
- 유지 기간 경과 및 사용자 승인 뒤에만 Vercel 자동 배포/프로젝트를 정리한다. 초기 이전 PR에서 vercel.json부터 지우지 않는다.

## 5. HTTP 수용 기준

아래 검사는 파일럿과 최종 사용자 URL에서 각각 수행하고 status/Location/Content-Type/Cache-Control/CSP를 기록한다.

- `/hwpx/compile`, `/hwpx/parse`: 200 HTML, 브라우저 직접 접근/새로고침 시 올바른 화면.
- `/`, `/hwpx`, `/hwpx/`, `/hwpx/m2h`, `/m2h` 및 `/m2h/test`: compile로 임시 리다이렉트. 단, xcv.kr 공유 gateway의 `/`는 기존 서비스 유지이며 앱 단독 origin에서만 루트 규칙을 적용한다.
- `/hwpx/h2m`, `/h2m`, `/h2m/test`: parse로 임시 리다이렉트. 공유 도메인의 레거시 최상위 경로는 기존 소유/충돌 확인 후 적용한다.
- HTML이 참조한 JS/CSS/WASM: 200, 올바른 MIME, HTML 본문 아님. WASM은 `application/wasm`.
- `/hwpx/template.hwpx`, `/hwpx/prompt.md`, `/hwpx/instruction.md`, `/hwpx/example.md`: 200, 다운로드 바이트가 빌드 산출물과 일치.
- 존재하지 않는 `/hwpx/assets/missing.wasm`: 404, 200 HTML fallback 아님.
- hashed assets: `max-age=31536000, immutable`. HTML/고정 이름 public 파일: `max-age=0, must-revalidate` 등 명시적 재검증 정책.
- HTML에 기존 CSP, nosniff, DENY, Referrer-Policy, Permissions-Policy가 보존되며 WASM 로드 CSP 오류 없음.

## 6. 브라우저 수용 기준

- `web/public/example.md`와 기본 template으로 HWPX를 생성하고 다운로드한다. 해당 결과를 parse 화면에 넣어 Markdown을 얻고, 다시 compile하여 정상 출력한다. 바이트 동일성 대신 경고/블록 구조의 기존 기준선과 비교한다.
- Chrome 및 Safari의 실제 검증 버전을 기록하고 두 화면의 복사·다운로드·드래그앤드롭·지침/템플릿 로드를 확인한다.
- compile에서 고유 문자열을 입력한 뒤 parse로 갔다 돌아와도 문자열 유지, 뒤로/앞으로 이동 시 URL·화면 일치.
- 같은 운영 origin에서 전환 전 저장한 compile 초안(localStorage), 사용자 템플릿(IndexedDB), 테마와 안내 닫힘 상태가 전환 후 복구된다. 다른 origin에서는 자동 이관되지 않음을 안내하며 파일럿 결과를 운영 데이터 보존 증거로 쓰지 않는다. parse 결과의 새로고침 복원은 현재 기능이 아니므로 새 요구로 추가하지 않는다.
- Network 로그에 문서 내용·템플릿 바이트를 서버로 보내는 요청이 없다. 자산 다운로드와 로컬 blob 다운로드는 구분한다.
- 새 배포 후 기존 탭과 새 탭에서 엔진·public 파일 조합이 깨지지 않음을 확인한다.

### design 검토 반영: 세부 회귀 체크

- 두 화면에 다른 내용을 둔 뒤 전환·뒤로·앞으로 이동하여 내용·파일명·결과·문서 제목을 기준선과 비교한다. compile에 HWPX, parse에 MD/TXT를 드롭하면 대상 화면으로 전환되고 파일이 중복 처리되지 않는다.
- 실제 HTTPS에서 본문/지침 복사 후 외부 편집기에 붙여넣어 원문 일치 확인. Clipboard 권한 거부 시 성공 표시를 하지 않는다. 새 origin의 권한은 기존 origin과 별개로 검증한다.
- 한글·공백·금지문자 포함 제목의 다운로드 이름과 파일 재열기, 템플릿 교체·초기화를 확인한다. parse 다중 입력의 개별 다운로드·실패 파일 처리·마지막 성공 결과를 기존 배포와 비교한다.
- 데스크톱 Chrome/Safari와 모바일 Safari/Chrome에서 기존 배포와 동일한 화면·버튼·로딩·드롭 피드백을 확인한다. 모바일은 키보드 표시/회전 후 편집 영역·푸터 접근 가능 여부도 확인한다. 모바일 드롭 미지원 환경은 해당 기능 제외 사유를 기록한다.
- 테마 순환과 새로고침 후 유지, auto에서 OS 테마 반영을 확인한다. compile 초안은 입력 후 400ms 이상 지난 뒤 새로고침하여 복원 확인한다. 안내 닫힘 24시간 처리는 저장 시각을 제어하는 테스트로 확인하며 실제 대기를 필수화하지 않는다.
- parse의 손상·크기 초과 입력 시 기존 본문 유지와 오류/경고 표시를 기준선과 비교한다. 인쇄에는 활성 문서만 나타나는지 두 화면 미리보기를 모두 켠 조건으로 확인한다.

기존 결함 분리: design 검토에서 두 화면 미리보기의 중복 `#print-area` 가능성이 지적되었다. 실제 인쇄 결함은 미확인이다. P0에서 재현 여부를 기록하고, 기존 결함이면 별도 이슈와 사용자 승인된 제외 범위를 남긴다. 기존 미리보기/복사 UI 차이는 호스팅 이전 명목으로 임의 통일하지 않는다. 신규 회귀는 제외 처리하지 않는다.

## 7. 롤백

트리거: HTML/WASM 로드 실패, 어느 한 방향 변환 불가, CSP 오류, 로컬 데이터 접근 회귀 또는 다른 서비스 라우팅 침범.

1. gateway에서 `/hwpx` upstream을 전환 전 Vercel 대상으로 복구한다.
2. Cloudflare 자산 배포 자체의 문제면 직전 정상 배포로 복원한다. 두 롤백 경로를 구분한다.
3. 최종 사용자 URL의 두 화면·자산 로딩·예제 변환을 재검증한다.
4. 원인과 적용 배포 ID를 기록하고 자동 배포를 중지한 상태에서 수정한다.

Cloudflare 버전 롤백은 파일럿에서 실제 수행하고 성공 로그를 남긴다. gateway 복구는 운영과 동등한 시험 경로에서 CF → Vercel upstream 전환을 실제 검증한다. 시험 경로가 없으면 그 제약을 명시하고 사용자 승인된 점검 시간에 별도 리허설한 뒤 본 전환한다. Worker 버전 복원만으로 gateway/DNS/route가 복구된다고 보지 않는다. 운영 전환 전에 변경 권한과 복구 대상을 확보한다.

## 8. 남은 불확실성과 검토

- 운영 gateway의 구현·레포·동일 origin 유지 가능 여부: P0에서 확인 필요.
- Cloudflare 계정/zone 권한, workers.dev 사용 가능 여부, CI secret 등록: 실행 단계에서 확인 필요.
- dev 역할 검토 완료: 앱 호스팅 변경은 낮은 난이도, gateway 포함 시 중간 수준. 경로·fallback·헤더·CI·분리 롤백을 주요 위험으로 확인했다.
- design 역할 검토 완료: 동일 origin 보존이 최우선이며 로컬 데이터·권한·양쪽 페이지 상태·캐시·다운로드 회귀 기준을 위에 반영했다. 두 검토 모두 코드 기반이며 실제 배포 브라우저 승인을 대신하지 않는다.
- 이번 계획 수립에서 실제 빌드/테스트/배포는 수행하지 않았다. 위 기준은 앞으로의 완료 조건이지 통과 주장 아님.

## 9. 공식 근거

- Static Assets: https://developers.cloudflare.com/workers/static-assets/
- SPA fallback(`/index.html`): https://developers.cloudflare.com/workers/static-assets/routing/single-page-application/
- `_redirects`의 상대 200 proxy/규칙 순서: https://developers.cloudflare.com/workers/static-assets/redirects/
- `_headers`와 기본 재검증 정책: https://developers.cloudflare.com/workers/static-assets/headers/
- 정적 요청/Worker 실행 과금 구분: https://developers.cloudflare.com/workers/static-assets/billing-and-limitations/
- Workers Builds 대안 및 배포 명령 구성: https://developers.cloudflare.com/workers/ci-cd/builds/configuration/
