# Loremetry

Single-user Vue + Rust (Axum) app for fiction market/craft analysis. Ships as a **Docker image**; run it on any host that provides Postgres and a few server env vars.

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

### Server environment (any host)

These are **infrastructure** settings only. Provider API keys, Clerk, and other operator config live in **Postgres** (`lore.platform_secrets`) via **Admin → Platform credentials**.

| Variable | Purpose |
|----------|---------|
| `DATABASE_URL` | PostgreSQL connection string (`postgres://…`). Also accepts `POSTGRES_URL`, `POSTGRESQL_URL`, or any `POSTGRES_<name>_URL` from a managed addon. |
| `SECRETS_ENCRYPTION_KEY` | Base64-encoded 32-byte key (`openssl rand -base64 32`). Required in production to encrypt API secrets in the DB. **Keep this with your database backup** when you change hosts. |
| `BOOTSTRAP_ADMIN_EMAIL` | Optional. First admin user when the DB has no users and Clerk is not configured (default `admin@local`). |
| `PORT` | HTTP listen port (default `8080`; many platforms set this automatically). |
| `STATIC_DIR` | Path to built Vue assets (default `./ui/dist`; set in the Docker image). |
| `DATABASE_SSLMODE` | Optional. `prefer` (default for remote hosts), `disable` for localhost. |
| `MAX_BODY_MB` | Optional upload limit (default 256). |

### Moving to another host

1. **Database** — `pg_dump` / restore (or attach the same Postgres from the new network). Migrations run on boot.
2. **`SECRETS_ENCRYPTION_KEY`** — set the **same** value on the new host before serving traffic. Without it, encrypted credentials in `lore.platform_secrets` cannot be decrypted.
3. **Operator config** — already in the DB (API keys, Clerk issuer/publishable key, stories, usage). No Miget- or host-specific secrets to re-enter if the DB moved intact.
4. **`DATABASE_URL`** — point at the database from the new runtime.
5. **Clerk** — add the new app URL to Clerk **allowed origins** / redirect URLs if the domain changed.
6. **Deploy** — build from the repo `Dockerfile` (or pull your image) and set the env vars above.

### Clerk (sign-in + admin role)

1. Create a Clerk application. In Loremetry **Admin → Platform credentials**, set **Clerk publishable key** and **JWT issuer** (Clerk → **API keys** → “Frontend API URL”, without a trailing slash). Save, then reload the app so sign-in initializes with the publishable key.
2. **Your user (admin):** Clerk Dashboard → **Users** → your user → **Public metadata**:

   ```json
   { "role": "admin" }
   ```

3. **Session token** (required so the API sees role/email): Clerk → **Sessions** → **Customize session token** → add:

   ```json
   {
     "role": "{{user.public_metadata.role}}",
     "email": "{{user.primary_email_address}}"
   }
   ```

4. Everyone else: leave public metadata empty or `{ "role": "user" }` — they get `subscriber` in Postgres and **cannot** open Admin (platform secrets, SQL, WinningCat, usage).

5. Redeploy. Sign in via Clerk; API calls send `Authorization: Bearer <session token>` automatically.

## Production deploy

**Portable:** root **`Dockerfile`** + `cargo run` target **`loremetry-web`**. Entrypoint listens on **`$PORT`** and serves the UI from **`STATIC_DIR`** baked into the image.

```bash
docker build -t loremetry .
docker run -p 8080:8080 \
  -e DATABASE_URL=postgres://… \
  -e SECRETS_ENCRYPTION_KEY=… \
  -e PORT=8080 \
  loremetry
```

### Miget (current host)

GitHub → repo → deploy with **`app.json`** (`LANGUAGE=dockerfile`). Miget builds the Dockerfile on push.

1. **GitHub app** → Loremetry repo → deploy / auto-deploy on push.
2. If builds fail with `cargo: not found` or `./app: not found`, remove leftover buildpack vars (`BUILD_COMMAND`, `LANGUAGE=rust`) and ensure the **`Dockerfile`** is on `main`.
3. **Postgres:** `DATABASE_URL` on the app or a project `POSTGRES_*_URL` — the server discovers any of these automatically.

### Inspecting the database (SQL, schema)

**In the app:** **Admin** → **SQL console**.

```sql
\dt lore.*
SELECT COUNT(*) FROM lore.kdp_categories;
```

### Deploy / runtime troubleshooting

| Symptom | Fix |
|---------|-----|
| `cargo: not found` during build | Host still using **buildpacks**. Deploy via the repo **`Dockerfile`**; remove `BUILD_COMMAND` / `LANGUAGE=rust` if set. |
| `./app: not found` | Same — use Docker image with `CMD /app/loremetry-web`. |
| `DATABASE_URL` / DB errors | Real `postgres://…` at runtime; check logs for `FATAL` / `Database init failed`. |
| Pod **CrashLoopBackOff** right after secrets deploy | `SECRETS_ENCRYPTION_KEY` must be a **base64 key**, not the text `openssl rand -base64 32`. Run `openssl rand -base64 32` locally, set that one line in the server environment, redeploy. |
| `relation "_sqlx_migrations" does not exist` during `Database init failed` | Usually migration 002 left `search_path` on `lore` so sqlx could not see `public._sqlx_migrations`. Deploy the fix (migrate pool forces `public` search_path; migration 002 no longer sets `search_path`). If the DB is stuck, ensure `public._sqlx_migrations` exists (redeploy) or create it from a working sqlx migrate on another env. |
| `migration … was previously applied but has been modified` | Migration 002 was recorded, then the file in git changed. If `lore.users` already exists: `DELETE FROM public._sqlx_migrations WHERE version = 2;` then redeploy (002 is idempotent). Otherwise fix migration rows only if you know the schema is already correct. |

