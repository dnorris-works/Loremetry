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

**Docker (Postgres only)** — optional local database container:

```bash
docker compose -f docker-compose.db.yml up -d
export DATABASE_URL=postgres://loremetry:loremetry@localhost:5432/loremetry
```

The `lore` schema and tables are created automatically on first API boot.

### Run the app

```bash
# Build the UI once (or use `cd ui && npm run dev` while developing)
cd ui && npm install && npm run build && cd ..

# Terminal 1 — API
export DATABASE_URL=postgres://localhost:5432/loremetry
export STATIC_DIR=./ui/dist PORT=8080
cargo run -p loremetry-web

# Terminal 2 — Vue dev server (proxies /api → :8080)
cd ui && npm run dev
```

Optional env secrets (also used on Miget):

- `DATABASE_URL` — PostgreSQL connection string (required on Miget)
- `ANTHROPIC_API_KEY` / `TOKENMIX_API_KEY`
- `CANOPY_API_KEY`
- `DATAFORSEO_LOGIN` / `DATAFORSEO_PASSWORD`

## Production (Miget via GitHub)

Deploy as a **single application** — **not** a Compose Stack. There is **no `Dockerfile`**; Miget **Buildpacks** build the app from `app.json`, `package.json`, and `Cargo.toml`.

1. **Miget:** connect GitHub → **New application** → this repo and branch.
2. **Builder:** **Miget Buildpacks** (Auto detection). Do **not** select Dockerfile.
3. **Database:** shared project Postgres → set **`DATABASE_URL`** on the app.
4. **Other config vars:** `ANTHROPIC_API_KEY`, `CANOPY_API_KEY`, etc. Miget sets **`PORT`** (usually `5000`); the app binds to `PORT`.
5. **Auto-deploy** on push.

### What Miget builds

| Step | Source |
|------|--------|
| Vue UI | Root `package.json` → `npm run build` in `ui/` |
| Rust API | `cargo build --release -p loremetry-web` (`BUILD_COMMAND` in `app.json`) |
| Start | `Procfile` → `web: bin/start` (finds `./app` or `./loremetry-web`) |

### Inspecting the database (SQL, schema)

**In the app:** **Admin** → **SQL console**.

```sql
\dt lore.*
SELECT COUNT(*) FROM lore.kdp_categories;
```

### App deploys but crashes / CrashLoopBackOff

Check **runtime logs** (not build logs). Common causes:

| Log message | Fix |
|-------------|-----|
| `/bin/sh: ./app: not found` | Rust binary not at `./app` in the image. `Procfile` should use `bin/start` (this repo) — push and redeploy. |
| `DATABASE_URL must be a postgres://…` / `does not look like postgres` | The var is a **placeholder or secret id**, not the real URL. In Miget, link the **shared DB** to the app or paste the full `postgres://user:pass@host:5432/db` string. Set on **Run** config vars, not build-only. |
| `DATABASE_URL is not set` | App has no runtime URL. Use project **link** to inject `POSTGRES_*_URL`, or set `DATABASE_URL` on **this application**. |
| `Database init failed` | Wrong host/credentials, DB not reachable from the app network, or SSL. Try `DATABASE_SSLMODE=require` (or `disable` for internal Miget Postgres). |
| `Bind failed` | Rare; check `PORT` (Miget usually sets `5000`). |

Startup waits up to ~60s for Postgres (retries). If the health check is shorter, the pod may restart before DB connects — check DB hostname is reachable from the app.

**Note:** Linux env names are case-sensitive — use `DATABASE_URL`, not `database_url`.

**`cp target/release/` failed** — Rust buildpack on a workspace. Ensure **rust + nodejs** buildpacks in `app.json` and `BUILD_COMMAND` in `app.json`. Redeploy.

**UI 404 / empty** — set **`STATIC_DIR=/app/ui/dist`** on the app (default in `app.json`). Confirm the Node build step ran (`ui/dist` exists in the image).

**Wrong builder** — **Settings → Builders → Miget Buildpacks**, not Dockerfile.

### Build logs mention Docker / `Dockerfile.runtime`

Normal. **Miget Buildpacks** (migetpacks) always compile your app by generating a temporary **`Dockerfile.runtime`** and running **BuildKit** — even when you did not add a `Dockerfile` to the repo. That is not the same as choosing **Builder → Dockerfile** (your own root `Dockerfile`).

If the build fails on `rust:stable: not found`, the platform mirror is missing that tag. This repo pins **`rust-toolchain.toml`** to a concrete version (e.g. `1.85.0`) instead of `stable`.
