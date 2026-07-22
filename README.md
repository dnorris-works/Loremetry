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
cargo run -p loremetry-web --bin app

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
| Rust API | `cargo build --release -p loremetry-web --bin app` (`BUILD_COMMAND` in `app.json`) |
| Start | `Procfile` → `web: ./app` |

### Inspecting the database (SQL, schema)

**In the app:** **Admin** → **SQL console**.

```sql
\dt lore.*
SELECT COUNT(*) FROM lore.kdp_categories;
```

### Build troubleshooting

**`cp target/release/` failed** — Rust-only buildpack on a workspace with no root binary. This repo names the release binary **`app`** and sets `BUILD_COMMAND` in `app.json`. Ensure **nodejs + rust** buildpacks are used (declared in `app.json`) and redeploy.

**UI 404 / empty** — set **`STATIC_DIR=/app/ui/dist`** on the app (default in `app.json`). Confirm the Node build step ran (`ui/dist` exists in the image).

**Wrong builder** — **Settings → Builders → Miget Buildpacks**, not Dockerfile.

### Build logs mention Docker / `Dockerfile.runtime`

Normal. **Miget Buildpacks** (migetpacks) always compile your app by generating a temporary **`Dockerfile.runtime`** and running **BuildKit** — even when you did not add a `Dockerfile` to the repo. That is not the same as choosing **Builder → Dockerfile** (your own root `Dockerfile`).

If the build fails on `rust:stable: not found`, the platform mirror is missing that tag. This repo pins **`rust-toolchain.toml`** to a concrete version (e.g. `1.85.0`) instead of `stable`.
