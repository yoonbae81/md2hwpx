# Cloudflare 파일럿 배포 및 운영 전환 런북

## 현재 범위와 상태

이 문서는 `cloudflare.yml`의 실행 계약이다. 워크플로 추가는 실제 배포·검증·운영 승인 완료를 뜻하지 않는다. 현재 자동화는 **secret 없는 PR 검증과 수동 승인된 workers.dev 파일럿만** 제공한다. push 자동 배포, production job, gateway/DNS/route 변경은 없다. Cloudflare Git 자동 배포를 병행하지 않는다.

- Node는 **22.21.1**로 고정한다. 루트와 `web`의 lockfile로 각각 `npm ci`한다.
- Rust 엔진 변경이나 WASM 재빌드는 하지 않는다. 커밋된 `web/src/lib/wasm/`을 사용한다.
- Vite base `/hwpx/`, 산출물 `web/dist/hwpx`, Cloudflare 자산 루트 `web/dist`를 유지한다. `_headers`, `_redirects`는 자산 루트에 있어야 한다.
- `docs/cloudflare-migration-plan.md`의 HTTP/브라우저/보안/복구 수용 기준이 계속 적용된다.
- quota 저장소의 gateway 코드는 참고 증거일 뿐이다. 실제 `xcv.kr` 응답은 Caddy를 경유하며 **활성 라우트의 소유 레포·설정·변경 권한은 미확정**이다. 확인 없이 해당 코드를 운영 제어점으로 간주하지 않는다.

## 관리자 사전 설정 — 실행 전 필수

1. GitHub Environment **`Cloudflare`**을 먼저 생성한다. Required reviewers와 승인 규칙을 설정하고 가능하면 자기 승인 및 관리자 우회를 막는다. 배포 허용 브랜치를 검토된 브랜치로 제한한다. GitHub 요금제/저장소 공개 범위에서 보호 규칙이 실제 적용되는지 확인한다. YAML의 environment 선언만으로 required reviewer가 생기지 않는다. 보호 환경 승인이 불가능하면 파일럿을 실행하지 않는다.
2. 이 environment에만 `CLOUDFLARE_API_TOKEN`, `CLOUDFLARE_ACCOUNT_ID` secrets를 등록한다. PR 검증에는 secrets를 넣지 않는다. token은 대상 계정의 파일럿 Worker 배포에 필요한 최소 권한으로 제한하며 DNS/zone route 변경 권한은 주지 않는다. 토큰을 로그·문서·채팅에 쓰지 않는다.
3. 검토할 ref의 `cf:deploy`와 Wrangler 설정을 확인한다. **별도 파일럿 Worker, workers.dev 활성화, custom domain/운영 routes 없음**이 승인 조건이다. `npm run cf:deploy`가 파일럿만 대상으로 하는지 확인한다. 이 워크플로는 Worker 이름을 덮어쓰거나 운영 배포를 안전한 것으로 자동 판정하지 않는다.
4. 계정 subdomain과 Worker 이름으로 예상되는 `https://WORKER.SUBDOMAIN.workers.dev`를 확보한다. 입력 URL은 실제 배포 대상과 같아야 한다. 환경 승인자는 배포 설정과 입력 URL의 일치를 확인한다. hostname 허용 검사만으로 새 배포의 identity가 증명되지는 않는다.
5. 기존 Vercel 배포 ID/URL, 알려진 정상 Cloudflare 버전(존재한다면), 실제 gateway 복구 설정/권한을 별도로 기록한다. 첫 파일럿이면 이전 정상 CF 버전은 없다고 명시한다.

## PR 검증

`pull_request`에서 `validate`만 실행한다. `pull_request_target`이나 secret 주입은 사용하지 않는다. checkout 인증정보도 보존하지 않는다. 순서:

```sh
npm ci
npm --prefix web ci
cargo test
npm run test:deploy
npm run build:cloudflare
npx playwright install --with-deps chromium
npm run test:browser
npm run cf:dry-run
```

`test:browser`는 `SMOKE_BASE_URL`이 없는 경우 로컬 대상으로 실행하는 스크립트 계약이다. `cf:dry-run`은 인증 없이 로컬 산출물만 검증해야 한다. 이 명령 중 하나라도 실패하면 배포 job으로 진행하지 않는다. root 스크립트와 lockfile 구현은 별도 변경의 책임이며, 아직 없거나 불완전하면 CI 실패가 정상 차단 동작이다.

