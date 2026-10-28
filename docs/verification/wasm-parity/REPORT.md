# Web WASM parity verification — 2026-10-28

## Root cause
The tracked `web/src/lib/wasm/hwpx_wasm_bg.wasm` was unchanged since the initial commit. The Rust feature commit updated CLI source/tests and the native template but not the prebuilt WASM. `npm run build:cloudflare` runs Vite and asset preparation, not Rust/WASM compilation. Re-deploying that bundle re-published the old engine.

## Controlled reproduction
Input: adjacent `input.md` (user's exact supplied Markdown).
Using the same current `template.hwpx`, old WASM leaves both `^[잠정]` tokens literal and all three last-row cells DASH. Rebuilt WASM consumes both markers, produces `[잠정]` superscript runs, and makes all three last-row bottoms SOLID, #808080, 0.12 mm (native 3-column prototype values). No engine/source/template changes were needed for this fix. The attached user document exhibited the same old-WASM symptoms.

## Build
```
rustup target add wasm32-unknown-unknown
PATH="$HOME/.cargo/bin:$PATH" npm exec --yes --package=wasm-pack@0.15.0 -- wasm-pack build web/wasm --target web --release --out-name hwpx_wasm --out-dir ../src/lib/wasm
wasm-opt -Oz --enable-bulk-memory-opt --enable-nontrapping-float-to-int --enable-sign-ext --strip-debug --strip-producers web/src/lib/wasm/hwpx_wasm_bg.wasm -o web/src/lib/wasm/hwpx_wasm_bg.wasm
```
Remove the generated `web/src/lib/wasm/.gitignore` before staging tracked build artifacts. Homebrew rustc lacks the installed rustup target; prioritize `$HOME/.cargo/bin` for this build.

## Verified gates
- cargo test: 182 lib + 6 integration passed.
- npm run test:deploy: 1 passed; cf:dry-run succeeded.
- Browser smoke: 3 passed locally and 3 passed on workers.dev, using installed Chrome because Playwright's pinned headless shell was absent.
- Real Chrome UI at `https://xcv.kr/hwpx/compile`: entered exact user Markdown, clicked HWPX download, inspected downloaded package: two superscript `[잠정]` runs, no residual caret markers, three SOLID last-row bottoms. No browser page errors.
- Live WASM downloaded from workers.dev matches local rebuilt binary SHA256 `f50700b274815ae65a127ebae440cf96102a3443f4dd68cd23684ba7fa439e83`.
- Deployed Version ID: `67051789-0209-4d08-8c4c-fa5b9ea2b61f`; deployment readback confirms 100% allocation.

## Artifacts / reproduction
`workspace/wasm-parity/` (gitignored): `compile.mjs`, `check.py`, `live-browser.mjs`, exact input, old/fixed WASM outputs and live downloaded `live-browser.hwpx`.

```
node workspace/wasm-parity/compile.mjs
node workspace/wasm-parity/live-browser.mjs
python3 workspace/wasm-parity/check.py workspace/wasm-parity/live-browser.hwpx
```

Adjacent `results.json` records the controlled old-WASM failure and deployed-browser success. HWPX XML and browser conversion are verified; native Hancom opening/visual acceptance is not claimed.
