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
- `SECRETS_ENCRYPTION_KEY` — 32-byte key, base64-encoded (required in production for encrypted platform credentials)
- `BOOTSTRAP_ADMIN_EMAIL` — email for the first admin user when the database has no users (default `admin@local`)
- `ANTHROPIC_API_KEY` / `TOKENMIX_API_KEY` — seeded into encrypted storage on first boot if the secrets table is empty
- `CANOPY_API_KEY`
- `DATAFORSEO_LOGIN` / `DATAFORSEO_PASSWORD`

Platform API keys are stored encrypted in Postgres (`lore.platform_secrets`) and managed in **Admin → Platform credentials**. The browser no longer sends API keys on analysis or chat requests.

## Production (Miget via GitHub)

You should only need **GitHub → this repo → deploy** (same as your other app). Miget builds from what’s **in the repo** — you don’t run Docker locally or pick a separate “Dockerfile deploy” product in the UI.

What worked before: a root **`Dockerfile`** in git plus a minimal **`app.json`** (`LANGUAGE=dockerfile`). Miget builds that image on push.

What broke **Loremetry** on `main`: the **`Dockerfile` was deleted** and **`app.json` pointed at Rust + Node buildpacks** (`BUILD_COMMAND`, `Procfile`, root `package.json`). That’s when you started seeing `./app: not found` and `cargo: not found` — not because the app name changed.

After you push the fix ( **`Dockerfile` back**, buildpack cruft removed):

1. **GitHub app** → Loremetry repo → deploy / auto-deploy on push (same flow as the working app).
2. If this app still fails but the other one doesn’t, open **Settings → Variables** on Loremetry and **delete leftovers** from the bad period: `BUILD_COMMAND`, `LANGUAGE=rust`, anything forcing buildpacks.
3. **Postgres:** DB addon on the app (`DATABASE_URL`) or project `POSTGRES_*_URL` — the server accepts both.

The container runs **`/app/loremetry-web`** on Miget’s **`$PORT`**.

### Inspecting the database (SQL, schema)

**In the app:** **Admin** → **SQL console**.

```sql
\dt lore.*
SELECT COUNT(*) FROM lore.kdp_categories;
```

### Deploy / runtime troubleshooting

| Symptom | Fix |
|---------|-----|
| `cargo: not found` during build | Repo or app vars still on **buildpacks**. Commit root **`Dockerfile`**, restore minimal **`app.json`**, remove `BUILD_COMMAND` / `LANGUAGE=rust` on the Miget app, redeploy from GitHub. |
| `./app: not found` | Same — buildpack image. Push **`Dockerfile`** + `CMD /app/loremetry-web`; redeploy from GitHub. |
| `DATABASE_URL` / DB errors | Real `postgres://…` at runtime; check logs for `FATAL` / `Database init failed`. |

Optional local production image:

```bash
docker build -t loremetry .
docker run -p 5000:5000 -e DATABASE_URL=... -e PORT=5000 loremetry
```
