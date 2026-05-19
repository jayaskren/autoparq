# TASKS — GitHub Pages Deployment (Spec 009)

## T01 — Create root `.gitignore`
**File:** `/.gitignore`
**Change:** New file covering `target/`, `/pkg/`, `.venv/`, `__pycache__/`, `*.py[cod]`, `*.so`, `.pytest_cache/`, `tests/fixtures/*.parquet`, `web/node_modules/`, `web/pkg/`, `web/dist/`, `.DS_Store`, `Thumbs.db`, `.idea/`, `.vscode/`.
**Verify:** `git check-ignore -v target .venv web/pkg web/dist tests/fixtures/monotonic_ints.parquet pkg` — all paths should be matched.
**Status:** ✅ Done

---

## T02 — Add `base` to `vite.config.js`
**File:** `web/vite.config.js`
**Change:** Add `base: '/autoparq/',` as the first key in `defineConfig({...})`.
**Verify:** `npm run dev` in `web/` starts at `http://localhost:5173/autoparq/` (Vite logs the URL). `npx vite build` produces `web/dist/index.html` where script/link `src`/`href` attributes start with `/autoparq/assets/`.
**Status:** ✅ Done

---

## T03 — Fix placeholder GitHub link in `index.html`
**File:** `web/index.html`
**Change:** `href="https://github.com/your-org/autoparq"` → `href="https://github.com/jayaskren/autoparq"`.
**Verify:** `grep "your-org" web/index.html` returns nothing.
**Status:** ✅ Done

---

## T04 — Delete `docs.yml`
**File:** `.github/workflows/docs.yml`
**Change:** Delete file. MkDocs deploy conflicts with the web app deploy for the single `github-pages` environment.
**Verify:** File no longer exists. `ls .github/workflows/` shows only `ci.yml`, `deploy.yml`, `release.yml`.
**Status:** ✅ Done

---

## T05 — Create `deploy.yml`
**File:** `.github/workflows/deploy.yml`
**Change:** New workflow triggered on push to `main`. Steps: checkout → Rust + cargo cache → wasm-pack → WASM build → Node 20 + npm cache → `npm ci` → `vite build` → upload `web/dist/` → deploy Pages.
**Verify:** File exists and is valid YAML (`python3 -c "import yaml; yaml.safe_load(open('.github/workflows/deploy.yml'))"` — no error).
**Status:** ✅ Done

---

## T06 — Fix maturin feature flag in `pyproject.toml`
**File:** `pyproject.toml`
**Change:** `[tool.maturin].features = ["pyo3/extension-module"]` → `features = ["python"]`
**Why:** `pyo3/extension-module` does not activate the `python` Cargo feature that gates the `#[pymodule]` block in `src/lib.rs`. The extension-module feature is already declared inline on the pyo3 dep in `Cargo.toml` and is activated automatically when `dep:pyo3` is enabled via the `python` feature.
**Verify:** `maturin develop` (no flags) succeeds and `python -c "import autoparq._lib"` works.
**Status:** ⬜ Pending

---

## T07 — Manual GitHub repo setting (not automatable)
**Action:** In the GitHub repo Settings → Pages → Source, switch from "Deploy from a branch" to **"GitHub Actions"**.
**Why:** Without this, the `deploy-pages` action fails with a permissions error even if the YAML is correct.
**Verify:** First push to `main` after committing these changes triggers `deploy.yml` and the Actions tab shows a successful Pages deployment.
**Status:** ⬜ Pending (requires repo to exist on GitHub)
