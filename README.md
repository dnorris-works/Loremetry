# Loremetry

Single-user Vue + Rust (Axum) app for fiction market/craft analysis. Deployed on Miget.

## Local development

```bash
# Start Postgres (schema `lore` is created on first API boot)
docker compose up -d db

# Terminal 1 — API
export DATABASE_URL=postgres://loremetry:loremetry@localhost:5432/loremetry
export STATIC_DIR=./ui/dist PORT=8080
cargo run -p loremetry-web

# Terminal 2 — Vue (proxies /api → :8080)
cd ui && npm install && npm run dev
```

Optional env secrets (also used on Miget):

- `DATABASE_URL` — PostgreSQL connection string (required)
- `ANTHROPIC_API_KEY` / `TOKENMIX_API_KEY`
- `CANOPY_API_KEY`
- `DATAFORSEO_LOGIN` / `DATAFORSEO_PASSWORD`
- `ADMIN_TOKEN` — required for the Admin panel and `/api/admin/*` (WinningCat catalog import)

## Production (Miget via GitHub)

This repo is ready for **GitHub → Miget** deploy (same flow as other web apps). Push your code to GitHub; Miget builds from the root `Dockerfile`.

### One-time setup

1. **Miget:** Workspace Settings → **Git Credentials** → **Connect GitHub** → install the Miget app on this repo.
2. **New application** → source **GitHub** → select the `Loremetry` repo and branch (e.g. `main`).
3. **Builder:** choose **Docker Engine** (not “Auto detection / Buildpacks”).
   - Miget will build from the root **`Dockerfile`** (Vue + Rust in one image).
   - If you only see buildpacks, set **Settings → Variables** → `LANGUAGE` = `dockerfile` and redeploy.
4. **Database:** provision **PostgreSQL** on Miget and set **`DATABASE_URL`** in app variables. All tables live in the `lore` schema (migrations run automatically on startup).
5. **Variables** (Settings → Variables):
   - `DATABASE_URL` — from Miget Postgres
   - `ANTHROPIC_API_KEY`, `CANOPY_API_KEY`, etc.
   - `ADMIN_TOKEN` — a long random string; you use this in the **Admin** panel (sidebar) to import WinningCat CSV. End users never see this.
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
docker compose up -d db
docker build -t loremetry .
docker run -p 8080:8080 \
  -e DATABASE_URL=postgres://loremetry:loremetry@host.docker.internal:5432/loremetry \
  -e ANTHROPIC_API_KEY=... \
  loremetry
```

On Linux use `--network host` or point `DATABASE_URL` at the compose Postgres service hostname.
