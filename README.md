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
# Terminal 1 — API
export DATABASE_URL=postgres://localhost:5432/loremetry   # or the Docker URL above
export STATIC_DIR=./ui/dist PORT=8080
cargo run -p loremetry-web

# Terminal 2 — Vue (proxies /api → :8080)
cd ui && npm install && npm run dev
```

Optional env secrets (also used on Miget):

- `DATABASE_URL` — PostgreSQL connection string (required on Miget)
- `ANTHROPIC_API_KEY` / `TOKENMIX_API_KEY`
- `CANOPY_API_KEY`
- `DATAFORSEO_LOGIN` / `DATAFORSEO_PASSWORD`

## Production (Miget via GitHub)

Deploy as a **single application** from this repo — **not** a Docker Compose Stack.

1. **Miget:** Workspace Settings → **Git Credentials** → **Connect GitHub** → install the Miget app on this repo.
2. **New application** (not “Compose Stack”) → source **GitHub** → select the `Loremetry` repo and branch (e.g. `main`).
3. **Builder:** **Docker Engine** (builds the root **`Dockerfile`**: Vue + Rust in one image).
   - If Miget only offers buildpacks: **Settings → Variables** → `LANGUAGE` = `dockerfile` → redeploy.
4. **Database:** use your **shared project Postgres**. On the **application**, set **`DATABASE_URL`** to that connection string (project variables can supply it). Migrations create the `lore` schema on startup.
5. **Other variables** on the app: `ANTHROPIC_API_KEY`, `CANOPY_API_KEY`, etc. Miget sets **`PORT`**; the app listens on whatever `PORT` is.
6. Enable **Auto-deploy** on push.

No `compose.miget.yml`, no `docker-compose.yml` in this repo for production — those were for Compose Stack experiments only.

### Inspecting the database (SQL, schema)

**In the app:** **Admin** → **SQL console**.

Loremetry tables are in the **`lore`** schema. Migration metadata is in `public._sqlx_migrations`.

**From your Mac** (`psql`, TablePlus, DBeaver): use the shared DB connection string from Miget.

```sql
\dt lore.*
SELECT COUNT(*) FROM lore.kdp_categories;
```

### “Unable to detect language” on deploy

| Fix | What to do |
|-----|------------|
| **Recommended** | App **Settings → Deployment** → **Docker Engine** → redeploy |
| **Buildpacks** | **Variables** → `LANGUAGE` = `dockerfile` → redeploy |

Do **not** use `LANGUAGE=rust` or `nodejs` alone — only the `Dockerfile` builds both UI and API.

### Optional: run the production image locally

```bash
docker compose -f docker-compose.db.yml up -d
docker build -t loremetry .
docker run -p 8080:8080 \
  -e DATABASE_URL=postgres://loremetry:loremetry@host.docker.internal:5432/loremetry \
  -e PORT=8080 \
  -e ANTHROPIC_API_KEY=... \
  loremetry
```

On Linux, use `--network host` or a reachable `DATABASE_URL` host.
