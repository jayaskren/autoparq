# PLAN — GitHub Pages Deployment (Spec 009)

## Approach

Five independent fixes applied in a single pass. No new Rust code. No new JS code. All changes are configuration and CI plumbing.

---

## Phase 1 — Repo hygiene: `.gitignore`

Create a root `.gitignore` that covers every generated artifact: Rust `target/`, both `pkg/` directories (root wasm-pack + `web/pkg/`), Python venv and bytecode, generated test fixtures, and web build outputs. The existing `web/.gitignore` covers the web subdirectory; the root file consolidates everything into one canonical location so nothing leaks into the initial commit.

---

## Phase 2 — Fix Vite base path

Add `base: '/autoparq/'` to `web/vite.config.js`. Without this, all asset paths in the production build reference `/assets/...` which resolves against the domain root, not the GitHub Pages subdirectory `/autoparq/`. The `base` option does not affect the dev server's behaviour — `npm run dev` continues to work at `http://localhost:5173/autoparq/` with an automatic redirect from `/`.

---

## Phase 3 — Fix placeholder GitHub link

Replace `href="https://github.com/your-org/autoparq"` with `href="https://github.com/jayaskren/autoparq"` in `web/index.html`. One line.

---

## Phase 4 — Replace docs deploy with web app deploy

Delete `.github/workflows/docs.yml` (MkDocs). GitHub Pages supports one active deployment per repo — keeping both would cause the second deploy to fail. Create `.github/workflows/deploy.yml` that:

1. Installs Rust stable with cargo cache
2. Installs `wasm-pack` via `jetli/wasm-pack-action`
3. Builds the WASM target: `wasm-pack build --target web --out-dir web/pkg --release --features wasm --no-default-features`
4. Installs Node 20 with npm cache keyed to `web/package-lock.json`
5. Runs `npm ci` in `web/`
6. Runs `npx vite build` in `web/`
7. Uploads `web/dist/` as the Pages artifact
8. Deploys to the `github-pages` environment

The workflow uses `concurrency: group: pages, cancel-in-progress: false` to prevent concurrent deploys from racing.

---

## Phase 5 — Fix maturin feature flag in `pyproject.toml`

**Conflict:** `[tool.maturin].features = ["pyo3/extension-module"]` does not activate the `python` Cargo feature, which is the feature gate on all `#[cfg(feature = "python")]` blocks in `src/lib.rs`. Without it, `maturin develop` compiles pyo3 as a dependency but the `#[pymodule]` registration block is excluded — the resulting `.so` has no exported symbol and the Python import fails.

The pyo3 dependency in `Cargo.toml` already declares `features = ["extension-module"]` inline, so `extension-module` is automatically enabled whenever `dep:pyo3` is activated via the `python` feature. Specifying it again in `pyproject.toml` is redundant and misleading.

**Fix:** Change `features = ["pyo3/extension-module"]` to `features = ["python"]`. This is read by both `maturin develop` (in `ci.yml`) and `maturin-action` (in `release.yml`), fixing both without touching either workflow file.

---

## Verification

After all five phases:

- `git status` on a clean tree shows no untracked generated files
- `npm run dev` in `web/` serves at `http://localhost:5173/autoparq/`
- `npx vite build` in `web/` produces `web/dist/` with correct asset paths (`/autoparq/assets/...`)
- `maturin develop` (no flags) compiles the Python extension correctly
- Pushing to `main` triggers `deploy.yml`, which builds and deploys to `https://jayaskren.github.io/autoparq/`
- One manual GitHub setting required: Settings → Pages → Source → GitHub Actions
