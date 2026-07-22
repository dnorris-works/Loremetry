# Loremetry

Single-user Vue + Rust (Axum) app for fiction market/craft analysis. Deployed on Miget.

## Local development

### Postgres (pick one)

**Homebrew (no Docker)** — you already have `brew` and `psql`:

```bash
brew install postgresql@16

# First-time only: initialize the data directory (if createdb says "connection refused")
$(brew --prefix postgresql@16)/bin/initdb -D $(brew --prefix)/var/postgresql@16 --locale=en_US.UTF-8 -E UTF-8

# Start the server (if `brew services` shows `error`, use pg_ctl instead):
brew services start postgresql@16
# fallback:
# $(brew --prefix postgresql@16)/bin/pg_ctl -D $(brew --prefix)/var/postgresql@16 start

createdb loremetry
export DATABASE_URL=postgres://localhost:5432/loremetry
```

**Docker** — only if [Docker Desktop](https://www.docker.com/products/docker-desktop/) is installed:

```bash
docker compose up -d db
export DATABASE_URL=postgres://loremetry:loremetry@localhost:5432/loremetry
```

`docker-compose.yml` is for **local** Postgres + optional `web` service. On Miget, `compose.miget.yml` replaces `db` with a **managed Postgres addon** and sizes **`web`** (1Gi RAM, port 5000).

The `lore` schema and tables are created automatically on first API boot.

### Run the app

```bash
# Terminal 1 — API
export DATABASE_URL=postgres://localhost:5432/loremetry   # or the Docker URL above
export STATIC_DIR=./ui/dist PORT=8080
cargo run -p loremetry-web

# Terminal 2 — Vue (proxies /api → :8080)
cd ui && npm install && npm run dev
```

Optional env secrets (also used on Miget):

- `DATABASE_URL` — PostgreSQL connection string (required; Miget may inject `POSTGRES_<addon>_URL` instead, e.g. `POSTGRES_DBWEI_URL`)
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
4. **Database:** attach the **PostgreSQL** addon (or deploy with `compose.miget.yml`, which sets `db` to `managed: postgres`). Miget injects a URL such as `POSTGRES_DBWEI_URL` or `DATABASE_URL`; the app reads those automatically. Tables live in the `lore` schema (migrations on startup).
   - You do **not** need a `/data` volume anymore (that was only for the old SQLite file).
5. **Variables** (Settings → Variables):
   - `DATABASE_URL` — from Miget Postgres
   - `ANTHROPIC_API_KEY`, `CANOPY_API_KEY`, etc.
   - Miget sets `PORT` automatically; the app already listens on it.
6. Enable **Auto-deploy** so pushes to your branch redeploy.

After the first deploy, Miget gives you a public URL. No `git push miget` remote required.

### Inspecting the database (SQL, schema)

**In the app:** open **Admin** (sidebar) → **SQL console**. Run queries, browse `lore` tables, and see results without leaving the UI.

Loremetry tables are in the **`lore`** schema (not `public`). Migration metadata is in `public._sqlx_migrations`.

**From your Mac** (TablePlus, DBeaver, or `psql`):

1. Open the **PostgreSQL addon** (or standalone Postgres service) in Miget.
2. Enable **Public Access** on the database if you need to connect from outside Miget.
3. Copy the **External Connection** details or the ready-made `psql` command from the Connection Information panel.
4. Connect, then run:

```sql
-- List all Loremetry tables
\dt lore.*

-- Example queries
SELECT * FROM lore.stories;
SELECT COUNT(*) FROM lore.kdp_categories;
SELECT * FROM lore.genres LIMIT 10;
```

In GUI tools, set the schema to `lore` or qualify tables as `lore.stories`, etc.

**Local dev:** `psql $DATABASE_URL` then `\dt lore.*`

### “Unable to detect language” on deploy

You’re on **Buildpacks** instead of Docker. Fix one of these:

| Fix | What to do |
|-----|------------|
| **Recommended** | App **Settings → Deployment** → change builder to **Docker Engine** → redeploy |
| **Stay on buildpacks** | **Settings → Variables** → add `LANGUAGE` = `dockerfile` → redeploy |

Do **not** set `LANGUAGE=rust` or `nodejs` alone — this app needs both the Vue build and the Rust binary; only the `Dockerfile` does that.

### Manual Docker (optional)

**Full stack (app + Postgres):**

```bash
docker compose up -d --build
open http://localhost:8080
```

**Database only** (run the Rust binary on the host):

```bash
docker compose up -d db
export DATABASE_URL=postgres://loremetry:loremetry@localhost:5432/loremetry
export STATIC_DIR=./ui/dist PORT=8080
cargo run -p loremetry-web
```

**Single container** (Postgres elsewhere):

```bash
docker compose up -d db
docker build -t loremetry .
docker run -p 8080:8080 \
  -e DATABASE_URL=postgres://loremetry:loremetry@host.docker.internal:5432/loremetry \
  -e ANTHROPIC_API_KEY=... \
  loremetry
```

On Linux use `--network host` or point `DATABASE_URL` at the compose Postgres service hostname.

### Miget Compose Stack

Use **New Compose Stack** with compose path `.` (repo root). Miget merges `docker-compose.yml` + `compose.miget.yml`:

- **`web`** — builds from the `Dockerfile`, **1Gi** RAM, listens on **port 5000** (required for Miget ingress).
- **`db`** — **managed Postgres** addon (not the local `postgres:16` image); Miget injects `DATABASE_URL` / `POSTGRES_*_URL` into `web`.

You need **at least one `web` service** in compose; a managed `db` alone shows **Services: 0** and blocks Continue.

Alternatively, deploy only the **Dockerfile** app and attach a Postgres addon in the UI (what you used with `POSTGRES_DBWEI_URL`).