검증된 `web/dist` 전체를 `cloudflare-assets-<commit SHA>` artifact로 14일 보존한다. 파일럿 job은 동일 실행의 artifact를 받아 사용하며 다시 빌드하지 않는다. `cf:deploy`가 이 산출물을 배포하고 별도 미검증 빌드를 하지 않는지도 확인한다.

## 수동 파일럿 실행

1. Actions → **Cloudflare validation and pilot** → Run workflow에서 검토된 ref를 선택한다. GitHub가 수동 실행 UI를 제공하려면 workflow_dispatch 정의가 기본 브랜치에 있어야 한다. 이 조건을 맞추려는 임의 push/merge는 본 작업 범위가 아니다.
2. `smoke_url`에 **origin만** 입력한다. 예시 형식은 `https://WORKER.SUBDOMAIN.workers.dev`이며 실제 이름으로 바꾼다. `/hwpx/compile`은 붙이지 않는다. 선택적 마지막 `/`만 허용한다. HTTPS와 정확히 두 hostname label 뒤의 `.workers.dev`만 허용한다. custom domain, HTTP, port, 사용자정보, 추가 path/query/fragment 및 우회 문자열은 배포 전 검증에서 거부한다. smoke 스크립트가 `/hwpx` 경로를 검사한다.
3. 동일한 secret-free 검증이 먼저 통과해야 한다. `Cloudflare` 검토자는 SHA, diff, artifact, 파일럿 설정과 URL을 확인한 뒤 environment 승인을 내린다. workflow_dispatch 권한만으로 보호 환경 승인을 대체하지 않는다.
4. 파일럿 job은 `Cloudflare` concurrency group, `cancel-in-progress: false`로 겹쳐 실행되지 않는다. GitHub concurrency는 FIFO 큐를 보장하지 않으며 대기 실행이 대체될 수 있으므로 필요한 실행의 상태를 직접 확인한다.
5. 승인된 job이 의존성과 Chromium을 설치하고 검증 artifact를 복원한 뒤 다음 명령을 실행한다. 두 Cloudflare secrets는 deploy step에만 전달한다.

```sh
npm run cf:deploy
npm run test:http -- "$URL"
SMOKE_BASE_URL="$URL" npm run test:browser
```

`URL`은 앞에서 검증·정규화한 workers.dev origin이다. PR이나 입력값을 shell 코드로 직접 보간하지 않는다. HTTP 또는 브라우저 smoke 실패는 job 실패이며 운영 승격 금지다. **배포 후 smoke 실패가 배포를 자동 취소하거나 자동 롤백하지는 않는다.** 로그에서 실패를 확인하고 아래 복구 절차를 적용한다.

## 증거와 남은 수동 검증

실행 URL, commit SHA, artifact, Wrangler 배포 ID/버전, 정확한 workers.dev URL, HTTP/브라우저 로그와 결과를 검토 기록에 연결한다. Actions summary의 job 결과는 자동화 범위의 결과일 뿐이다. 이 문서 작성 작업에서 실제 파일럿·운영 네트워크 변경은 하지 않는다.

다음은 **미완료 게이트**이며 Chromium CI 통과로 완료 표시하지 않는다:

- 실제 HTTPS에서 Chrome 및 데스크톱 Safari의 버전과 결과 기록. 모바일 Safari/Chrome에서 편집, 키보드·회전·푸터, 복사/다운로드, 드롭 지원 범위를 수동 확인한다. Playwright Chromium은 실제 모바일 Safari 검증이 아니다.
- 양방향 예제 변환과 왕복, 다운로드 파일 재열기, 한글/공백/금지문자 파일명, 템플릿 교체·복원, 손상/과대 입력 처리와 실제 인쇄를 기준선과 비교한다.
- 두 화면의 상태 유지, 뒤로/앞으로, 직접 진입·새로고침, 테마/안내 저장, 실제 외부 편집기에 clipboard 붙여넣기와 권한 거부를 확인한다.
- Network 기록으로 본문·템플릿 업로드가 없음을 확인한다. CSP 오류, WASM MIME, missing asset 404, 고정 파일 재검증과 hashed asset immutable, 새 탭/기존 탭 캐시 호환성을 확인한다.
- workers.dev는 운영과 다른 origin이다. 운영 localStorage 초안·IndexedDB 템플릿·권한 보존을 파일럿으로 입증할 수 없다. 실제 동일 origin의 전환 전후 검증이 필요하다.
- 기존 인쇄 중복 가능성 등 기존 결함은 재현·별도 이슈·승인된 제외 범위를 기록한다. 신규 회귀를 기존 결함으로 숨기지 않는다.
- 두 롤백 경로의 리허설 및 **명시적인 사용자 운영 승인**이 필요하다.

