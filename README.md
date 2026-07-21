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
3. **Builder:** choose **Docker Engine** (not “Auto detection / Buildpacks”).
   - Miget will build from the root **`Dockerfile`** (Vue + Rust in one image).
   - If you only see buildpacks, set **Settings → Variables** → `LANGUAGE` = `dockerfile` and redeploy.
4. **Storage:** attach a persistent volume mounted at **`/data`** (SQLite lives here).
5. **Variables** (Settings → Variables):
   - `ANTHROPIC_API_KEY`, `CANOPY_API_KEY`, etc. (optional BYOK overrides in the UI still work)
   - Miget sets `PORT` automatically; the app already listens on it.
6. Enable **Auto-deploy** so pushes to your branch redeploy.

After the first deploy, Miget gives you a public URL. No `git push miget` remote required.

### “Unable to detect language” on deploy

You’re on **Buildpacks** instead of Docker. Fix one of these:

| Fix | What to do |
|-----|------------|
| **Recommended** | App **Settings → Deployment** → change builder to **Docker Engine** → redeploy |
| **Stay on buildpacks** | **Settings → Variables** → add `LANGUAGE` = `dockerfile` → redeploy |

Do **not** set `LANGUAGE=rust` or `nodejs` alone — this app needs both the Vue build and the Rust binary; only the `Dockerfile` does that.

### Manual Docker (optional)

```bash
docker build -t manuscript-intel .
docker run -p 8080:8080 -v mi-data:/data \
  -e ANTHROPIC_API_KEY=... \
  manuscript-intel
```

Mount a persistent volume at `/data` for SQLite.
