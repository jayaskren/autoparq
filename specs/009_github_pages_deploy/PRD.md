# GitHub Pages Deployment — Product Requirements Document

**Status:** Draft  
**Version:** 0.1  
**Date:** 2026-05-17  

---

## 1. Overview

The autoparq web app (Vite + WebAssembly) must be deployable to GitHub Pages on every push to `main`. Currently five blocking issues prevent this:

1. No workflow builds or deploys the web app — only MkDocs docs are deployed
2. Vite is missing a `base` path — all assets 404 on GitHub Pages subdirectory URLs
3. The GitHub link in `index.html` is a placeholder (`your-org`)
4. No `.gitignore` at the repo root — generated build artifacts will be committed
5. `web/pkg/` (WASM build output) is not excluded at root level — stale artifacts risk being committed

---

## 2. Goals

- Visiting `https://<user>.github.io/autoparq/` loads the working web app
- Every push to `main` triggers an automatic redeploy
- No generated artifacts (`target/`, `web/pkg/`, `web/dist/`, `.venv/`) are committed to the repo
- MkDocs documentation is either served alongside the app or dropped if there is no separate docs audience

## 3. Non-goals

- Custom domain (can be added later via CNAME)
- Separate staging environment
- CDN or caching configuration beyond GitHub Pages defaults

---

## 4. Changes Required

### 4.1 Root `.gitignore`

Create `/home/jayaskren/dev/autoparq/.gitignore` covering:

| Pattern | Reason |
|---------|--------|
| `/target/` | Rust build artifacts |
| `/pkg/` | Root-level wasm-pack output (PyO3 WASM build) |
| `.venv/` | Python virtual environment |
| `__pycache__/`, `*.pyc`, `*.pyo` | Python bytecode |
| `.pytest_cache/` | pytest cache |
| `python/autoparq/*.so` | Compiled PyO3 extension |
| `tests/fixtures/*.parquet` | Generated fixtures (run `cargo run --example gen_fixtures`) |
| `web/node_modules/` | npm dependencies |
| `web/pkg/` | wasm-pack output for the web app |
| `web/dist/` | Vite production build |
| `.DS_Store`, `Thumbs.db` | OS noise |
| `.idea/`, `.vscode/` | Editor noise |

The `web/.gitignore` already exists and covers `pkg/`, `dist/`, `node_modules/`. The root `.gitignore` consolidates everything into one canonical place.

### 4.2 Vite `base` configuration

`web/vite.config.js` needs a `base` option set to `/autoparq/` so that asset URLs (JS bundles, the `.wasm` file) are correct under GitHub Pages.

**Before:**
```js
export default defineConfig({
  plugins: [wasm(), topLevelAwait(), tailwindcss()],
  build: { target: 'es2022', assetsInlineLimit: 0 },
  resolve: { alias: { '@wasm': fileURLToPath(new URL('./pkg', import.meta.url)) } },
});
```

**After:**
```js
export default defineConfig({
  base: '/autoparq/',
  plugins: [wasm(), topLevelAwait(), tailwindcss()],
  build: { target: 'es2022', assetsInlineLimit: 0 },
  resolve: { alias: { '@wasm': fileURLToPath(new URL('./pkg', import.meta.url)) } },
});
```

`base` does not affect the local dev server, only the production build, so `npm run dev` continues to work as-is.

### 4.3 Fix placeholder GitHub link

In `web/index.html`, replace:
```html
<a href="https://github.com/your-org/autoparq" ...>GitHub</a>
```
With the actual repo URL (to be confirmed — `https://github.com/<user>/autoparq`).

### 4.4 Replace `docs.yml` with a unified web app deploy workflow

GitHub Pages only supports one active deployment per repo (one `pages` environment). The current `docs.yml` deploys MkDocs and will conflict with a web app deployment.

**Decision required:** Should the MkDocs docs be preserved?

- **Option A (Recommended): Web app only.** Replace `docs.yml` with a workflow that builds and deploys `web/dist/`. The CLI docs (MkDocs) are still in `docs/` and can be served via the web app itself or dropped.
- **Option B: Web app at root, docs at `/docs`.** Build both; merge them into one artifact. Adds complexity; only worthwhile if the MkDocs site has a real audience.

The new workflow (Option A) must:

1. Install Rust stable + `wasm-pack`
2. Build the WASM target: `wasm-pack build --target web --out-dir web/pkg --release --features wasm --no-default-features`
3. Install Node.js and run `npm ci` in `web/`
4. Run `vite build` in `web/` (outputs to `web/dist/`)
5. Upload `web/dist/` as the Pages artifact
6. Deploy to Pages

**Draft workflow:**

```yaml
name: Deploy Web App

on:
  push:
    branches: [main]

permissions:
  contents: read
  pages: write
  id-token: write

concurrency:
  group: pages
  cancel-in-progress: false

jobs:
  deploy:
    runs-on: ubuntu-latest
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Install wasm-pack
        uses: jetli/wasm-pack-action@v0.4.0

      - name: Build WASM
        run: wasm-pack build --target web --out-dir web/pkg --release --features wasm --no-default-features

      - name: Set up Node
        uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: npm
          cache-dependency-path: web/package-lock.json

      - name: Install JS dependencies
        run: npm ci
        working-directory: web

      - name: Build web app
        run: npx vite build
        working-directory: web

      - name: Upload Pages artifact
        uses: actions/upload-pages-artifact@v3
        with:
          path: web/dist/

      - name: Deploy to GitHub Pages
        id: deployment
        uses: actions/deploy-pages@v4
```

### 4.5 GitHub repository setting

In the GitHub repo settings → Pages → Source, set to **GitHub Actions**. Without this the deployment workflow will fail with a permissions error even if the YAML is correct.

This is a one-time manual step; it cannot be automated from the repo itself.

---

## 5. Acceptance Criteria

- [ ] `https://<user>.github.io/autoparq/` loads the web app (drop zone visible, WASM loads)
- [ ] Dropping a `.parquet` file produces a complete recommendation report
- [ ] No 404 errors for JS, CSS, or `.wasm` assets
- [ ] Every push to `main` triggers a redeploy automatically
- [ ] `git status` on a clean checkout shows no generated files as untracked
- [ ] `npm run dev` in `web/` still works locally (base path does not break dev server)

---

## 6. Open Questions

1. **What is the GitHub username/org?** Needed to fill in the `index.html` GitHub link and to confirm the Pages URL.
2. **Keep MkDocs docs or drop them?** Determines Option A vs B for the workflow.
3. **Does `package-lock.json` exist?** The workflow uses `npm ci` which requires it. If only `package.json` exists, use `npm install` instead and commit the lock file.