## 운영 전환 — 현재 워크플로에 없음

live smoke와 수동/mobile Safari 검증 증거가 승인되기 전에는 production CI를 추가하거나 gateway를 변경하지 않는다. 이후 별도 검토에서 production environment 승인과 직렬 배포 정책을 설계한다. 환경 이름은 `Cloudflare`지만 이 환경은 파일럿 전용 게이트다. 운영 전환용 environment는 별도로 만든다.

실제 gateway 운영 소유자, 레포/설정, 배포 권한과 현재 Vercel upstream을 확인한다. 동일 origin과 `/hwpx` prefix 전달을 유지하고 `/hwpx` 경로만 전환하는 변경을 사용자에게 승인받는다. `xcv.kr` 전체 DNS, 공유 `/`, 다른 서비스 route는 변경 대상이 아니다. 내부 backend hostname 노출이나 gateway 헤더 덮어쓰기도 검증한다.

승인된 별도 전환 뒤 최종 사용자 URL에서 계획의 전체 HTTP/브라우저 기준, 다른 앱 무영향, 실제 CF 제공 여부와 로컬 데이터 보존을 확인한다. 직후와 다음 날 재점검한다. Vercel은 제안 최소 7일간 복구 대상으로 유지하며 기간·정리는 사용자 승인 후 결정한다.

## 롤백 — 서로 다른 두 경로

트리거: HTML/WASM/CSP 실패, 어느 한 방향 변환 불가, 데이터 접근 회귀, 다른 앱 라우트 침범. 자동 배포/추가 실행을 중지하고 실패 SHA·배포 ID·로그를 보존한다. 아래 조작은 권한 있는 담당자가 별도 승인 범위에서 수행한다. 이 워크플로에는 자동 복구나 gateway 변경이 없다.

### A. Cloudflare 배포/자산 문제

Cloudflare Dashboard에서 정확한 **파일럿 Worker**의 Deployments/Versions와 알려진 정상 버전을 확인하고, 해당 배포에 대한 rollback 기능으로 복원한다. CLI를 쓰면 저장소에 고정된 Wrangler 버전의 `wrangler rollback --help`를 확인한 후 정확한 Worker·대상 배포를 명시하여 실행한다. 버전 ID와 배포 ID를 혼동하지 않는다. 최초 배포라 복구 대상이 없으면 없다고 기록하고 수정 파일럿을 승인받는다.

파일럿에서 실제 복원을 수행하고 복원된 배포 ID와 HTTP/브라우저 재검증 성공 로그를 남겨야 리허설 완료다. 현재는 미실행이다. Worker 버전 복원은 gateway/DNS/route 설정을 복구하지 않는다.

### B. gateway upstream 전환 문제

운영 전환 뒤라면 **확인된 활성 gateway 설정**에서 `/hwpx` upstream만 기록해 둔 이전 Vercel 대상으로 되돌린다. 적용 방식은 활성 운영 소유자 확인 후 결정하며 quota 코드나 추정 명령으로 대신하지 않는다. prefix, 헤더, 다른 서비스 경로를 보존한다.

운영과 동등한 시험 경로에서 CF → Vercel 복구를 실제 검증하고 로그를 남긴다. 시험 경로가 없으면 제약을 기록하고 사용자 승인된 점검 시간에 별도 리허설 후 본 전환한다. 최종 사용자 URL의 두 화면·WASM·예제 변환·로컬 데이터와 다른 앱 무영향을 재확인한다. 현재는 활성 소유권 확인과 리허설 모두 미완료다.
