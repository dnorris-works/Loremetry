# Manuscript Intel (web)

Single-user Vue + Rust (Axum) app for fiction market/craft analysis. Deployed on Miget.

## Local development

```bash
# Terminal 1 — API (SQLite in ./data)
export DATA_DIR=./data STATIC_DIR=./ui/dist PORT=8080
cargo run -p manuscript-intel-web

# Terminal 2 — Vue (proxies /api → :8080)
cd ui && npm install && npm run dev
```

Optional env secrets (also used on Miget):

- `ANTHROPIC_API_KEY` / `TOKENMIX_API_KEY`
- `CANOPY_API_KEY`
- `DATAFORSEO_LOGIN` / `DATAFORSEO_PASSWORD`

## Production (Miget via GitHub)

This repo is ready for **GitHub → Miget** deploy (same flow as other web apps). Push your code to GitHub; Miget builds from the root `Dockerfile`.

### One-time setup

1. **Miget:** Workspace Settings → **Git Credentials** → **Connect GitHub** → install the Miget app on this repo.
2. **New application** → source **GitHub** → select the `Loremetry` repo and branch (e.g. `main`).
3. **Builder:** **Docker Engine** (uses the repo `Dockerfile`; do not use auto-detect buildpacks — this app is Vue + Rust).
4. **Storage:** attach a persistent volume mounted at **`/data`** (SQLite lives here).
5. **Variables** (Settings → Variables):
   - `ANTHROPIC_API_KEY`, `CANOPY_API_KEY`, etc. (optional BYOK overrides in the UI still work)
   - Miget sets `PORT` automatically; the app already listens on it.
6. Enable **Auto-deploy** so pushes to your branch redeploy.

After the first deploy, Miget gives you a public URL. No `git push miget` remote required.

### Manual Docker (optional)

```bash
docker build -t manuscript-intel .
docker run -p 8080:8080 -v mi-data:/data \
  -e ANTHROPIC_API_KEY=... \
  manuscript-intel
```

Mount a persistent volume at `/data` for SQLite.
