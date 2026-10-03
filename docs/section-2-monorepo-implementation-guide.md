# Implementing §2 From Scratch — Step by Step

**Companion to:** [`refactor-plan-document.md`](refactor-plan-document.md) §2 (lines 31–176)
**Approach:** fresh clone → new branch → new tree at the repo root. The old tree on
`master` is never modified and stays as the reference and the fallback.
**Scope decision:** compile + tests + structural fixes. Known behavioural defects are
carried over as-is and tracked (see [What is deliberately carried over as broken](#what-is-deliberately-carried-over-as-broken)).

---

## What changed from the previous version of this guide

The earlier draft assumed you would restructure the existing checkout in place. You are
building the target layout in a **new folder on a new branch**, copying logic across and
making small changes. Three consequences run through the whole document:

|                         | In-place restructure                               | **From-scratch port (this version)**                                                  |
| ----------------------- | -------------------------------------------------- | ------------------------------------------------------------------------------------- |
| Source tree             | modified as you go                                 | `master` stays pristine; you only ever read from it                                   |
| The 43 compile errors   | a gate you must clear **before** touching anything | irrelevant — you never build the old tree. You regenerate `.sqlx` in the **new** tree |
| Step 0                  | fix the build in place, then move files            | stand up infra + apply migrations, then copy and fix                                  |
| Recovery from a mistake | `git revert` (state is already committed)          | `git checkout master -- <path>` — instant, no commit needed                           |
| Review surface          | 20 sequential commits                              | one big obvious "pure copy" commit, then small structural ones                        |
| Effort                  | higher (must stay green throughout)                | lower, but you must be disciplined about the copy being _pure_                        |

The last row is the real trade: you get a much safer process in exchange for being able
to make one large, boring, mechanical copy commit before any interesting work starts.

### The most important new idea

Because `master` is intact, you have a recovery primitive that did not exist before:

```bash
# Pull any file from the old tree into the new one, any time, with no commit required.
git checkout master -- apps/api/src/modules/broadcast/service.rs

# Or just look at it without touching your tree.
git show master:apps/api/src/modules/broadcast/service.rs | less
```

This is why the plan is safe to execute in a large number of steps. **If a port step
goes wrong, you restore the reference file and retry** — you never have to reverse-engineer
what the old version said, because it is always one command away.

---

## The strategy in one paragraph

Three phases, and the boundaries matter:

1. **Port (pure copy).** Get the entire existing API into the new layout with the
   minimum possible edits, compiling and booting. This is one large commit that should
   be _boring to review_ — if it isn't boring, you are editing logic during the port and
   should stop. You end with a checkpoint that is byte-for-byte the old behaviour, in
   the new shape.
2. **Structure.** Now change things: split `shared/`, split the binaries, add adapter
   traits, split `BroadcastService`, consolidate errors. Each of these is a separate
   commit that returns to green. Because the port checkpoint is known-good, any
   regression is bisectable to exactly one structural commit.
3. **Infrastructure.** `ops/`, Dockerfile, CI, Makefile, placeholders. None of this
   touches Rust, so it cannot break phase 1 or 2.

The mistake people make is starting at step 2. If you restructure `shared/` and split
the service _while_ porting, then when something breaks you cannot tell whether the port
was wrong or the split was wrong. Do them separately.

---

## Ground rules

| Rule                                                                           | Why                                                                                                                                                                                |
| ------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **The port phase copies code; it does not rewrite it.**                        | The value of a from-scratch port is that you inherit 19,860 lines of _working_ domain logic. Editing during the port makes review impossible and merges two risks into one commit. |
| **`master` is read-only.** Never commit to it, never check out over your work. | It is your reference _and_ your fallback. Its value is entirely in being untouched.                                                                                                |
| **One commit per structural change, each returning to green.**                 | A bisect that lands on "refactor(broadcast): split service" is useless if that commit is 3,000 lines of mixed changes.                                                             |
| **Never move and edit in the same commit.**                                    | Keeps `git blame` meaningful and makes a bad commit trivially revertable.                                                                                                          |
| **`crates/core` stays dependency-free** — no `sqlx`, `axum`, `fred`, `tokio`.  | This is what makes the cursor and error logic testable in milliseconds. If you need one of those, the type belongs in `apps/api`.                                                  |
| **Delete the old tree in the port commit, not gradually.**                     | A half-migrated tree confuses the compiler and every reviewer. One commit: old layout out, new layout in.                                                                          |

---

# Phase 1 — Port (pure copy)

The goal of this phase is a compiling, booting API in the new layout, with zero
intentional behaviour change. No new features, no refactors, no "while I'm here".

## Step 0 — Branch, clone, and keep the reference

```bash
# 1. Clone fresh. This is the new folder.
git clone <your-repo-url> meno-v2
cd meno-v2

# 2. Branch off master. The new tree goes at the repo root of this branch.
git checkout -b refactor/monorepo

# 3. Keep a read-only alias for the old tree. This is optional but very useful —
#    it lets you write `old:` instead of `master:` and it documents intent.
git branch old-archive master

# 4. Confirm the reference is intact and note its commit.
git log -1 --oneline master
# 903c3ba is the reviewed baseline for everything in this guide.
```

### Step 0.1 — Clean slate on this branch

Delete the old layout in a single commit, so the branch starts from an honest empty
state rather than a half-overlaid one:

```bash
git rm -r -q apps packages resources .docker
git rm -q dockerfile docker-compose.yml promtail-config.yml .env.example .sqlx
git commit -m "chore: clear pre-monorepo layout

The monorepo target tree is built from scratch on this branch. master remains
the untouched reference and fallback; nothing here is lost.

Rationale for removing rather than overlaying: a half-migrated tree confuses the
compiler and makes review impossible to scope."
```

> `.sqlx` is deleted on purpose. The old cache is the source of the 43 errors and is
> regenerated from scratch in Step 1 against a real migrated database. Committing it
> forward would mean carrying the staleness into the new tree.
>
> `.env.example` is also removed — the new one is generated from the new typed config
> in Step 6, so writing it now would just mean writing it twice.

You now have an empty branch with full access to the old tree:

```bash
ls apps/            # gone
git show master:apps/api/src/modules/broadcast/service.rs | wc -l   # 1605
```

---

## Step 1 — Local dev infrastructure, from nothing

This is the only hard prerequisite, and it is genuinely the _only_ one: **you never need
the old tree to compile.** You only need a database with the real schema so `sqlx` can
generate correct metadata, and a Redis plus object store so the app can boot.

Nothing below assumes anything already exists. If you are starting on a bare machine,
this whole step is the path from "no containers at all" to "API boots".

### What you are building

| Service          | Image              | Host port   | Purpose                                 | Persistent?           |
| ---------------- | ------------------ | ----------- | --------------------------------------- | --------------------- |
| **postgres**     | `postgres:18`      | 5432        | the entire schema                       | yes (`pgdata` volume) |
| **redis**        | `redis:8-alpine`   | 6379        | cache, rate limits, pub/sub, OTP        | **no** — matches prod |
| **storage**      | `rustfs/rustfs`    | 9000 / 9001 | S3-compatible storage, stands in for R2 | yes (`storage_data`)  |
| **storage-init** | `amazon/aws-cli`   | —           | creates the bucket, then exits          | no                    |
| **mailpit**      | `axllent/mailpit`  | 1025 / 8025 | catches SMTP, shows it in a web UI      | no                    |
| **prometheus**   | `prom/prometheus`  | 9090        | scrapes `/metrics`                      | no                    |
| **loki**         | `grafana/loki`     | 3100        | log aggregation                         | no                    |
| **promtail**     | `grafana/promtail` | —           | ships container logs to Loki            | no                    |
| **grafana**      | `grafana/grafana`  | 3001        | dashboards                              | yes (`grafana_data`)  |

> **The API itself is deliberately NOT in this list.** In development you run the API
> **natively** with `cargo run`, not in a container — you get incremental rebuilds, a
> working debugger, and instant reload. Docker is only for the services it would take
> hours to install natively. A containerised `api` service appears in Step 13, purely so
> you can verify the Dockerfile builds and runs; it is not your dev loop.

> **Redis has `--appendonly no` on purpose.** Render Key Value's free plan has **no
> persistence** (§3.2). A local Redis that survives a restart while production's does not
> means you never reproduce the "OTP state lost on restart" bug until it happens in
> production. Match the failure modes you will actually have.

---

### Step 1.1 — Start Docker Desktop (Windows)

You have Docker Desktop 29.8.0 with Compose v5.5.1 installed, but **the daemon is not
running**, which is why every `docker` command fails with:

```
failed to connect to the docker API at npipe:////./pipe/dockerDesktopLinuxEngine
```

That is the expected symptom of a stopped daemon, not a broken install. Start it:

```bash
# Start the GUI and wait for the whale icon to settle on "running".
docker-desktop &

# Or poll until the daemon answers. This takes 20–40s on first launch.
until docker info >/dev/null 2>&1; do
  printf '.'
  sleep 2
done
echo
docker info --format 'Server {{.ServerVersion}} · {{.OSType}} · {{.NCPU}} CPUs · {{.MemTotal}} bytes RAM'
```

**Your WSL setup is already correct.** `wsl --status` reports Ubuntu as the default
distribution and confirms "WSL1 is not supported by your current machine
configuration" — that message only appears when you are already on **WSL2**, which is
what Docker Desktop requires. No action needed.

If the daemon still will not start, check these in order:

1. **Resources.** Docker Desktop → Settings → Resources. Give it at least **4 GB** RAM
   and 2 CPUs. The default 2 GB is not enough for Postgres + Redis + RustFS + Grafana +
   Loki + Prometheus together, and the symptom is the OOM killer taking Postgres down
   mid-migration.
2. **Disk.** Settings → Resources → Disk Image, at least 20 GB. Six services pull roughly
   3 GB of images.
3. **Reboot the engine.** Settings → Troubleshoot → "Clean / Purge data" is the last
   resort; it deletes volumes, so do it before Step 1.3, never after.

---

### Step 1.2 — Write the dev compose file

Create `ops/docker-compose.yml`. This is the **backing services only** — the `api`
service is added in Step 13.

```yaml
# ops/docker-compose.yml — local development backing services.
#
# The API runs NATIVELY (`cargo run --bin meno-api`), not in a container, so it is not
# in this file. See Step 13 for a compose service whose only job is proving the
# Dockerfile works.

name: meno-dev

services:
  postgres:
    image: postgres:18
    container_name: meno-pg
    environment:
      POSTGRES_DB: meno_dev
      POSTGRES_USER: meno
      POSTGRES_PASSWORD: password
    ports:
      - "5432:5432"
    volumes:
      # Postgres 18 images changed the mount convention. The data directory is now
      # major-version-specific (`/var/lib/postgresql/18/docker`), so the volume mounts
      # at `/var/lib/postgresql`, NOT `/var/lib/postgresql/data`. Mounting the old
      # path makes the container exit 1 immediately with a long message about
      # "unused mount/volume" and pg_upgrade. This is a real breaking change in
      # postgres:18 — see the troubleshooting table.
      - pgdata:/var/lib/postgresql
    healthcheck:
      # Without this, `depends_on: service_healthy` below is meaningless and the API
      # starts before Postgres is accepting connections.
      test: ["CMD-SHELL", "pg_isready -U meno -d meno_dev"]
      interval: 3s
      timeout: 3s
      retries: 10
      start_period: 10s
    networks: [meno]
    restart: unless-stopped

  redis:
    image: redis:8-alpine
    container_name: meno-redis
    # No volume, and --appendonly no: Render KV free has no persistence, so local must
    # behave the same or you will not reproduce cache-loss bugs locally.
    command: >
      redis-server
      --maxmemory 256mb
      --maxmemory-policy allkeys-lru
      --save ""
      --appendonly no
    ports:
      - "6379:6379"
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 3s
      timeout: 3s
      retries: 10
    networks: [meno]
    restart: unless-stopped

  # S3-compatible object storage standing in for Cloudflare R2.
  #
  # NOT MinIO: the community edition's Docker Hub repositories were deleted on
  # 11 September 2026, so `minio/minio` now 404s and `docker compose up` cannot pull
  # it. RustFS is an Apache-2.0 Rust implementation that speaks the same S3 API, which
  # is all `object_store` needs. See the image-pinning note below.
  storage:
    image: rustfs/rustfs
    container_name: meno-storage
    environment:
      RUSTFS_ACCESS_KEY: rustfsadmin
      RUSTFS_SECRET_KEY: rustfsadmin
      RUSTFS_ADDRESS: ":9000"
      RUSTFS_CONSOLE_ADDRESS: ":9001"
      RUSTFS_CONSOLE_ENABLE: "true"
    ports:
      - "9000:9000" # S3 API
      - "9001:9001" # web console
    volumes:
      - storage_data:/data
    healthcheck:
      # RustFS exposes /health; there is no `mc ready` subcommand inside the image.
      test: ["CMD-SHELL", "curl -fsS http://localhost:9000/health || exit 1"]
      interval: 5s
      timeout: 5s
      retries: 12
      start_period: 10s
    networks: [meno]
    restart: unless-stopped

  # One-shot: creates the bucket, then exits. Running it as a service means a fresh
  # clone plus `docker compose up` yields a working object store with no manual step.
  # `minio/mc` is gone along with MinIO, so this drives the S3 API with the AWS CLI
  # pointed at the RustFS endpoint — same bucket, no MinIO dependency.
  storage-init:
    # NOTE: there is no `amazon/aws-cli:2` tag — it 404s. The project publishes dated
    # patches (2.37.x) plus `latest`, and ships a new one most days. Pinning a patch
    # here would mean chasing daily releases for a stateless one-shot container, so
    # `latest` is the deliberate choice, consistent with the observability images.
    image: amazon/aws-cli:latest
    container_name: meno-storage-init
    depends_on:
      storage:
        condition: service_healthy
    environment:
      AWS_ACCESS_KEY_ID: rustfsadmin
      AWS_SECRET_ACCESS_KEY: rustfsadmin
      AWS_DEFAULT_REGION: us-east-1
      AWS_EC2_METADATA_DISABLED: "true"
    networks: [meno]
    # `entrypoint` as a LIST plus `command` as a single-element LIST containing a literal
    # block. Both details are load-bearing:
    #
    #  - A string `command:` is split on whitespace by Compose and truncated at the first
    #    newline, so `sh -c` receives only `set` and dumps the environment instead of
    #    running the script. A one-element list passes the script through intact.
    #  - `set -e` is what makes a failure visible. Without it the script exits 0 even
    #    when every `aws` call fails, and the misleading "bucket ready" still prints.
    #
    # There are no `$` variables anywhere: Compose interpolates `$NAME` in the file
    # itself before the shell ever sees it, so a shell variable would be silently
    # substituted to an empty string.
    entrypoint: ["/bin/sh", "-c"]
    command:
      - |
        set -e
        aws --endpoint-url http://storage:9000 s3api head-bucket --bucket meno-uploads \
          || aws --endpoint-url http://storage:9000 s3api create-bucket --bucket meno-uploads
        aws --endpoint-url http://storage:9000 s3api put-public-access-block \
          --bucket meno-uploads \
          --public-access-block-configuration \
            BlockPublicAcls=true,IgnorePublicAcls=true,BlockPublicPolicy=true,RestrictPublicBuckets=true
        echo 'bucket meno-uploads ready'
    restart: "no"

  # SMTP catcher. The API still speaks SMTP in this branch (Brevo replaces it later),
  # so without this you would have to configure a real SMTP provider just to test a
  # password-reset email.
  mailpit:
    image: axllent/mailpit:latest
    container_name: meno-mailpit
    environment:
      MP_SMTP_AUTH_ACCEPT_ANY: 1
      MP_SMTP_AUTH_ALLOW_INSECURE: 1
    ports:
      - "1025:1025" # SMTP
      - "8025:8025" # web UI
    networks: [meno]
    restart: unless-stopped

  prometheus:
    image: prom/prometheus:latest
    container_name: meno-prometheus
    volumes:
      - ./prometheus/prometheus.yml:/etc/prometheus/prometheus.yml:ro
      - prom_data:/prometheus
    ports:
      - "9090:9090"
    networks: [meno]
    restart: unless-stopped

  loki:
    image: grafana/loki:latest
    container_name: meno-loki
    command: -config.file=/etc/loki/local-config.yaml
    volumes:
      - ./promtail/loki.yml:/etc/loki/local-config.yaml:ro
    ports:
      - "3100:3100"
    networks: [meno]
    restart: unless-stopped

  promtail:
    image: grafana/promtail:latest
    container_name: meno-promtail
    command: -config.file=/etc/promtail/config.yml
    volumes:
      # Only works on Docker Desktop, where /var/run/docker.sock is bind-mountable.
      # On Linux you must add the socket to the daemon's systemd unit instead.
      - /var/run/docker.sock:/var/run/docker.sock:ro
      - ./promtail/promtail.yml:/etc/promtail/config.yml:ro
    depends_on: [loki]
    networks: [meno]
    restart: unless-stopped

  grafana:
    image: grafana/grafana:latest
    container_name: meno-grafana
    environment:
      GF_SECURITY_ADMIN_USER: admin
      GF_SECURITY_ADMIN_PASSWORD: admin
      GF_USERS_ALLOW_SIGN_UP: "false"
    volumes:
      - ./grafana/provisioning:/etc/grafana/provisioning:ro
      - grafana_data:/var/lib/grafana
    ports:
      - "3001:3000" # host 3001, because 3000 is conventionally Next.js
    depends_on: [prometheus, loki]
    networks: [meno]
    restart: unless-stopped

networks:
  meno:
    name: meno
    driver: bridge

volumes:
  pgdata:
  storage_data:
  grafana_data:
  prom_data:
```

The config files it references (`prometheus.yml`, `promtail.yml`, `loki.yml`,
`grafana/provisioning/*`) are written in Step 12. **For the rest of this guide you only
need postgres, redis and storage** — create just those three first, and bring up the
observability stack in Step 12 when you have a binary actually emitting metrics.

That is why the Makefile targets them individually rather than `up -d`:

```makefile
db-up: ## Start Postgres, Redis and the object store
	docker compose -f ops/docker-compose.yml up -d postgres redis storage
```

---

### Step 1.3 — First run

```bash
cd /f/projects/personal/meno   # or your clone of it

# Pull images explicitly first, so you see which ones are slow and which fail.
docker compose -f ops/docker-compose.yml pull postgres redis storage

# Start the three services the port needs.
docker compose -f ops/docker-compose.yml up -d postgres redis storage

# Watch until healthy.
docker compose -f ops/docker-compose.yml ps
```

Expected `ps` output — all three `healthy`, not merely `running`. On Compose v2.5+ the
table carries seven columns, and each published port appears twice — once for IPv4, once
for IPv6:

```
NAME            IMAGE            COMMAND                  SERVICE    CREATED         STATUS                        PORTS
meno-postgres   postgres:18      "docker-entrypoint.s…"   postgres   20 seconds ago  Up 19 seconds (healthy)      0.0.0.0:5432->5432/tcp, [::]:5432->5432/tcp
meno-redis      redis:8-alpine   "docker-entrypoint.s…"   redis      20 seconds ago  Up 19 seconds (healthy)      0.0.0.0:6379->6379/tcp, [::]:6379->6379/tcp
meno-storage    rustfs/rustfs    "/entrypoint.sh rust…"   storage    20 seconds ago  Up 19 seconds (healthy)      0.0.0.0:9000-9001->9000-9001/tcp, [::]:9000-9001->9000-9001/tcp
```

> **`Restarting (n)` in the STATUS column is a crash, not a transient state.** It means
> the container process exited with code `n` and `restart: unless-stopped` is relaunching
> it in a loop — `CREATED` stays at "5 minutes ago" while `STATUS` keeps resetting. That
> is different from `Up … (unhealthy)`, where the process is alive and only the
> healthcheck fails. Read the logs:
>
> ```bash
> docker compose -f ops/docker-compose.yml logs --tail 50 postgres
> ```
>
> Postgres `Restarting (1)` almost always means the volume mount is wrong — see the next
> row of the troubleshooting table.

---

### Step 1.4 — Verify each service individually

Never assume `Up (healthy)` means reachable. Prove it.

```bash
# ── Postgres ────────────────────────────────────────────────────────────────
docker compose -f ops/docker-compose.yml exec postgres \
  psql -U meno -d meno_dev -c "SELECT version();"
# expect: PostgreSQL 18.x

docker compose -f ops/docker-compose.yml exec postgres \
  psql -U meno -d meno_dev -c "SELECT current_user, current_database();"
# expect: meno | meno_dev

# ── Redis ───────────────────────────────────────────────────────────────────
docker compose -f ops/docker-compose.yml exec redis redis-cli ping
# expect: PONG

docker compose -f ops/docker-compose.yml exec redis \
  redis-cli config get maxmemory-policy
# expect: 1) "maxmemory-policy"  2) "allkeys-lru"
#       ^ if this says "noeviction" your command override did not apply

# ── Object storage (RustFS) ─────────────────────────────────────────────────
curl -sf http://localhost:9000/health && echo "storage live ✅"
# expect: storage live ✅   (RustFS serves /health; there is no /minio/health/live)
```

---

### Step 1.5 — Create the bucket

If you started only postgres/redis/storage, `storage-init` did not run, so no bucket
exists. Create it:

```bash
docker compose -f ops/docker-compose.yml run --rm storage-init
```

Confirm:

```bash
docker compose -f ops/docker-compose.yml run --rm --entrypoint sh storage-init -c \
  "aws --endpoint-url http://storage:9000 s3api list-buckets --output text"
# expect: 2026/..  meno-uploads
```

You can also create it by hand in the console at <http://localhost:9001>
(`rustfsadmin` / `rustfsadmin`) if you prefer a UI.

---

### Step 1.6 — Create the database

The Postgres container creates `meno_dev` on first boot, via `POSTGRES_DB`. But it does
so **only when the data directory is empty** — so if you are reusing a volume from an
earlier attempt, the database may already exist with the wrong owner. Verify rather
than assume:

```bash
docker compose -f ops/docker-compose.yml exec postgres \
  psql -U meno -d postgres -c "\l meno_dev"
```

If that errors, create it explicitly:

```bash
docker compose -f ops/docker-compose.yml exec postgres \
  psql -U meno -d postgres -c "CREATE DATABASE meno_dev OWNER meno;"
```

> **This is the defect in `master`'s `.docker/postgres/init.sql`**, which ran
> `create database meno_dev owner postgres` — a role that compose never creates, since
> `POSTGRES_USER` is `meno`. That file was also mounted by nothing. It is deleted in
> Step 12; you do not need it, because `POSTGRES_DB` already does this job.

---

### Step 1.7 — Generate secrets and write `.env`

```bash
# Two DIFFERENT secrets. Reusing one for access and refresh tokens is a real
# vulnerability: compromising one then lets an attacker mint the other.
echo "JWT_SECRET=$(openssl rand -hex 64)"
echo "JWT_REFRESH_SECRET=$(openssl rand -hex 64)"
```

On Windows without `openssl`, use Rust — you already have the toolchain:

```bash
cargo run --quiet --bin gen-secret 2>/dev/null || python -c "import secrets;print(secrets.token_hex(64))"
```

Write `.env` at the repo root (Step 6.4 replaces this with a generated version, but you
need it now to boot):

```dotenv
# ── required ────────────────────────────────────────────────────────────────
DATABASE_URL=postgres://meno:password@localhost:5432/meno_dev
REDIS_URL=redis://localhost:6379
JWT_SECRET=<paste first value>
JWT_REFRESH_SECRET=<paste second value>
CORS_ORIGINS=http://localhost:3000,http://localhost:3001

# ── local ───────────────────────────────────────────────────────────────────
ENV=dev
PORT=8080
RUST_LOG=info,sqlx=warn

# ── object storage (RustFS, S3-compatible) ──────────────────────────────────────────────────
STORAGE_ENDPOINT=http://localhost:9000
STORAGE_ACCESS_KEY=rustfsadmin
STORAGE_SECRET_KEY=rustfsadmin
STORAGE_BUCKET=meno-uploads
STORAGE_REGION=us-east-1
STORAGE_PUBLIC_URL=http://localhost:9000/meno-uploads

# ── SMTP → Mailpit ──────────────────────────────────────────────────────────
SMTP_HOST=localhost
SMTP_PORT=1025
SMTP_USER=meno
SMTP_PASSWORD=meno
SMTP_FROM=no-reply@meno.local

# ── optional integrations: OFF locally ──────────────────────────────────────
PUSH_ENABLED=false
FIREBASE_PROJECT_ID=
FIREBASE_SERVICE_ACCOUNT_JSON=
LIVEKIT_ENABLED=false
LIVEKIT_API_KEY=
LIVEKIT_API_SECRET=
LIVEKIT_HOST=
GOOGLE_CLIENT_ID=
GOOGLE_CLIENT_SECRET=
GOOGLE_REDIRECT_URI=http://localhost:8080/api/v1/auth/google/callback
GOOGLE_AUTH_URI=https://accounts.google.com/o/oauth2/v2/auth
GOOGLE_TOKEN_URI=https://oauth2.googleapis.com/token

SKIP_MIGRATIONS=false
```

Two deliberate choices:

**`STORAGE_REGION=us-east-1`.** The S3 server ignores it, but the client **signs requests
with it**, so a mismatch between signing and the endpoint produces
`SignatureDoesNotMatch` errors that look like a clock-skew problem. Any fixed value
works as long as it is consistent.

**Push and LiveKit off.** Step 6 makes them genuinely optional. Until then, the copied
`Config::from_env` hard-fails without them — so if the API panics at startup in Step 6,
this is why.

---

### Step 1.8 — The trust boundary: what runs where

```
┌─ your machine ──────────────────────────────────────────────┐
│                                                            │
│   cargo run --bin meno-api        (native, port 8080)      │
│   cargo run --bin meno-worker     (native, no port)        │
│         │                                                   │
│         │  DATABASE_URL / REDIS_URL → localhost:5432/6379 │
│         ▼                                                   │
│   ┌─ Docker Desktop (WSL2 VM) ─────────────────────────┐    │
│   │  meno-pg      :5432                              │    │
│   │  meno-redis   :6379                              │    │
│   │  meno-storage :9000  console :9001               │    │
│   │  meno-mailpit :1025  UI :8025                    │    │
│   │  prometheus :9090  loki :3100  grafana :3001     │    │
│   └───────────────────────────────────────────────────┘    │
└────────────────────────────────────────────────────────────┘
```

The host ports are published so your **native** process can reach them. Containers talk
to each other over the `meno` bridge network by service name (`postgres:5432`, not
`localhost:5432`) — using `localhost` between containers silently connects each
container to itself, which is the single most common compose networking mistake.

---

### Step 1.8b — Why the images are pinned the way they are

Not dogma — each tag below reflects a specific risk.

| Image                                                  | Tag used               | Why                                                                                                                                                                                                                    |
| ------------------------------------------------------ | ---------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `postgres`                                             | **major** (`18`)       | must match Neon exactly. A major bump changes planner behaviour and defaults, so local would stop representing production. Major-only still floats across `18.x`, so you get security patches without cross-major risk |
| `redis`                                                | **major** (`8-alpine`) | the wire protocol is stable across minors; a major bump could change behaviour your code assumes                                                                                                                       |
| `rustfs`                                               | **unpinned**           | read the entry below — this is the exception, and it is a pragmatic one                                                                                                                                                |
| `mailpit`, `prometheus`, `loki`, `promtail`, `grafana` | `latest`               | stateless, no persisted data format you depend on, no SQL, no schema. Worst case is a broken dashboard, not a lost migration                                                                                           |
| `amazon/aws-cli`                                       | `latest`               | stateless one-shot bucket creator. There is no `2` tag; the project publishes dated patches (`2.37.x`) daily, so pinning would mean constant churn for a container that runs once                                      |

**The `postgres` version is the one that genuinely matters**, and it must match the Neon
project. Neon supports 14–18 and **defaults new projects to 18**, so if you create a Neon
project without choosing, you get 18 — and `postgres:18` locally is what keeps
`cargo sqlx prepare`'s output and your migrations honest.

> **Why `rustfs` is unpinned while everything else is pinned.** The honest answer is that
> pinning it would be pinning to something that can vanish. MinIO's community edition had
> its Docker Hub repositories deleted outright on 11 September 2026 — `minio/minio` now
> returns 404, which is a harder failure than any version drift. A pinned tag on a
> project with that publishing history buys reproducibility against a repository that may
> not exist next month. Re-pin to a release tag once RustFS's own publishing settles.
>
> The more transferable lesson: **an unpinned image is a supply-chain dependency you did
> not choose.** For anything that holds data or defines a schema, pin the major. For
> anything disposable, `latest` is fine and cheaper.

### Step 1.9 — Day-to-day commands

```bash
# Start / stop
make db-up
make db-down                    # keeps volumes

# See logs
docker compose -f ops/docker-compose.yml logs -f postgres
docker compose -f ops/docker-compose.yml logs --tail=100 redis

# Nuclear reset — DROPS ALL DATA and rebuilds the schema from zero.
# This is the equivalent of a fresh machine, so use it whenever you suspect
# something is stuck in a bad state.
make db-reset
```

**Use `db-reset` more often than you think.** "The API 500s and I don't know why" is
usually stale schema, and a 20-second reset beats 20 minutes of bisecting.

---

### Step 1.10 — When something does not come up

| Symptom                                                                                    | Cause                                                                                                                                                                                                                 | Fix                                                                                                                                                                                                                                                     |
| ------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `failed to connect to the docker API at npipe://…`                                         | Daemon not running                                                                                                                                                                                                    | Start Docker Desktop; poll `docker info`                                                                                                                                                                                                                |
| Port 5432 already allocated                                                                | A local Postgres, or another stack                                                                                                                                                                                    | `netstat -ano \| findstr :5432`, stop it, or change the host port                                                                                                                                                                                       |
| Postgres `Restarting (1)` in a loop, logs mention _"unused mount/volume"_ and `pg_upgrade` | **`postgres:18` changed its mount convention.** The data directory is now major-version-specific (`/var/lib/postgresql/18/docker`), so the volume must mount at `/var/lib/postgresql`, not `/var/lib/postgresql/data` | Change the `volumes:` entry for postgres to `- pgdata:/var/lib/postgresql`, then `docker compose -f ops/docker-compose.yml up -d --force-recreate postgres`. Pre-18 guides (including older versions of this one) used the `/data` path and crash on 18 |
| Postgres `Up` but never `healthy`                                                          | RAM too low, OOM-killed                                                                                                                                                                                               | Raise Docker Desktop memory to 4 GB+                                                                                                                                                                                                                    |
| `could not translate host name "postgres"`                                                 | Container-to-container name misuse                                                                                                                                                                                    | Use the **service name** inside the compose network, `localhost` only from the host                                                                                                                                                                     |
| `SignatureDoesNotMatch` from the S3 client                                                 | `STORAGE_REGION` inconsistent                                                                                                                                                                                         | Set any fixed value, e.g. `us-east-1`, on both sides                                                                                                                                                                                                    |
| Bucket missing                                                                             | `storage-init` skipped                                                                                                                                                                                                | `docker compose … run --rm storage-init`                                                                                                                                                                                                                |
| Redis policy is `noeviction`                                                               | Command override lost                                                                                                                                                                                                 | Confirm the `command:` block; `docker compose … up -d --force-recreate redis`                                                                                                                                                                           |
| Container restarts in a loop                                                               | Schema/code mismatch after a port                                                                                                                                                                                     | `make db-reset`                                                                                                                                                                                                                                         |
| `sqlx migrate run` hangs                                                                   | Postgres healthy but migrations lock held                                                                                                                                                                             | `make db-reset`, then retry                                                                                                                                                                                                                             |

---

### Step 1.11 — Install sqlx-cli and apply the migrations

```bash
cargo install sqlx-cli --no-default-features --features rustls,postgres --locked
sqlx --version
```

> **Toolchain note.** `rust-toolchain.toml` (Step 3.1) pins **1.88**, so rustup will
> download that toolchain even though your default is **1.97**. That is deliberate:
> pinning is what makes local, CI and the Docker build identical. `resolver = "3"` plus
> `rust-version = "1.88"` then stops Cargo selecting any dependency requiring a newer
> compiler. Without both, a build can pass locally on 1.97 and fail in CI.

Copy the migrations from the reference — this is the first time `master` earns its keep:

```bash
mkdir -p crates/db/migrations
git archive master packages/db/migrations | tar -x --strip-components=3 -C crates/db/migrations
ls crates/db/migrations | wc -l    # 18
```

Apply them:

```bash
sqlx database create || true   # already exists via POSTGRES_DB; harmless
sqlx migrate run
sqlx migrate info              # expect 18 applied, 0 pending
```

### Step 1.12 — Verify the schema, and prove the known defects

The 18 migrations have **never been run against a fresh database** in this project's
history — which is exactly how three dead triggers survived. Now they run, so check:

```bash
psql "$DATABASE_URL" -c "\dt" | head -30
```

If `psql` is not installed on Windows, use the container — it has the client:

```bash
docker compose -f ops/docker-compose.yml exec postgres \
  psql -U meno -d meno_dev -c "\dt"
```

Then prove the trigger defect rather than assuming it:

```bash
docker compose -f ops/docker-compose.yml exec postgres \
  psql -U meno -d meno_dev -c "
  SELECT tgname, pg_get_triggerdef(oid)
  FROM pg_trigger
  WHERE NOT tgisinternal AND tgrelid::regclass::text LIKE 'users%';"
```

Expect a body containing `'TG_OP' = 'INSERT'`. That is a **string literal**, not the
`TG_OP` variable, so the body is unreachable — and `users.followers`, `users.following`
and `users.broadcasts` have never once updated in any environment. Migration `0003`
shows the correct pattern (`IF TG_OP = 'INSERT'`, no quotes); copy that style when the
fixing phase comes.

**This defect is carried over, not fixed, on this branch.** See
[What is deliberately carried over as broken](#what-is-deliberately-carried-over-as-broken).
Checking now means you _know_ the behaviour is wrong rather than suspecting it, and the
tracking issue is written against a verified fact.

### Step 1.13 — Marker for later

`.sqlx` cannot be generated yet, because the new crates do not exist until Step 5:

```bash
# Runs in Step 5.4, once the port compiles against this database:
#   cargo sqlx prepare --workspace -- --all-targets
#   cargo sqlx prepare --workspace --check
echo "TODO: sqlx prepare — Step 5.4"
```

**Step 1 is done when** all of these are true:

- [ ] `docker info` succeeds (daemon running)
- [ ] postgres, redis, storage all report `healthy`
- [ ] `redis-cli ping` → `PONG`, and `maxmemory-policy` is `allkeys-lru`
- [ ] the storage health endpoint returns 200, and `meno-uploads` exists
- [ ] `SELECT version()` returns PostgreSQL 18
- [ ] `sqlx migrate info` shows 18 applied, 0 pending
- [ ] `.env` exists with two different JWT secrets

---

## Step 2 — The port map

This is the heart of a from-scratch port: **exactly what goes where.** Read it before
writing any code.

Total to port: **133 Rust files, 19,860 lines.** You are copying, not retyping.

### Step 2.1 — By destination

| New location                                                  | From `master`                                       | Files | Lines | Action                               |
| ------------------------------------------------------------- | --------------------------------------------------- | ----- | ----- | ------------------------------------ |
| `crates/db/migrations/`                                       | `packages/db/migrations/`                           | 18    | —     | copy                                 |
| `crates/core/src/pagination.rs`                               | `apps/api/src/shared/pagination.rs`                 | 1     | 287   | copy + edit (drop 1 impl, add tests) |
| `crates/core/src/{error,ids,time}.rs`                         | —                                                   | 3     | new   | write (Step 4)                       |
| `apps/api/src/infrastructure/redis/`                          | `shared/services/redis/`                            | 3     | 647   | copy                                 |
| `apps/api/src/infrastructure/ws/`                             | `shared/services/ws/`                               | 6     | 1,923 | copy                                 |
| `apps/api/src/infrastructure/livekit/`                        | `shared/services/livekit/`                          | 3     | 409   | copy                                 |
| `apps/api/src/infrastructure/push/`                           | `shared/services/push/`                             | 3     | 514   | copy                                 |
| `apps/api/src/infrastructure/storage/`                        | `shared/services/storage.rs`                        | 1     | —     | copy                                 |
| `apps/api/src/infrastructure/oauth/`                          | `shared/integrations/`                              | 2     | —     | copy                                 |
| `apps/api/src/infrastructure/query.rs`                        | `shared/repository.rs`                              | 1     | 40    | copy                                 |
| `apps/api/src/infrastructure/{database,signals,telemetry}.rs` | `src/{database,shared/signals,shared/telemetry}.rs` | 3     | —     | copy                                 |
| `apps/api/src/infrastructure/constants.rs`                    | `shared/constants.rs`                               | 1     | —     | copy                                 |
| `apps/api/src/middleware/`                                    | `shared/middleware/`                                | 6     | 755   | copy                                 |
| `apps/api/src/types/`                                         | `shared/types/`                                     | 3     | —     | copy                                 |
| `apps/api/src/modules/auth/`                                  | `modules/auth/` + `shared/identity.rs`              | 13    | 2,227 | copy + move identity                 |
| `apps/api/src/modules/broadcast/`                             | `modules/broadcast/`                                | 9     | 4,409 | copy (split service in Step 9)       |
| `apps/api/src/modules/chat/`                                  | `modules/chat/`                                     | 9     | 1,045 | copy                                 |
| `apps/api/src/modules/notes/`                                 | `modules/notes/`                                    | 8     | 2,278 | copy                                 |
| `apps/api/src/modules/notifications/`                         | `modules/notifications/`                            | 8     | 1,221 | copy                                 |
| `apps/api/src/modules/profile/`                               | `modules/profile/`                                  | 10    | 943   | copy                                 |
| `apps/api/src/modules/settings/`                              | `modules/settings/`                                 | 8     | 357   | copy                                 |
| `apps/api/src/modules/subscribers/`                           | `modules/subscribers/`                              | 7     | 663   | copy                                 |
| `apps/api/src/routes/`                                        | `routes/`                                           | 11    | 261   | copy                                 |
| `apps/api/src/jobs/`                                          | `jobs/`                                             | 7     | 743   | copy                                 |
| `apps/api/src/{config,state}.rs`                              | `src/{config,state}.rs`                             | 2     | 426   | copy (rewrite in Step 6)             |
| `apps/api/src/{main,lib}.rs`                                  | `src/{main,lib}.rs`                                 | 2     | 46    | copy (rewrite in Step 7)             |
| `apps/api/src/worker.rs`                                      | —                                                   | 1     | new   | write (Step 7)                       |
| `apps/api/src/bootstrap.rs`                                   | —                                                   | 1     | new   | write (Step 7)                       |
| `apps/api/src/infrastructure/traits.rs`                       | —                                                   | 1     | new   | write (Step 8)                       |
| `apps/api/tests/`                                             | `tests/`                                            | 3     | **0** | delete, rewrite (Step 11)            |

### Step 2.2 — By risk, which is how you should actually sequence it

| Risk                                    | Contents                                                                                                                                                         | Rule                                                                       |
| --------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| 🟢 **Copy verbatim, zero edits**        | All 9 domain `modules/` directories. Their internals reference each other, not `shared/`, except for `errors.rs` and pagination.                                 | Copy first. If `cargo check` complains here, you have introduced an error. |
| 🟡 **Copy + mechanical import rewrite** | `middleware/`, `infrastructure/`, `routes/`, `jobs/`, `types/`. The only changes are `crate::shared::*` → new paths.                                             | Copy, then one `sed` pass. No logic edits.                                 |
| 🔴 **Copy + small structural edits**    | `broadcast/service.rs` (split, Step 9), `state.rs` + `config.rs` (rewrite, Step 6), `main.rs`/`lib.rs` (rewrite, Step 7), `errors.rs` ×9 (consolidate, Step 10). | Separate commits, each returning to green.                                 |

### Step 2.3 — Copy the tree in one shot

Do not hand-copy 133 files. Extract from the reference:

```bash
mkdir -p apps/api/src crates/core/src

# Every Rust source file, preserving structure.
git archive master apps/api/src | tar -x
# -> creates apps/api/src/... exactly as it was

# Migrations.
git archive master packages/db/migrations | tar -x -C crates/db/migrations
```

`git archive <ref> <path> | tar -x` is the right tool: it gives you the old tree as a
plain directory with no git metadata, which is exactly the "copy the logic, not the
history" semantics you want. History is preserved on `master` anyway.

Now apply the moves. These are `git mv` because the branch is a git repo and you want
`git log --follow` to work:

```bash
cd apps/api/src

# ── shared/ → three destinations ──────────────────────────────────────────
git mv shared/middleware                middleware
mkdir -p infrastructure
git mv shared/services/redis            infrastructure/redis
git mv shared/services/livekit          infrastructure/livekit
git mv shared/services/push             infrastructure/push
git mv shared/services/ws               infrastructure/ws
git mv shared/services/storage.rs       infrastructure/storage/mod.rs
git mv shared/integrations              infrastructure/oauth
git mv shared/email.rs                  infrastructure/mail/mod.rs
git mv shared/repository.rs             infrastructure/query.rs
git mv shared/constants.rs              infrastructure/constants.rs
git mv shared/database.rs               infrastructure/database.rs   # 2>/dev/null || true
git mv shared/signals.rs                infrastructure/signals.rs
git mv shared/telemetry.rs              infrastructure/telemetry.rs
git mv shared/types                     types

# IdentityReader returns auth::model::User — it belongs next to it, not in shared/.
git mv shared/identity.rs               modules/auth/identity.rs

# pagination goes to the pure crate.
mkdir -p ../../../crates/core/src
git mv shared/pagination.rs             ../../../crates/core/src/pagination.rs

rm shared/mod.rs
ls shared 2>/dev/null && echo "NOT EMPTY — something is still in shared/" || echo "shared/ fully dissolved ✅"
```

> `shared/database.rs` does not exist — `database.rs` sits at `src/` root, not under
> `shared/`. The `2>/dev/null || true` is there because this is exactly the kind of thing
> that will differ if the reference has moved on. **Always confirm your reference commit:**
> `git log -1 --oneline master` and compare against `903c3ba` before porting.

### Step 2.4 — The single import rewrite

Every reference to the old paths becomes the new ones. This is the only mechanical edit
the port requires, and it is one `sed` pass over 133 files:

```bash
cd apps/api

# Dry run first — see the blast radius before changing anything.
grep -rc 'crate::shared::' src/ | grep -v ':0$' | sort -t: -k2 -rn | head -20

# Apply. Order matters: longest/most specific first, so a short pattern cannot
# consume a longer one.
find src -name '*.rs' -print0 | xargs -0 sed -i \
  -e 's/crate::shared::middleware/crate::middleware/g' \
  -e 's/crate::shared::services::redis/crate::infrastructure::redis/g' \
  -e 's/crate::shared::services::livekit/crate::infrastructure::livekit/g' \
  -e 's/crate::shared::services::push/crate::infrastructure::push/g' \
  -e 's/crate::shared::services::ws/crate::infrastructure::ws/g' \
  -e 's/crate::shared::services::storage/crate::infrastructure::storage/g' \
  -e 's/crate::shared::services/crate::infrastructure/g' \
  -e 's/crate::shared::integrations/crate::infrastructure::oauth/g' \
  -e 's/crate::shared::email/crate::infrastructure::mail/g' \
  -e 's/crate::shared::repository/crate::infrastructure::query/g' \
  -e 's/crate::shared::constants/crate::infrastructure::constants/g' \
  -e 's/crate::shared::signals/crate::infrastructure::signals/g' \
  -e 's/crate::shared::telemetry/crate::infrastructure::telemetry/g' \
  -e 's/crate::shared::types/crate::types/g' \
  -e 's/crate::shared::identity/crate::modules::auth::identity/g' \
  -e 's/crate::shared::pagination/crate::meno_core::pagination/g' \
  -e 's/crate::database/crate::infrastructure::database/g' \
  -e 's/crate::signals/crate::infrastructure::signals/g'

# Then prove nothing was missed. This must print nothing.
grep -rn 'shared::' src/ || echo "no stale shared:: references ✅"
```

Note that pagination resolves to `crate::meno_core::pagination`, i.e. `meno_core` is
declared as a **dependency of `apps/api`** in Step 3. The port cannot compile until it is.

**Do not commit yet.** Finish the manifests first — the port commit should be the whole
layout change at once, so review is "did anything move or change that shouldn't have",
which is a much easier question than "did three separate things work".

---

## Step 3 — The manifests and toolchain policy

The port commit needs the workspace wired: three members, one lockfile, one dependency
table, one lint policy. This is scaffolding and cannot change behaviour.

### Step 3.1 — `rust-toolchain.toml`

```toml
[toolchain]
channel = "1.88"
components = ["rustfmt", "clippy", "rust-src"]
profile = "minimal"
```

Pin the channel **and** the components so `cargo fmt`/`clippy` are the same version on
your machine, in CI and in the Docker builder. Without this, local Rust and the
Dockerfile's `rust:1.88` can silently diverge.

### Step 3.2 — `rustfmt.toml` and `clippy.toml`

```toml
# rustfmt.toml
edition = "2024"
max_width = 100
imports_granularity = "Module"
group_imports = "StdExternalCrate"
newline_style = "Unix"
```

```toml
# clippy.toml
msrv = "1.88"
too-many-arguments-threshold = 8
```

`imports_granularity`/`group_imports` are nightly-only and are silently ignored on
stable. That is fine — they activate the moment anyone runs `cargo +nightly fmt`. Do not
set `unstable_features = true` on stable; it errors. `msrv` stops clippy suggesting APIs
newer than your pinned toolchain, which is the difference between a lint gate people
respect and one they disable.

### Step 3.3 — Root `Cargo.toml`

```toml
[workspace]
resolver = "3"
members = ["apps/api", "crates/core", "crates/db"]

[workspace.package]
version = "0.2.0"
edition = "2024"
rust-version = "1.88"
license = "MIT"

# Every third-party crate is declared ONCE here, with features pinned. Members say
# `{ workspace = true }` and never specify a version, so a member cannot silently
# request a different one.
[workspace.dependencies]
# ── internal ──────────────────────────────────────────────────────────────
meno-core = { path = "crates/core" }
meno-db   = { path = "crates/db" }

# ── async runtime & web ───────────────────────────────────────────────────
tokio = { version = "1.52.3", features = ["full"] }
axum = { version = "0.8.9", features = ["ws", "macros", "multipart"] }
tower = { version = "0.5.3", features = ["util", "timeout", "retry"] }
tower-http = { version = "0.7.0", features = [
    "cors", "trace", "timeout", "request-id", "compression-gzip", "limit",
] }
http-body-util = "0.1.3"
bytes = "1.11.1"

# ── database ──────────────────────────────────────────────────────────────
sqlx = { version = "0.8.6", default-features = false, features = [
    "postgres", "runtime-tokio", "tls-rustls", "uuid", "time", "macros", "migrate", "json",
] }

# ── cache & pubsub ────────────────────────────────────────────────────────
fred = { version = "10.1.0", default-features = false, features = [
    "tokio-rustls", "serde-json", "i-scripts", "i-pubsub", "replication",
] }

# ── serialisation ─────────────────────────────────────────────────────────
serde = { version = "1.0.228", features = ["derive"] }
serde_json = "1.0.149"
serde_with = "3.21.0"
validator = { version = "0.20.0", features = ["derive"] }

# ── errors & logging ──────────────────────────────────────────────────────
anyhow = "1.0.102"
thiserror = "2.0.18"
tracing = "0.1.44"
tracing-subscriber = { version = "0.3.23", features = ["env-filter", "json"] }

# ── types ─────────────────────────────────────────────────────────────────
uuid = { version = "1.23.1", features = ["v4", "serde"] }
time = { version = "0.3.47", features = ["serde", "serde-well-known"] }
chrono = "0.4.44"          # Apalis job payloads only — see crates/core/src/time.rs

# ── auth & crypto ─────────────────────────────────────────────────────────
argon2 = "0.5.3"
jsonwebtoken = { version = "10.4.0", features = ["aws_lc_rs"] }
rand = "0.10.1"
sha2 = "0.11.0"
hex = "0.4.3"
base64 = "0.22.1"

# ── jobs ──────────────────────────────────────────────────────────────────
apalis = { version = "1.0.0-rc.9", features = ["limit", "prometheus"] }
apalis-postgres = { version = "1.0.0-rc.8", features = ["time"] }

# ── outbound HTTP & OAuth ─────────────────────────────────────────────────
reqwest = { version = "0.13.3", default-features = false, features = [
    "json", "rustls-tls", "gzip",
] }
oauth2 = { version = "5.0.0", features = ["reqwest"] }

# ── storage ───────────────────────────────────────────────────────────────
object_store = { version = "0.13.2", features = ["aws"] }

# ── realtime ──────────────────────────────────────────────────────────────
livekit-api = "0.5.0"
livekit-protocol = "0.7.7"

# ── config & misc ─────────────────────────────────────────────────────────
config = { version = "0.15", default-features = false }
async-trait = "0.1.89"
dashmap = "6.2.1"
futures-util = "0.3.32"
strum = { version = "0.28", features = ["derive"] }
regex = "1.12.3"
once_cell = "1.21.4"
dotenvy = "0.15.7"
axum-prometheus = "0.10.0"
mockall = "0.13"           # dev-dependency only

[workspace.lints.clippy]
unwrap_used        = "deny"
expect_used        = "deny"
panic              = "deny"
dbg_macro          = "deny"
print_stdout       = "deny"
print_stderr       = "deny"
todo               = "warn"
missing_docs       = "warn"
must_use_candidate = "warn"

[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
strip = true
panic = "abort"
```

Five decisions that are not obvious:

**`resolver = "3"`.** Edition 2024 implies it, and it makes Cargo MSRV-aware: it will no
longer select a dependency requiring a newer Rust than your `rust-version`. With 1.88
pinned, resolver 2 could pick a crate needing 1.90 and fail _only inside the Docker
build_ — a failure you cannot reproduce locally.

**`default-features = false` on `reqwest`, `fred`, `sqlx`.** The old manifest had
`reqwest` with `blocking` + native-tls and `fred` with `tokio-rustls` _and_
`native-tls`. Two TLS stacks in one binary is ~10 MB of dead weight and a class of
certificate bug.

> `reqwest`'s `blocking` feature and `oauth2`'s `reqwest-blocking` are dropped here.
> **Verified against `903c3ba`: no source file references either one** — the Google OAuth
> integration uses the async client throughout. So this is a manifest-only change with
> no source edit. Step 5.2 has the audit command and the full usage list; if your grep
> finds a hit, that is drift from the reference and you must convert that call site first.

**`panic = "abort"` on `[profile.release]` only**, so `cargo test` (the `test` profile) is
unaffected. Runtime image ~15% smaller, and a panic kills the process rather than
unwinding through a half-mutated request.

> Under `panic = "abort"` a panicking Apalis job takes down the _worker process_, not
> just the job. That is intended fail-fast behaviour, and jobs already have `RetryLayer`
> configured — but it is why the worker needs supervisor restarts (Render does this
> natively).

**`missing_docs = "warn"` will produce hundreds of warnings** on 19,860 copied lines.
That is the point — it makes the undocumented surface visible without blocking. Do not
"fix" it by setting `allow`.

**The lint policy will fail the build immediately.** The copied code has `.unwrap()` on a
live request path (`broadcast/service.rs:1055`) and eight `.expect()` calls in
`BroadcastServiceBuilder::build`. Land the policy with one annotated escape hatch, and
remove it in Step 9 when those are genuinely gone:

```toml
# apps/api/Cargo.toml — TEMPORARY, removed at the end of Step 9
[lints.clippy]
unwrap_used = "allow"
expect_used = "allow"
panic       = "allow"
```

> Never use `#![allow]` in source files — invisible to review and never removed. One
> annotated line in one manifest is auditable.

### Step 3.4 — `crates/db` (migrations only)

Already populated by Step 1.11. It needs a manifest and an empty lib target:

```toml
# crates/db/Cargo.toml
[package]
name = "meno-db"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
description = "Meno database schema. Migrations only — no business logic, by design."

[lib]
path = "src/lib.rs"

[lints]
workspace = true
```

```rust
// crates/db/src/lib.rs
//! Meno database schema.
//!
//! This crate intentionally contains **no executable code**. Its only artefact is
//! `migrations/`, which `apps/api` embeds at compile time via
//! `sqlx::migrate!("../../crates/db/migrations")`.
//!
//! # Why this crate exists
//!
//! A migrations-only crate cannot acquire business logic, so the schema stays a single
//! versioned source of truth that the API, the CI migration smoke test, and
//! `sqlx migrate run` all resolve to the same files. On `master` these 18 migrations
//! lived in `packages/db/` — a directory with no manifest that nothing referenced,
//! which is precisely why no migration runner was ever written.
//!
//! # Rules
//!
//! 1. No `sqlx`, no `axum`, no I/O, no types. If you want to add them, the code belongs
//!    in `apps/api`.
//! 2. Migrations are append-only. Never edit a shipped migration; add a new one.
//!    `_sqlx_migrations` records checksums and will refuse to start otherwise.
//! 3. `sqlx::migrate!()` resolves its path relative to the crate calling it.

// Intentionally empty. See above.
```

The comment block is load-bearing. An empty `lib.rs` in a package called `meno-db` looks
like a mistake, and the first person to "tidy up" would delete it. This is the cheapest
possible defence against a well-meaning future contributor.

### Step 3.5 — `crates/core` (pure domain primitives)

```toml
# crates/core/Cargo.toml
[package]
name = "meno-core"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
description = "Pure, dependency-light domain primitives for Meno: cursors, errors, ids, time."

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
time = { workspace = true }
uuid = { workspace = true }
base64 = { workspace = true }
serde_with = { workspace = true }

[lints]
workspace = true
```

Note what is **absent**: no `sqlx`, `axum`, `fred`, `tokio`. That is the whole point —
the domain types cannot see the database even if a future service wants them to, and the
cursor and error logic become testable with no Docker.

```rust
// crates/core/src/lib.rs
//! Pure domain primitives shared across Meno services.
//!
//! **Hard constraint: no I/O.** No `sqlx`, no `axum`, no `fred`, no `tokio`, no `std::fs`,
//! no `std::net`. If a type here needs to touch the outside world it belongs in
//! `apps/api/src/infrastructure/`.
//!
//! The payoff: the cursor system has five distinct wire shapes and is the most
//! intricate logic in the codebase, and on `master` it was untestable without standing
//! up Postgres, Redis and an Axum router. Here it is a pure `#[test]`.

pub mod error;
pub mod ids;
pub mod pagination;
pub mod time;

pub use error::{to_body, Error, ErrorBody, ErrorCode};
pub use pagination::{Cursor, CursorPage, CursorParams, Order};
```

`pagination.rs` was already copied in Step 2.3. Three edits to it:

**(a) Delete the cross-crate import.** The bottom of the copied file has:

```rust
impl From<CursorError> for crate::shared::errors::MenoError { … }
```

`crates/core` cannot name a type from `apps/api` — the dependency runs the other way.
**Delete the impl.** The equivalent conversion comes from `Error` in Step 4, and the six
modules that relied on it (`broadcast`, `chat`, `notes`, `subscribers`, `notifications`,
`profile`) get it transitively. If `cargo check` reports a missing conversion, add it at
the _use_ site — do not reintroduce the cross-crate reference.

**(b) Fix the `use` block.** The copied file imports `serde_with` and `base64`, both
declared above. No change needed unless the reference has drifted.

**(c) Add the tests** — the highest-value tests in the project, because they need no
infrastructure and they pin the most intricate logic:

```rust
// crates/core/src/pagination.rs — append
#[cfg(test)]
mod tests {
    use super::*;

    fn ts(nanos: i128) -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp_nanos(nanos).expect("valid timestamp")
    }

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    // ── shape 1: (timestamp, uuid) ──────────────────────────────────────
    #[test]
    fn timestamp_id_round_trips() {
        let (t, i) = (ts(1_700_000_000_123_456_789), id(7));
        let (t2, i2) = Cursor::from_timestamp_id(t, i)
            .to_timestamp_id()
            .expect("decodes");
        assert_eq!(t.unix_timestamp_nanos(), t2.unix_timestamp_nanos());
        assert_eq!(i, i2);
    }

    // ── shape 2: (ts, ts, uuid) — pinned notes / folders ─────────────────
    #[test]
    fn two_timestamp_id_round_trips() {
        let (a, b, i) = (ts(100), ts(200), id(9));
        let (a2, b2, i2) = Cursor::from_two_timestamps_id(a, b, i)
            .to_two_timestamps_id()
            .expect("decodes");
        assert_eq!(a.unix_timestamp_nanos(), a2.unix_timestamp_nanos());
        assert_eq!(b.unix_timestamp_nanos(), b2.unix_timestamp_nanos());
        assert_eq!(i, i2);
    }

    // ── shape 3: (score, uuid) — count-sorted feeds ──────────────────────
    #[test]
    fn score_id_round_trips_across_the_i64_range() {
        for score in [i64::MIN + 1, -1, 0, 1, i64::MAX] {
            let (s, i) = Cursor::from_score_id(score, id(3))
                .to_score_id()
                .expect("decodes");
            assert_eq!(s, score, "score {score} must survive the round trip");
            assert_eq!(i, id(3));
        }
    }

    // ── shape 4: (name, uuid) ────────────────────────────────────────────
    #[test]
    fn name_id_round_trips_including_awkward_characters() {
        for name in ["simple", "with space", "emoji 🎙️", "quote'and\"dq", "pipe|inside"] {
            let (n, i) = Cursor::from_name_id(name, id(4))
                .to_name_id()
                .expect("decodes");
            assert_eq!(n, name, "name {name:?} must survive the round trip");
            assert_eq!(i, id(4));
        }
    }

    // ── shape 5: (rank, ts, uuid) — search ───────────────────────────────
    #[test]
    fn rank_timestamp_id_round_trips() {
        let (rank, t, i) = (0.75_f32, ts(1_700_000_000_000_000_000), id(5));
        let (r2, t2, i2) = Cursor::from_rank_timestamp_id(rank, t, i)
            .to_rank_timestamp_id()
            .expect("decodes");
        assert!((r2 - rank).abs() < f32::EPSILON, "rank must round-trip");
        assert_eq!(t.unix_timestamp_nanos(), t2.unix_timestamp_nanos());
        assert_eq!(i, i2);
    }

    #[test]
    fn encoding_is_url_safe_and_unpadded() {
        // These go straight into a ?cursor= query parameter, so '+', '/' and '=' would
        // each need escaping, and any client that forgets is silently broken.
        let c = Cursor::from_timestamp_id(ts(1), id(1));
        assert!(!c.0.contains('='), "padding needs escaping in a query string");
        assert!(!c.0.contains('+'), "base64 '+' must not appear in a URL");
        assert!(!c.0.contains('/'), "base64 '/' must not appear in a URL");
    }

    #[test]
    fn client_supplied_garbage_is_rejected_and_never_panics() {
        // A client fully controls this via ?cursor=. Every one of these is reachable,
        // so every one must be an Err — never a panic, because the release profile
        // uses panic = "abort".
        for bad in ["", "!!!", "YWJj", "MTIzfGVrZXM", "999999999999999999999|fake"] {
            let c = Cursor(bad.to_string());
            assert!(c.to_timestamp_id().is_err(), "cursor {bad:?} must not decode");
        }
        // A score cursor fed to the timestamp decoder must fail rather than
        // silently mis-parse, and vice versa.
        assert!(Cursor::from_score_id(5, id(1)).to_timestamp_id().is_err());
    }

    #[test]
    fn from_rows_truncates_the_extra_row_and_reports_more() {
        let rows: Vec<u32> = (0..=10).collect(); // 11 rows
        let page = CursorPage::from_rows(rows, 10, |v| {
            Cursor::from_score_id(i64::from(*v), id(1))
        });
        assert!(page.has_next_page, "11 rows at limit 10 means a next page exists");
        assert_eq!(page.data.len(), 10, "the limit+1 probe row must be dropped");
        assert!(page.next_cursor.is_some());
    }

    #[test]
    fn from_rows_emits_no_cursor_on_the_last_page() {
        let rows: Vec<u32> = (0..10).collect(); // exactly limit
        let page = CursorPage::from_rows(rows, 10, |v| {
            Cursor::from_score_id(i64::from(*v), id(1))
        });
        assert!(!page.has_next_page);
        assert!(page.next_cursor.is_none(), "a final page must not advertise a cursor");
    }

    #[test]
    fn order_emits_only_fixed_sql_fragments() {
        // These strings are interpolated straight into SQL by push_order_and_limit.
        // This test is a SQL-injection guard: it proves a deserialized Order can never
        // carry client text into a query.
        assert_eq!(Order::Desc.sql(), "DESC NULLS LAST");
        assert_eq!(Order::Asc.sql(), "ASC NULLS FIRST");
        assert_eq!(Order::Desc.cursor_op(), "<");
        assert_eq!(Order::Asc.cursor_op(), ">");
    }

    #[test]
    fn limit_is_clamped_to_a_sane_range() {
        let mut p = CursorParams::default();
        assert_eq!(p.limit(), 20, "default page size");
        p.limit = Some(0);
        assert_eq!(p.limit(), 1, "zero must clamp up to 1");
        p.limit = Some(10_000);
        assert_eq!(p.limit(), 100, "absurd limits must clamp to 100");
        p.limit = Some(-5);
        assert_eq!(p.limit(), 1, "negative must clamp to 1");
        assert_eq!(p.limit_plus_one(), 2);
    }
}
```

### Step 3.6 — `apps/api/Cargo.toml`

```toml
[package]
name = "meno-api"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
description = "Meno HTTP API and background worker."

# Two binaries from one crate: web server and job worker. Same image, same
# dependencies, different entry point. See src/main.rs and src/worker.rs.
[[bin]]
name = "meno-api"
path = "src/main.rs"

[[bin]]
name = "meno-worker"
path = "src/worker.rs"

[lib]
name = "meno_api"
path = "src/lib.rs"

[dependencies]
meno-core = { workspace = true }
meno-db = { workspace = true }

axum = { workspace = true }
tokio = { workspace = true }
tower = { workspace = true }
tower-http = { workspace = true }
sqlx = { workspace = true }
fred = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
serde_with = { workspace = true }
validator = { workspace = true }
anyhow = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
uuid = { workspace = true }
time = { workspace = true }
chrono = { workspace = true }
argon2 = { workspace = true }
jsonwebtoken = { workspace = true }
rand = { workspace = true }
sha2 = { workspace = true }
hex = { workspace = true }
base64 = { workspace = true }
apalis = { workspace = true }
apalis-postgres = { workspace = true }
reqwest = { workspace = true }
oauth2 = { workspace = true }
object_store = { workspace = true }
livekit-api = { workspace = true }
livekit-protocol = { workspace = true }
async-trait = { workspace = true }
dashmap = { workspace = true }
futures-util = { workspace = true }
strum = { workspace = true }
regex = { workspace = true }
once_cell = { workspace = true }
dotenvy = { workspace = true }
axum-prometheus = { workspace = true }
http-body-util = { workspace = true }
bytes = { workspace = true }
# lettre is deliberately absent — SMTP is replaced by Brevo (later phase).

[lints.clippy]
unwrap_used = "allow"   # TEMPORARY: removed at the end of Step 9
expect_used = "allow"
panic       = "allow"

[dev-dependencies]
mockall = { workspace = true }
```

> **`meno-worker` will not compile until Step 7** writes `src/worker.rs`. That is fine and
> expected — you will not run `cargo check` until Step 5, by which point it exists.

`meno-db` is a dependency with no code. That is intentional: it makes the direction of
the schema relationship explicit and gives `sqlx::migrate!` a stable crate-relative path.

### Step 3.7 — Rewrite `lib.rs`, drop the empty test tree

```rust
// apps/api/src/lib.rs
pub mod bootstrap;
pub mod config;
pub mod infrastructure;
pub mod jobs;
pub mod middleware;
pub mod modules;
pub mod routes;
pub mod state;
pub mod types;
```

The copied `lib.rs` declares `shared`, which no longer exists. Delete the three stub test
files too — `tests/mod.rs`, `tests/auth/mod.rs` and `tests/auth/repository_test.rs` are
**all 0 bytes**, and the layout is wrong anyway (Cargo auto-discovers only top-level
`tests/*.rs`). Step 11 rebuilds them properly.

### Step 3.8 — Verify the manifests parse before touching code

```bash
cargo metadata --format-version 1 --no-deps >/dev/null && echo "manifests resolve ✅"
```

---

## Step 4 — `crates/core::Error` and the small modules

The port copied `pagination.rs`. Now add the canonical error type and the two tiny
helpers.

### Step 4.1 — Why the error enum differs from the plan's sketch

§4.2 sketches an `Error` holding `sqlx::Error` and `fred::error::Error`. §2.1 makes
`crates/core` dependency-free, and §5.5 says domain code must stop seeing driver types.
**The plan contradicts itself here.** The resolution: infrastructure errors are erased to
a string at the repository boundary.

> `db_err(context, source)` takes a `sqlx::Error`, so it cannot live in `core`. Its real
> home is `apps/api/src/infrastructure/database.rs`:
>
> ```rust
> /// Erase a driver error at the infrastructure boundary.
> ///
> /// The *only* place a `sqlx::Error` becomes an `Error`. `context` is what makes the
> /// log line diagnosable — "database error" alone is not.
> pub fn db_err(context: &'static str, source: &sqlx::Error) -> Error {
>     tracing::error!(context, error = %source, "database error");
>     Error::Internal { context, detail: source.to_string() }
> }
> ```
>
> If you want `impl From<sqlx::Error> for Error`, that is the §5.5 leak re-entering
> through the back door.

```rust
// crates/core/src/error.rs
use std::collections::HashMap;

/// Stable, machine-readable error codes.
///
/// # Stability contract
///
/// Every variant is a wire value. Flutter and Next.js branch on these strings. The
/// human-readable `message` may be reworded at any time; a `code` may not be renamed or
/// removed without an API version bump. Adding codes is the intended extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorCode {
    // 400
    BadRequest, InvalidCursor, InvalidTimeZone, StartTimeInPast,
    // 401
    Unauthorized, InvalidCredentials, InvalidToken, TokenExpired, RefreshTokenExpired,
    // 403
    Forbidden, NotCreator, NotOwner, NotParticipant,
    // 404
    NotFound,
    // 409
    Conflict, EmailTaken, AlreadyExists, VersionConflict,
    // 422
    ValidationFailed,
    // 429
    RateLimited,
    // 503
    UpstreamUnavailable,
    // 500
    Internal,
}

impl ErrorCode {
    /// The exact string sent on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BadRequest => "BAD_REQUEST",
            Self::InvalidCursor => "INVALID_CURSOR",
            Self::InvalidTimeZone => "INVALID_TIME_ZONE",
            Self::StartTimeInPast => "START_TIME_IN_PAST",
            Self::Unauthorized => "UNAUTHORIZED",
            Self::InvalidCredentials => "INVALID_CREDENTIALS",
            Self::InvalidToken => "INVALID_TOKEN",
            Self::TokenExpired => "TOKEN_EXPIRED",
            Self::RefreshTokenExpired => "REFRESH_TOKEN_EXPIRED",
            Self::Forbidden => "FORBIDDEN",
            Self::NotCreator => "NOT_CREATOR",
            Self::NotOwner => "NOT_OWNER",
            Self::NotParticipant => "NOT_PARTICIPANT",
            Self::NotFound => "NOT_FOUND",
            Self::Conflict => "CONFLICT",
            Self::EmailTaken => "EMAIL_TAKEN",
            Self::AlreadyExists => "ALREADY_EXISTS",
            Self::VersionConflict => "VERSION_CONFLICT",
            Self::ValidationFailed => "VALIDATION_FAILED",
            Self::RateLimited => "RATE_LIMITED",
            Self::UpstreamUnavailable => "UPSTREAM_UNAVAILABLE",
            Self::Internal => "INTERNAL_ERROR",
        }
    }
}

/// The single error type for the whole API.
///
/// Infrastructure failures are erased to a string at the repository boundary and logged
/// there with the driver error. Nothing driver-shaped crosses into domain code — that is
/// the §5.5 fix, and it is also what lets this type live with no `sqlx` and no `fred`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{message}")]
    BadRequest { code: ErrorCode, message: String },
    #[error("{message}")]
    Unauthorized { code: ErrorCode, message: String },
    #[error("{message}")]
    Forbidden { code: ErrorCode, message: String },
    /// `resource` is a static string like `"broadcast"` — never user input.
    #[error("{resource} not found")]
    NotFound { resource: &'static str, code: ErrorCode },
    #[error("{message}")]
    Conflict { code: ErrorCode, message: String },
    #[error("validation failed")]
    Validation { fields: HashMap<String, Vec<String>> },
    #[error("rate limited")]
    RateLimited { retry_after_secs: u64 },
    /// An upstream (LiveKit, Brevo, FCM, R2) failed or is disabled.
    #[error("{service} unavailable")]
    Upstream { service: &'static str, detail: String },
    /// Always logged at `error` with full context; the client gets a generic message.
    #[error("{context}: {detail}")]
    Internal { context: &'static str, detail: String },
}

impl Error {
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::BadRequest { code, .. }
            | Self::Unauthorized { code, .. }
            | Self::Forbidden { code, .. }
            | Self::NotFound { code, .. }
            | Self::Conflict { code, .. } => *code,
            Self::Validation { .. } => ErrorCode::ValidationFailed,
            Self::RateLimited { .. } => ErrorCode::RateLimited,
            Self::Upstream { .. } => ErrorCode::UpstreamUnavailable,
            Self::Internal { .. } => ErrorCode::Internal,
        }
    }

    /// True when the message is the caller's to see. Everything else must be logged and
    /// replaced with a generic message.
    #[must_use]
    pub const fn is_client_safe(&self) -> bool {
        matches!(
            self,
            Self::BadRequest { .. }
                | Self::Unauthorized { .. }
                | Self::Forbidden { .. }
                | Self::NotFound { .. }
                | Self::Conflict { .. }
                | Self::Validation { .. }
                | Self::RateLimited { .. }
        )
    }
}

/// The status/code/message triple sent to the client. Pure data — no `axum`, no
/// `http::StatusCode`. `apps/api` renders it.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorBody {
    pub status_code: u16,
    pub code: &'static str,
    pub message: String,
    pub status: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<HashMap<String, Vec<String>>>,
}

/// Map an `Error` to its HTTP status, wire code and client-safe body.
///
/// The single place this decision is made. On `master` it is duplicated across nine
/// `IntoResponse` impls and has drifted: `MenoError::NotFound` forwards the caller's
/// message while `Database` correctly does not, with no rule behind the difference.
#[must_use]
pub fn to_body(err: &Error) -> ErrorBody {
    let (status_code, message, data) = match err {
        Error::BadRequest { message, .. } => (400, message.clone(), None),
        Error::Unauthorized { message, .. } => (401, message.clone(), None),
        Error::Forbidden { message, .. } => (403, message.clone(), None),
        Error::NotFound { resource, .. } => (404, format!("{resource} not found"), None),
        Error::Conflict { message, .. } => (409, message.clone(), None),
        Error::Validation { fields } => (
            422,
            "One or more fields are invalid".to_string(),
            Some(fields.clone()),
        ),
        Error::RateLimited { retry_after_secs } => {
            (429, format!("Too many requests. Retry in {retry_after_secs}s"), None)
        }
        // 503: the caller may retry, so naming the dependency is useful — but the
        // upstream's own error text must not appear.
        Error::Upstream { service, .. } => {
            (503, format!("{service} is temporarily unavailable"), None)
        }
        // 500: generic. The detail is in the logs, correlated by request id.
        Error::Internal { .. } => (500, "An internal error occurred".to_string(), None),
    };

    ErrorBody {
        status_code,
        code: err.code().as_str(),
        message,
        status: false,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infrastructure_errors_never_leak_detail_to_the_client() {
        // The security property. A SQL error string contains table names, column names
        // and sometimes row values. None of it may reach the client.
        let err = Error::Internal {
            context: "load_broadcast",
            detail: "error returned from database: relation \"users_secret\" does not exist"
                .to_string(),
        };
        let body = to_body(&err);
        assert_eq!(body.status_code, 500);
        assert_eq!(body.code, "INTERNAL_ERROR");
        assert_eq!(body.message, "An internal error occurred");
        assert!(
            !body.message.contains("users_secret"),
            "schema names must not reach the client"
        );
        assert!(!err.is_client_safe());
    }

    #[test]
    fn upstream_detail_is_summarised_not_forwarded() {
        let err = Error::Upstream {
            service: "livekit",
            detail: "401 from https://livekit.internal:7880/admin".to_string(),
        };
        let body = to_body(&err);
        assert_eq!(body.status_code, 503);
        assert_eq!(body.code, "UPSTREAM_UNAVAILABLE");
        assert!(
            !body.message.contains("livekit.internal"),
            "upstream URLs must not leak"
        );
    }

    #[test]
    fn client_errors_keep_their_specific_code() {
        // The whole point of ErrorCode: clients branch on this, never on message text.
        let cases = [
            (
                Error::NotFound { resource: "broadcast", code: ErrorCode::NotFound },
                404,
                "NOT_FOUND",
            ),
            (
                Error::Conflict { code: ErrorCode::EmailTaken, message: "taken".into() },
                409,
                "EMAIL_TAKEN",
            ),
            (
                Error::BadRequest {
                    code: ErrorCode::StartTimeInPast,
                    message: "start_time must be in the future".into(),
                },
                400,
                "START_TIME_IN_PAST",
            ),
            (Error::RateLimited { retry_after_secs: 30 }, 429, "RATE_LIMITED"),
        ];
        for (err, status, code) in cases {
            let body = to_body(&err);
            assert_eq!(body.status_code, status);
            assert_eq!(body.code, code);
            assert!(!body.status, "every error body sets status:false");
        }
    }

    #[test]
    fn validation_details_are_structured_not_a_flat_string() {
        let mut fields = HashMap::new();
        fields.insert("email".to_string(), vec!["invalid format".to_string()]);
        let body = to_body(&Error::Validation { fields });
        assert_eq!(body.status_code, 422);
        let data = body.data.expect("validation carries per-field detail");
        assert_eq!(data["email"], vec!["invalid format".to_string()]);
    }

    #[test]
    fn every_code_maps_to_a_distinct_wire_string() {
        // Guards against copy-paste when adding codes — a duplicated string would
        // silently merge two error meanings in the client.
        let all = [
            ErrorCode::BadRequest, ErrorCode::InvalidCursor, ErrorCode::InvalidTimeZone,
            ErrorCode::StartTimeInPast, ErrorCode::Unauthorized,
            ErrorCode::InvalidCredentials, ErrorCode::InvalidToken, ErrorCode::TokenExpired,
            ErrorCode::RefreshTokenExpired, ErrorCode::Forbidden, ErrorCode::NotCreator,
            ErrorCode::NotOwner, ErrorCode::NotParticipant, ErrorCode::NotFound,
            ErrorCode::Conflict, ErrorCode::EmailTaken, ErrorCode::AlreadyExists,
            ErrorCode::VersionConflict, ErrorCode::ValidationFailed, ErrorCode::RateLimited,
            ErrorCode::UpstreamUnavailable, ErrorCode::Internal,
        ];
        let mut seen = std::collections::HashSet::new();
        for code in all {
            assert!(seen.insert(code.as_str()), "duplicate wire string for {code:?}");
        }
        assert_eq!(seen.len(), all.len());
    }
}
```

`#[non_exhaustive]` on `ErrorCode` is load-bearing: it forces a wildcard arm in every
downstream `match`, so a variant added later is a compile error in your client generator
rather than a silently unhandled case in production.

### Step 4.2 — `ids.rs` and `time.rs`

The project carries **both** `time::OffsetDateTime` and `chrono::DateTime<Utc>` — two
time types in application logic is a reliable source of conversion bugs at the sqlx
boundary. Centralising the choice is the fix.

```rust
// crates/core/src/ids.rs
//! Canonical id generation.
//!
//! Every id in Meno is a v4 UUID. Centralised so the one place that would ever need to
//! change — to v7, for index locality — is findable by grep.

use uuid::Uuid;

/// Generate a new random v4 identifier.
#[must_use]
pub fn new_id() -> Uuid {
    Uuid::new_v4()
}
```

```rust
// crates/core/src/time.rs
//! Canonical time type.
//!
//! Meno uses `time::OffsetDateTime` everywhere. `chrono` is confined to the Apalis job
//! payloads, which require it; converting at that one boundary is cheap, but letting it
//! leak into services is not.

use time::OffsetDateTime;

/// The current instant, in UTC.
#[must_use]
pub fn now() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}

/// True when `ts` is in the past. Used by broadcast scheduling validation.
#[must_use]
pub fn is_past(ts: OffsetDateTime) -> bool {
    ts < now()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_is_utc() {
        assert_eq!(now().offset(), time::UtcOffset::UTC);
    }

    #[test]
    fn is_past_distinguishes_yesterday_from_tomorrow() {
        assert!(is_past(now() - time::Duration::days(1)));
        assert!(!is_past(now() + time::Duration::days(1)));
    }
}
```

**Verify — and this is your first real green light:**

```bash
cargo test -p meno-core
```

Every test should pass **with no database and no Redis running**. If it does not,
something leaked a dependency into the crate. That is the property you are buying.

---

## Step 5 — Make the port compile

This is the step where the port actually lands. Everything so far has been structure;
this is where 133 copied files either work or tell you what's wrong.

**Do not run `cargo check` before `worker.rs` exists** (Step 7), because
`[[bin]] meno-worker` points at a file that does not exist and the error will bury the
real ones. Write the minimal `worker.rs` stub first if you want to check early:

```bash
cat > apps/api/src/worker.rs <<'EOF'
// placeholder — replaced in Step 7
fn main() {}
EOF
```

### Step 5.1 — First compile, expect noise

```bash
cargo check --workspace --all-targets 2>&1 | tail -60
```

You should see a small, mechanical set of errors: missing module declarations for the
new `infrastructure/`, `middleware/`, `types/` namespaces, and any path the `sed` in
Step 2.4 missed. Fix those with the new `infrastructure/mod.rs` etc:

```rust
// apps/api/src/infrastructure/mod.rs
//! Everything that talks to the outside world.
//!
//! `shared/` on master asserted nothing about where its contents belonged, so business
//! rules, Axum layers, Redis clients and the response envelope all sat behind one
//! namespace. This module is the actual boundary: Redis, LiveKit, FCM, WebSockets,
//! object storage, OAuth, email, the database pool, telemetry and signals.

pub mod constants;
pub mod database;
pub mod mail;
pub mod oauth;
pub mod push;
pub mod query;
pub mod redis;
pub mod signals;
pub mod storage;
pub mod telemetry;
pub mod traits;      // Step 8
pub mod ws;
```

```rust
// apps/api/src/middleware/mod.rs
//! Axum request layers. Nothing here is domain logic.

pub mod auth;
pub mod extractors;
pub mod idempotency;
pub mod rate_limit;
pub mod timing;
```

```rust
// apps/api/src/types/mod.rs
//! Wire types shared by every endpoint.

pub mod dto;
pub mod meno_response;
```

Re-run `cargo check`. Iterate until the only errors left are the ones Step 7 onward
address. Each round should be smaller than the last.

### Step 5.2 — Drop the unused `blocking` features

The workspace table omits `reqwest`'s `blocking` feature and `oauth2`'s
`reqwest-blocking`, which `master`'s manifest enables. **Verified against `903c3ba`: no
source file references `reqwest::blocking` at all** — the Google OAuth integration uses
the async client throughout:

```bash
# Must print nothing before you drop the features.
grep -rn 'reqwest::blocking\|oauth2::blocking' apps/api/src/ && echo "BLOCKING IN USE — do not drop the feature"
```

All five `reqwest` uses in the tree are async:

```
shared/integrations/google.rs:77   oauth2::reqwest::ClientBuilder::new()
shared/integrations/google.rs:89   reqwest::Client::new()      (userinfo fetch)
shared/integrations/google.rs:101  reqwest::Client::new()      (userinfo fetch)
shared/services/push/mod.rs:31     use reqwest::Client;        (FCM v1 POST)
shared/services/push/error.rs:19   Http(#[from] reqwest::Error)
```

So dropping the features is a **manifest-only change with no source edit** — a pure
win, since two TLS stacks and a blocking pool were compiled into the binary for nothing.
If your grep finds a hit, that is drift from the reference and you must convert the call
site to async first.

### Step 5.3 — Note the copied `database.rs` panic path

The copied [database.rs] does `.expect("Failed to connect to PostgreSQL DB")`. Leave it
for now if you like — it is not a compile error — but note it: Step 6 removes it, and
the lint escape hatch in Step 3.6 exists partly for it.

### Step 5.4 — Generate `.sqlx` and clear the 43 errors

This is the moment the port becomes buildable without a database. The 43 errors on
`master` were ~40 stale-cache errors plus 3 `E0063` (missing `total_participants`).
You never reproduced them, because you never built the old tree — and you do not
reproduce them now, because you generate the cache against the migrated database from
Step 1:

```bash
unset SQLX_OFFLINE   # essential: sqlx must talk to the real DB to prepare
cargo sqlx prepare --workspace -- --all-targets
```

This writes a fresh `.sqlx/` reflecting the real schema — **including
`total_participants`**, because migration `0005` and the `0020` semantics are in the
database. The 3 `E0063` errors therefore disappear without editing
`broadcast/repository.rs:299/345/363` by hand.

```bash
cargo check --workspace --all-targets     # should now be clean
```

### Step 5.5 — Commit the port

This should be **one boring commit**. If reviewing it feels like reviewing logic, you
edited during the port and should split the edit out.

```bash
git add -A
git commit -m "refactor: port API into the monorepo layout

Moves the existing implementation into the §2 target tree: apps/api with
infrastructure/, middleware/ and types/ replacing shared/; crates/db holding the
18 migrations; crates/core holding Cursor/CursorPage/Order plus the canonical
Error taxonomy.

This is a port, not a rewrite. Domain logic is copied unchanged from 903c3ba.
The only edits are mechanical: one sed pass rewriting crate::shared::* to the new
paths, the cross-crate From<CursorError> impl dropped from pagination.rs because
crates/core cannot name an apps/api type, and the unused reqwest/oauth2 blocking
features dropped from the manifest. No source file referenced them.

.sqlx is regenerated from the migrated database rather than carried over, which
clears the 43 compile errors on master (40 stale cache entries, 3 missing
total_participants) without hand-editing repository.rs.
```

> **This commit is your safe checkpoint.** Everything after it is optional improvement.
> If the branch becomes unworkable, you can stop here and still have a working API in
> the new layout.

---

# Phase 2 — Structure

From here each step is a separate commit that returns to green. The port checkpoint
means any regression bisects to exactly one of these.

## Step 6 — Typed, layered config

**Why.** The copied `Config::from_env` requires ~15 variables that compose never set,
hardcodes `origins` to `yourdomain.com`, and `read_to_string`s
`FIREBASE_SERVICE_ACCOUNT_PATH` from disk — so a missing FCM file blocks local boot even
though push is optional. It also reports only the _first_ missing variable, which makes
the "why won't it start" loop slow.

This is in scope because without it the port cannot boot, which is Step 6.4's gate.

### Step 6.1 — Required vs optional

```rust
//! Typed configuration, validated once at the boundary.
//!
//! Only the six genuinely-required settings fail startup. Every optional integration
//! is `Option`, so a deployment without FCM, LiveKit or object storage still boots —
//! on master, a missing FIREBASE_SERVICE_ACCOUNT_PATH killed the process.

use std::net::IpAddr;

/// A string that is never printed. `Debug` is redacted so a config dump cannot leak a
/// JWT secret into the logs.
#[derive(Clone, PartialEq)]
pub struct Secret(String);

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

impl Secret {
    /// Deliberately not `Debug`, so the only way to use it is explicit.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}
```

That `Secret` wrapper is a §9.5 rule made concrete. On `master`, `Config` derives `Clone`
and holds ~10 secrets as plain `String`, so any `#[derive(Debug)]` on a struct containing
it — or any accidental `?config` — prints live credentials.

### Step 6.2 — One error listing every problem

```rust
#[derive(Debug, thiserror::Error)]
#[error("invalid configuration:\n{}", .problems.iter().map(|p| format!("  - {p}")).collect::<Vec<_>>().join("\n"))]
pub struct ConfigError {
    pub problems: Vec<String>,
}

impl ConfigError {
    fn push(&mut self, var: &str, why: &str) {
        self.problems.push(format!("{var}: {why}"));
    }
}

/// Load and validate configuration.
///
/// Collects **every** problem rather than returning on the first, so one run tells you
/// everything you need to fix.
pub fn load() -> Result<Config, ConfigError> {
    dotenvy::dotenv().ok();
    let mut err = ConfigError { problems: Vec::new() };

    // ── required: the app cannot run without these ───────────────────────
    let database_url = required(&mut err, "DATABASE_URL");
    let redis_url = required(&mut err, "REDIS_URL");
    let jwt_secret = required_secret(&mut err, "JWT_SECRET");
    let jwt_refresh_secret = required_secret(&mut err, "JWT_REFRESH_SECRET");
    // CORS_ORIGINS is required because defaulting it silently to "*" is how a staging
    // environment ends up serving production traffic with wide-open CORS.
    let cors_origins = required(&mut err, "CORS_ORIGINS");

    // ── optional integrations ────────────────────────────────────────────
    let livekit = LiveKitSettings::optional(&mut err);
    let storage = StorageSettings::optional(&mut err);
    let push = PushSettings::optional(&mut err);

    if !err.problems.is_empty() {
        return Err(err);
    }
    Ok(Config { /* … */ })
}
```

```rust
fn required(err: &mut ConfigError, var: &str) -> String {
    match std::env::var(var) {
        Ok(v) if !v.trim().is_empty() => v,
        _ => {
            err.push(var, "required but not set (see .env.example)");
            String::new()
        }
    }
}
```

The `PushSettings::optional` shape is what makes `PUSH_ENABLED=false` real:

```rust
pub struct PushSettings {
    /// Raw JSON of the service account, or `None` when push is disabled. Note this is
    /// the *contents*, not a path — reading a file from disk at startup is what made
    /// local boot depend on a secret existing.
    pub service_account_json: Option<String>,
}

impl PushSettings {
    fn optional(err: &mut ConfigError) -> Self {
        let enabled = std::env::var("PUSH_ENABLED").unwrap_or_else(|_| "false".into()) == "true";
        if !enabled {
            return Self { service_account_json: None };
        }
        match std::env::var("FIREBASE_SERVICE_ACCOUNT_JSON") {
            Ok(v) => Self { service_account_json: Some(v) },
            Err(_) => {
                // Only an error *because* push was explicitly enabled.
                err.push("FIREBASE_SERVICE_ACCOUNT_JSON", "required when PUSH_ENABLED=true");
                Self { service_account_json: None }
            }
        }
    }
}
```

### Step 6.3 — No-op adapters, so disabled means disabled

`NoopPush` from Step 8 is what actually implements "disabled". Wire it in `state.rs`
based on the `Option`, rather than constructing a client and hoping it is never called:

```rust
fn build_push(config: &Config) -> Result<Arc<dyn PushSender>, Error> {
    let push: Arc<dyn PushSender> = match &config.push.service_account_json {
        Some(json) => Arc::new(FirebasePush::new(json)?),
        None => Arc::new(NoopPush),
    };
    Ok(push)
}
```

### Step 6.4 — Rewrite `.env.example` from the struct

Generate it from the config definition rather than hand-maintaining it. On `master`,
`.env.example` declares `FIREBASE_SERVICE_ACCOUNT_URL`, `AWS_REGION`, `EMAIL_URL` and
`CLOUDINARY_URL` — **none of which the code reads**. It is a fiction.

```bash
# Generated by scripts/gen-env-example.sh — do not edit by hand.
# ── REQUIRED ──────────────────────────────────────────────────────────────
DATABASE_URL=postgres://meno:password@localhost:5432/meno_dev
REDIS_URL=redis://localhost:6379
JWT_SECRET=                 # openssl rand -hex 64
JWT_REFRESH_SECRET=         # openssl rand -hex 64  (must differ from JWT_SECRET)
CORS_ORIGINS=http://localhost:3000

# ── optional integrations (default: disabled) ────────────────────────────
ENV=dev
PORT=8080
ACCESS_TOKEN_EXPIRATION=900
REFRESH_TOKEN_EXPIRATION=604800

LIVEKIT_API_KEY=
LIVEKIT_API_SECRET=
LIVEKIT_HOST=
LIVEKIT_ENABLED=false

STORAGE_ENDPOINT=http://localhost:9000
STORAGE_ACCESS_KEY=rustfsadmin
STORAGE_SECRET_KEY=rustfsadmin
STORAGE_BUCKET=meno-uploads
STORAGE_REGION=auto
STORAGE_PUBLIC_URL=http://localhost:9000/meno-uploads

PUSH_ENABLED=false
FIREBASE_PROJECT_ID=
FIREBASE_SERVICE_ACCOUNT_JSON=   # the JSON contents, not a path

GOOGLE_CLIENT_ID=
GOOGLE_CLIENT_SECRET=
GOOGLE_REDIRECT_URI=
GOOGLE_AUTH_URI=
GOOGLE_TOKEN_URI=

SMTP_HOST=
SMTP_PORT=465
SMTP_USER=
SMTP_PASSWORD=
SMTP_FROM=

SKIP_MIGRATIONS=false
```

### Step 6.5 — Make the pool configuration Neon-safe

The copied `create_postgres_pool` uses `max_connections(20)` and
`max_lifetime(30 min)`. Neon runs PgBouncer in **transaction** mode, where a
transaction-mode pooler pins a server connection for the duration of a transaction —
so a long-lived pooled connection can be killed mid-transaction. §3.5 calls for a
smaller, shorter-lived pool:

```rust
pub async fn create_postgres_pool(url: &str) -> Result<PgPool, sqlx::Error> {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(8)          // was 20 — PgBouncer has its own limit
        .min_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .idle_timeout(Duration::from_secs(300))
        .max_lifetime(Duration::from_secs(300))   // was 30 min
        .test_before_acquire(true)
        .connect(url)
        .await
}
```

Note the signature change to `Result` — no `.expect`. That is one of the three startup
panics removed here.

**Verify / commit:**

```bash
cargo check --workspace --all-targets
# start with a deliberately incomplete env — the error must list everything at once
env -i DATABASE_URL=x cargo run --bin meno-api 2>&1 | head -20
```

```
refactor(config): typed layered config with aggregated validation errors

Config::from_env returned on the first missing variable, so fixing startup took
N runs for N variables. It also hard-fails on FIREBASE_SERVICE_ACCOUNT_PATH
and read_to_string's it from disk, so a missing secret blocked boot even though
push is optional, and hardcoded CORS origins to yourdomain.com.

Six settings are required; every integration is Option behind a flag. Secrets are
wrapped in a redacting Secret type so no config dump can leak them. The error
lists every problem in one run. Also reduces pool max_connections 20 -> 8 and
max_lifetime 30min -> 300s for Neon's transaction-mode PgBouncer.
```

## Step 7 — Split the binaries: `bootstrap.rs`, `main.rs`, `worker.rs`

**Why.** The copied `state.rs::build_meno_router` calls `start_background_workers`
unconditionally, so every replica of the Render web service spawns an Apalis monitor.
Two replicas means every job runs twice: duplicate emails, duplicate fan-out, duplicate
LiveKit room teardown. `AppRole` makes that structurally impossible rather than a rule
people must remember.

> **Scope boundary.** This step moves the monitor and the two interval loops to the
> worker binary. It does **not** fix the unwired `BroadcastScheduledFanOutJob` or convert
> the intervals to Apalis `ScheduledAt` — those are behavioural and stay carried over.

### Step 7.1 — `AppRole` and `AppContext`

```rust
// apps/api/src/bootstrap.rs
//! Shared startup for both binaries.
//!
//! `main.rs` (web) and `worker.rs` (jobs) build their world through the same code, so
//! config, telemetry, the pool and Redis are wired identically. The only difference is
//! which half of the world each role is allowed to run.

use sqlx::PgPool;
use std::sync::Arc;

/// Which half of the application this process is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppRole {
    /// Serves HTTP. **Must not** start the Apalis monitor.
    Web,
    /// Runs the Apalis monitor. Does not bind a port.
    Worker,
}

impl AppRole {
    /// Whether this role may run background jobs.
    ///
    /// Called from `run_jobs` rather than checked by convention, so re-enabling jobs in
    /// the web binary is a compile error rather than a production incident.
    #[must_use]
    pub const fn runs_jobs(self) -> bool {
        matches!(self, Self::Worker)
    }
}

/// Everything both binaries need, built once at startup.
pub struct AppContext {
    pub config: Arc<Config>,
    pub db: PgPool,
    pub redis: Redis,
    pub state: Arc<MenoState>,
    pub role: AppRole,
}
```

### Step 7.2 — The shared builder, with no panics

```rust
/// Build config, telemetry, the pool and Redis. Called by both binaries.
///
/// Never panics: every failure returns `Err` with context so the process exits non-zero
/// with a readable message. This removes three startup panics inherited from master —
/// `.expect("Failed to connect to PostgreSQL DB")` in database.rs, the
/// `.expect("WsPubSubBridge failed to initialise")` at state.rs:125, and the SMTP
/// `.unwrap()` at state.rs:146.
pub async fn build_context(role: AppRole) -> anyhow::Result<AppContext> {
    install_crypto_provider();
    let config = Arc::new(Config::load().map_err(|e| anyhow::anyhow!("{e}"))?);
    init_telemetry(&config);

    let db = infrastructure::database::create_postgres_pool(config.database_url.expose())
        .await
        .map_err(|e| anyhow::anyhow!("failed to connect to PostgreSQL: {e}"))?;

    // Migrations run before anything queries the schema. On failure we log and exit —
    // never serve traffic against a schema we do not understand.
    if !config.skip_migrations {
        // Path is relative to the crate containing this macro call, i.e. apps/api.
        sqlx::migrate!("../../crates/db/migrations")
            .run(&db)
            .await
            .map_err(|e| anyhow::anyhow!("database migration failed: {e}"))?;
        tracing::info!("migrations applied");
    }

    // Apalis' own tables. Lives here, not in run_monitor, because the web binary now
    // *pushes* jobs and needs those tables to exist.
    apalis_postgres::PostgresStorage::setup(&db)
        .await
        .map_err(|e| anyhow::anyhow!("apalis setup failed: {e}"))?;

    let redis = Redis::new(RedisConfig::from_url(config.redis_url.expose()))
        .await
        .map_err(|e| anyhow::anyhow!("failed to connect to Redis: {e}"))?;

    let state = Arc::new(build_state(&config, db.clone(), redis.clone()).await?);

    Ok(AppContext { config, db, redis, state, role })
}

/// The one place a job monitor may be started.
///
/// `run_jobs` is the only caller and it passes `ctx.role`, so a web binary physically
/// cannot reach this with a role that runs jobs.
pub async fn run_jobs(ctx: AppContext) -> anyhow::Result<()> {
    if !ctx.role.runs_jobs() {
        return Ok(());
    }
    jobs::monitor::run_monitor(ctx.db, ctx.state).await
}
```

`SKIP_MIGRATIONS` defaults to `false` everywhere, including production. It exists so a
worker restart during a rolling deploy cannot race the web service on the migration lock
— set it on one of the two, deliberately.

### Step 7.3 — `main.rs` becomes thin

```rust
// apps/api/src/main.rs
use meno_api::{
    bootstrap::{self, AppRole},
    infrastructure::signals::shutdown_signal,
};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let ctx = bootstrap::build_context(AppRole::Web).await?;

    let router = meno_api::build_router(ctx.state.clone());
    let listener = TcpListener::bind(format!("0.0.0.0:{}", ctx.config.port)).await?;

    tracing::info!(port = ctx.config.port, env = %ctx.config.env, "Meno API listening");

    // Deliberately absent: bootstrap::run_jobs(). A web replica running jobs means
    // every job executes once per replica.
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("API shut down cleanly");
    Ok(())
}
```

### Step 7.4 — `worker.rs`

```rust
// apps/api/src/worker.rs
//! The Meno background worker.
//!
//! Deployed as its own Render Background Worker from the same image as the API; only the
//! start command differs. Running jobs in the web process would mean every
//! horizontally-scaled replica executes every job.

use meno_api::bootstrap::{self, AppRole};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let ctx = bootstrap::build_context(AppRole::Worker).await?;

    tracing::info!(env = %ctx.config.env, "Meno worker starting");

    // Blocks until SIGTERM, draining in-flight jobs. Runs the monitor plus the two
    // interval loops that were previously spawned inside the web process.
    bootstrap::run_jobs(ctx).await?;

    tracing::info!("Worker shut down cleanly");
    Ok(())
}
```

### Step 7.5 — Remove the in-process spawn

This is the actual behaviour change:

```rust
// apps/api/src/state.rs — DELETE
fn start_background_workers(db: &PgPool, app: &Arc<MenoState>) {
    tokio::spawn(async move { /* run_monitor */ });
    tokio::spawn(crate::jobs::monitor::schedule_cleanup_job(pool_1));
    tokio::spawn(crate::jobs::monitor::schedule_notes_cleanup_job(pool_2));
}
```

Also delete its call inside `build_meno_router`. The two `tokio::time::interval` loops
move into `run_jobs` — they are already worker concerns, and leaving them behind after
removing the monitor would silently stop token cleanup and note purging in production.

### Step 7.6 — Deduplicate `shutdown_signal`

`master` has two copies: `shared/signals.rs` and a second inside `jobs/monitor.rs` with
30-second job draining. Do not silently keep either — compare and keep both behaviours:

```rust
// infrastructure/signals.rs
/// SIGTERM + Ctrl-C. Used by the web binary, which can drop connections immediately.
pub async fn shutdown_signal() { /* select! on both */ }

/// Same, but gives in-flight jobs up to `grace` seconds to finish. Worker-only: on
/// Render a deploy sends SIGTERM then SIGKILL after ~30s, and draining first means far
/// fewer retries and no half-sent emails.
pub async fn shutdown_signal_graceful(grace: Duration) { /* select! with timeout */ }
```

### Step 7.7 — Remove the other two startup panics

`state.rs` also has `.expect("WsPubSubBridge failed to initialise")` and an SMTP
`.unwrap()`. Both become returned errors:

```rust
async fn build_ws_pubsub_bridge(
    config: &Config,
    ws: WsService,
    redis: Redis,
) -> Result<WsPubSubBridge, BridgeError> {
    let bridge = WsPubSubBridge::build(config, ws, redis).await?;
    bridge.spawn_subscriber_loop();
    Ok(bridge)
}
```

**Verify — this is the check that proves the split works:**

```bash
cargo build --bin meno-api --bin meno-worker
grep -n 'run_jobs' apps/api/src/main.rs    # expect: only a comment, no call

# The real test: with only the API running, nothing may be picked up.
cargo run --bin meno-api &
sleep 8
psql "$DATABASE_URL" -c "SELECT job_type, run_at, attempts FROM apalis_jobs ORDER BY run_at DESC LIMIT 5;"
# expect: no attempt counters increment
kill %1
```

**Commit:**

```
refactor(api): split web and worker into separate binaries behind an AppRole guard

start_background_workers spawned an Apalis monitor and two interval loops inside
the web process, so every horizontally-scaled replica executed every background
job — duplicate emails, duplicate fan-out, duplicate LiveKit room teardown. Both
binaries now build through bootstrap::build_context, and the monitor starts only
via run_jobs, gated on AppRole::runs_jobs(), making re-enabling it in the web
binary a compile error rather than a production incident.

Also adds the missing migration runner and Apalis setup to build_context so the
schema and job tables exist before anything reads them, removes all three startup
expect/unwrap panics, and deduplicates the two shutdown_signal implementations.

Behavioural gaps in the jobs system are deliberately unchanged: the unwired
BroadcastScheduledFanOutJob and the per-process interval loops remain, and are
tracked separately.
```

---

## Step 8 — Adapter traits

**Why.** The copied `state.rs` does `Arc::new(RoomClient::with_api_key(...))` inline and
`broadcast/service.rs` calls `.mint_token()` on the concrete SDK, so "what happens when
LiveKit returns 503" is unreachable from a test. This is also the prerequisite for
splitting the service in Step 9 without losing testability.

### Step 8.1 — The four ports

```rust
// apps/api/src/infrastructure/traits.rs
//! Ports for every external dependency.
//!
//! Each has one real implementation and one in-memory double. Services depend on the
//! trait; `infrastructure` depends on the SDK. That inversion is what makes the service
//! layer testable without a network — and it is what lets Step 9 split BroadcastService
//! without losing the ability to test its failure paths.

use async_trait::async_trait;
use meno_core::Error;
use std::collections::HashMap;

/// Object storage (Cloudflare R2 in prod, RustFS in dev).
#[async_trait]
pub trait ObjectStore: Send + Sync {
    /// Upload bytes and return the public URL.
    async fn put(&self, key: &str, bytes: Vec<u8>, content_type: &str) -> Result<String, Error>;

    /// Delete. Missing objects are not an error — delete is idempotent.
    async fn delete(&self, key: &str) -> Result<(), Error>;

    /// Fetch bytes, for the API-proxy path used when a bucket is private (§3.3).
    async fn get(&self, key: &str) -> Result<Vec<u8>, Error>;

    /// A URL the client can fetch directly. `None` when the bucket is private and the
    /// object must be served through the API.
    fn public_url(&self, key: &str) -> Option<String>;
}

/// Transactional email (Brevo in prod, an in-memory collector in tests).
#[async_trait]
pub trait EmailSender: Send + Sync {
    async fn send(&self, to: &str, template: &str, vars: &HashMap<String, String>) -> Result<(), Error>;
}

/// LiveKit room lifecycle and token minting.
#[async_trait]
pub trait LiveKitAdapter: Send + Sync {
    async fn mint_token(&self, user_id: Uuid, room: &str, can_publish: bool) -> Result<String, Error>;
    async fn end_room(&self, room: &str) -> Result<(), Error>;
    async fn remove_participant(&self, room: &str, identity: &str) -> Result<(), Error>;
    async fn update_permission(&self, room: &str, identity: &str, can_publish: bool) -> Result<(), Error>;
    async fn list_participants(&self, room: &str) -> Result<Vec<String>, Error>;
}

/// Push notifications (FCM). No-op when `PUSH_ENABLED=false`.
#[async_trait]
pub trait PushSender: Send + Sync {
    async fn send_to_tokens(
        &self,
        tokens: &[String],
        title: &str,
        body: &str,
        data: &HashMap<String, String>,
    ) -> Result<(), Error>;
}
```

### Step 8.2 — The doubles, which are the point

```rust
// apps/api/src/infrastructure/doubles.rs
//! In-memory doubles, behind the `testing` cargo feature.
//!
//! Each records what it was asked to do, so a test can assert on the *interaction* —
//! "joining a broadcast that is not live must not call mint_token" — a class of
//! assertion that is impossible without a seam.

use std::sync::Mutex;

#[derive(Default)]
pub struct FakeLiveKit {
    minted: Mutex<Vec<(Uuid, String, bool)>>,
    fail_with: Mutex<Option<&'static str>>,
}

impl FakeLiveKit {
    #[must_use]
    pub fn new() -> Self { Self::default() }

    /// A LiveKit that always fails, for testing the 503 path.
    #[must_use]
    pub fn unavailable() -> Self {
        Self { minted: Mutex::new(Vec::new()), fail_with: Mutex::new(Some("livekit")) }
    }

    /// Every token minted, as (user, room, can_publish).
    #[must_use]
    pub fn minted_tokens(&self) -> Vec<(Uuid, String, bool)> {
        self.minted.lock().expect("test mutex").clone()
    }
}

#[async_trait]
impl LiveKitAdapter for FakeLiveKit {
    async fn mint_token(&self, user_id: Uuid, room: &str, can_publish: bool) -> Result<String, Error> {
        if let Some(service) = *self.fail_with.lock().expect("test mutex") {
            return Err(Error::Upstream { service, detail: "fake failure".into() });
        }
        self.minted.lock().expect("test mutex").push((user_id, room.into(), can_publish));
        Ok(format!("fake-token-{user_id}"))
    }
    // … remaining methods: record and succeed
}
```

> `expect` in a test double is fine. The `unwrap_used`/`expect_used` deny applies to
> `src/`, not to `#[cfg(test)]` or a `testing` feature. Poisoning here means a test
> panicked while holding the lock and every later assert is meaningless, so panicking
> loudly is correct.

### Step 8.3 — The no-op implementations matter as much as the doubles

They are what makes "disabled" real rather than theoretical:

```rust
/// Used when `PUSH_ENABLED=false`. Push is genuinely optional: a Meno deployment
/// without FCM must still boot, register and stream.
pub struct NoopPush;

#[async_trait]
impl PushSender for NoopPush {
    async fn send_to_tokens(
        &self,
        _t: &[String],
        _title: &str,
        _body: &str,
        _d: &HashMap<String, String>,
    ) -> Result<(), Error> {
        tracing::debug!("push disabled; dropping notification");
        Ok(())
    }
}
```

### Step 8.4 — Swap the concrete types

```rust
// state.rs
fn build_adapters(config: &Config) -> Result<(Arc<dyn LiveKitAdapter>, Arc<dyn PushSender>), Error> {
    let livekit: Arc<dyn LiveKitAdapter> = Arc::new(LivekitService::new(livekit_cfg));
    let push: Arc<dyn PushSender> = match &config.push.service_account_json {
        Some(json) => Arc::new(FirebasePush::new(json)?),
        None => Arc::new(NoopPush),
    };
    Ok((livekit, push))
}
```

Then in `broadcast/state.rs`, `LivekitService` becomes `Arc<dyn LiveKitAdapter>` —
which is what lets Step 9 build the service against `FakeLiveKit::new()`.

**Verify / commit:**

```bash
cargo check --workspace --all-targets
grep -rn 'RoomClient' apps/api/src/ --include='*.rs' | grep -v 'infrastructure/livekit/'
# expect: no hits — the SDK must not be reachable from service code
```

```
feat(api): introduce ObjectStore/EmailSender/LiveKitAdapter/PushSender ports

state.rs constructed SDK clients inline and services called concrete SDK methods,
so no failure path for LiveKit, R2, Brevo or FCM was reachable from a test. Each
port now has one real implementation and one in-memory double that records its
interactions, and optional integrations gain no-op implementations so a deployment
with PUSH_ENABLED=false or LIVEKIT_ENABLED=false still boots rather than failing
to start. This is the prerequisite for testing the BroadcastService split.
```

## Step 9 — Split `BroadcastService`

**Why.** §5.1. The copied `service.rs` is **1,605 lines** doing six jobs: CRUD, LiveKit
token minting, Redis live-count bookkeeping, WS pub/sub, job enqueueing, and response
projection. It opens with an eight-`expect()` builder that panics during construction,
and unwraps an `Option` on a live request path at line 1055 — directly after an
`is_none()` early return, making that return unreachable.

### Step 9.1 — Replace the builder first

Do this **before** splitting. It is an independent, smaller change that makes the split
safe, and it is what lets you remove the lint escape hatch from Step 3.6.

```rust
// BEFORE (copied from master)
pub struct BroadcastServiceBuilder<R: BroadcastRepo = BroadcastRepository> { /* 8 fields */ }
impl<R: BroadcastRepo> BroadcastServiceBuilder<R> {
    pub fn repo(mut self, repo: Arc<R>) -> Self { … }   // × 8 setters
    pub fn build(self) -> BroadcastService<R> {
        // … eight .expect() calls, each a panic path at startup
    }
}
```

```rust
// AFTER
/// Everything `BroadcastService` needs, in one struct.
///
/// A struct rather than a builder because every field is required: a builder with eight
/// `expect()` calls is eight ways for the process to panic during startup instead of
/// eight fields the compiler checks. `Arc<dyn _>` rather than a generic parameter so
/// the dependency list is identical for the real repository and for a test double.
#[derive(Clone)]
pub struct BroadcastDeps {
    pub repo: Arc<dyn BroadcastRepo>,
    pub cache: Arc<dyn BroadcastCache>,
    pub redis: Redis,
    pub pubsub: Arc<WsPubSubBridge>,
    pub ws: WsService,
    pub jobs: Jobs,
    pub livekit: Arc<dyn LiveKitAdapter>,
}

pub struct BroadcastService {
    deps: BroadcastDeps,
}

impl BroadcastService {
    /// Total and infallible. No `Result`, no `Option`: every field is supplied, and an
    /// unavailable adapter is a no-op implementation, not `None`.
    #[must_use]
    pub fn new(deps: BroadcastDeps) -> Self {
        Self { deps }
    }
}
```

`LivekitService` → `Arc<dyn LiveKitAdapter>` is Step 8 paying for itself: the service can
now be built in a test against `FakeLiveKit::new()` with no network.

```rust
// broadcast/state.rs
impl BroadcastState {
    pub fn new(/* … */) -> Self {
        let deps = BroadcastDeps {
            repo: Arc::new(BroadcastRepository::new(db.clone())),
            cache: Arc::new(BroadcastRedisCache::new(redis.clone())),
            redis, pubsub, ws, jobs, livekit,
        };
        Self { service: Arc::new(BroadcastService::new(deps)) }
    }
}
```

**Commit this separately:**

```
refactor(broadcast): replace the eight-expect builder with an infallible new(deps)

BroadcastServiceBuilder::build called .expect() on all eight dependencies, so
every misconfiguration was a startup panic rather than a compile error, and the
generic parameter forced callers to name the concrete repository type. A
BroadcastDeps struct of Arc<dyn …> makes the dependency list explicit and checked,
drops the builder, and swaps LivekitService for the LiveKitAdapter trait.

No behaviour change: same collaborators, same construction order.
```

### Step 9.2 — Classify all 30 methods first

Do not start writing before you know where everything goes. The compiler gives you the
list. The copied file has **41 functions** in total: **11 builder functions that
disappear entirely** with Step 9.1, **24 public service methods**, and **6 private
helpers**.

```bash
grep -nE '^\s+(pub )?(async )?fn ' apps/api/src/modules/broadcast/service.rs | sed 's/(.*//'
```

| New owner                 | Methods (line numbers from the copied file)                                                                                                                                                                         | Count |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----- |
| `BroadcastCommandService` | `create` 135, `update` 199, `delete` 263, `start` 344, `end` 485                                                                                                                                                    | 5     |
| `BroadcastQueryService`   | `get_broadcast` 994, `get_broadcasts` 1035, `get_now_live` 1075, `get_live_for_you` 1096, `get_recently_live` 1124, `get_participants` 1149, `get_live_participants` 1166, `find_by_id` 1423, `is_active_host` 1432 | 9     |
| `LiveSessionCoordinator`  | `join` 602, `leave` 727, `add_cohost` 793, `remove_cohost` 881, `refresh_token` 1322                                                                                                                                | 5     |
| `PresenceService`         | `find_active_hosted_by` 1388, `find_active_participant` 1395, `remove_participant` 1402, `get_participants_ids` 1412, `get_subscriber_ids` 1419                                                                     | 5     |
| `ResponseProjector`       | `build_response` 1437, `build_ctx` 1493, `deduplicate_cohosts` 1529, `build_broadcast_page` 1584                                                                                                                    | 4     |

Why this grouping: commands own the write paths (already transactional); queries own the
cached reads; `LiveSessionCoordinator` owns anything that mints a LiveKit token or
mutates a room; `PresenceService` owns the Redis live state; `ResponseProjector` owns
pure mapping with no I/O.

`get_broadcasts_from_db` (1560) and `get_participants_from_db` (1572) are the remaining
2 of the 6 private helpers — cache-miss paths, so → `BroadcastQueryService`, the only
caller.

> The line numbers assume the reference is `903c3ba`, and every one above was verified
> against it. **Recompute them from your own checkout** — do not trust a document's line
> numbers if `git log -1 --oneline master` says anything other than `903c3ba`.
> Note `get_participants_ids` really is plural in the source.

### Step 9.3 — Extract bottom-up, compiling after each pass

The order matters: each pass is independently verifiable, and a bug in pass 1 cannot be
confused with a bug in pass 5.

**Pass 1 — `ResponseProjector`.** No dependencies on the others, pure transformation,
removes ~150 lines immediately.

```rust
/// Projects a `Broadcast` plus its relations into the client-facing response.
///
/// Pure: no database, no Redis, no LiveKit. Every `BroadcastResponse` field needing an
/// extra query is resolved here from data the caller already fetched — which is the
/// point, because the alternative is `build_response` reaching back into the
/// repository, and that is what made the original 1,605 lines.
pub struct ResponseProjector {
    repo: Arc<dyn BroadcastRepo>,
}

impl ResponseProjector {
    #[must_use]
    pub fn new(repo: Arc<dyn BroadcastRepo>) -> Self { Self { repo } }

    pub async fn project(
        &self,
        broadcast: Broadcast,
        ctx: RequestCtx,
    ) -> Result<BroadcastResponse, Error> {
        // body moved verbatim from build_response / build_ctx / deduplicate_cohosts
    }
}
```

**Pass 2 — `PresenceService`.** Where the typed `RedisKey` from Step 12 starts paying:

```rust
/// Redis-backed live state: participant counts, host grace windows, start markers.
///
/// Separate from the command services because its failure mode differs: Redis being
/// down must degrade a count, not fail a `start` or an `end`. Each method therefore has
/// an explicit degradation policy rather than propagating the error.
pub struct PresenceService {
    redis: Redis,
}

impl PresenceService {
    #[must_use]
    pub fn new(redis: Redis) -> Self { Self { redis } }

    /// Live participant count. **Degrades to `None`** when Redis is unavailable — the
    /// caller renders "—" rather than failing. A feed that 500s because a cache is down
    /// is worse than one showing a stale count.
    pub async fn live_count(&self, broadcast_id: Uuid) -> Option<i64> { … }

    /// How long after `start_time` a host may still reclaim the room. Always has a TTL.
    pub async fn set_host_grace(&self, broadcast_id: Uuid, secs: i64) -> Result<(), Error> { … }

    pub async fn participant_ids(&self, broadcast_id: Uuid) -> Result<Vec<Uuid>, Error> { … }
}
```

**Pass 3 — `LiveSessionCoordinator`.** The LiveKit boundary:

```rust
/// Owns everything that touches a LiveKit room: token minting, cohost permission
/// changes, room teardown.
///
/// Exists so the failure mode of an unreachable media server is decided in one place.
/// On master a LiveKit 503 surfaced as `LiveKitUnavailable` at three call sites with
/// three behaviours; here it is one policy, and it is assertable.
pub struct LiveSessionCoordinator {
    livekit: Arc<dyn LiveKitAdapter>,
    presence: PresenceService,
}

impl LiveSessionCoordinator {
    /// Propagates `Error::Upstream` — a user cannot join without a token, so there is
    /// no honest degraded mode.
    pub async fn mint_join_token(&self, user_id: Uuid, room: &str) -> Result<String, Error> { … }

    /// Tear down a room at broadcast end. **Failure is logged, not propagated**: the
    /// broadcast is already over from the database's point of view, and a LiveKit
    /// outage must not leave it stuck live forever.
    pub async fn end_room(&self, room: &str) { … }
}
```

**Pass 4 — `BroadcastQueryService` and `BroadcastCommandService`**, the two large ones.
One holds the repository for reads, one for writes:

```rust
/// Read paths. Every method may be served from cache; the repository is the cache-miss
/// path, not the primary path.
pub struct BroadcastQueryService {
    repo: Arc<dyn BroadcastRepo>,
    cache: Arc<dyn BroadcastCache>,
    projector: ResponseProjector,
    presence: PresenceService,
}
```

**Pass 5 — compose**, so handlers get exactly what they need:

```rust
pub struct BroadcastState {
    pub commands: Arc<BroadcastCommandService>,
    pub queries: Arc<BroadcastQueryService>,
    pub live: Arc<LiveSessionCoordinator>,
}
```

`handlers.rs` changes from `state.service.get_broadcast(..)` to
`state.queries.get_broadcast(..)`. That is the entire visible effect — and worth noticing:
**the handler can now see at a glance that `start` is a command and `list` is a query**,
information the single 1,605-line service had destroyed.

### Step 9.4 — Fix the `Option` unwrap while you are in there

Both live-path defects are in code you are moving:

```rust
// apps/api/src/modules/broadcast/service.rs:1055 — BEFORE
async fn get_broadcasts(
    &self,
    creator_id: Uuid,
    params: ListParams,
) -> Result<CursorPage<BroadcastResponse>> {
    if broadcast_list_cache_key(&creator_id, &params).is_none() {
        return self.fetch_from_db(creator_id, params).await;
    }
    let cache_key = broadcast_list_cache_key(&creator_id, &params).unwrap(); // panics
    todo!()
}
```

```rust
// AFTER — the key is computed once, and the early return is the only exit
async fn get_broadcasts(
    &self,
    creator_id: Uuid,
    params: ListParams,
) -> Result<CursorPage<BroadcastResponse>> {
    let Some(cache_key) = broadcast_list_cache_key(&creator_id, &params) else {
        tracing::warn!(%creator_id, "broadcast list cache key unavailable; bypassing cache");
        return self.fetch_from_db(creator_id, params).await;
    };
    todo!()
}
```

> This is in scope because it is on a live request path and is a one-line fix in code you
> are already touching. The `total_participants` drift is **not** fixed here — it needs a
> migration and a semantic decision, and stays on the carried-over list.

### Step 9.5 — Remove the lint escape hatch

The builder is gone and the unwrap is gone, so the workspace policy can now apply:

```toml
# apps/api/Cargo.toml — delete this block entirely
# [lints.clippy]
# unwrap_used = "allow"
# expect_used = "allow"
# panic       = "allow"
```

```bash
cargo clippy --workspace --all-targets 2>&1 | grep -E '^(error|warning)' | head -30
```

Expect hits in the still-unported code. Fix each with a real error return; **do not**
re-add the `allow`.

**Verify / commit:**

```bash
cargo check --workspace --all-targets
cargo test --workspace           # cursor + core + error tests still green
wc -l apps/api/src/modules/broadcast/*.rs | sort -rn | head
```

```
refactor(broadcast): split BroadcastService into five collaborating services

1,605 lines and six responsibilities in one type. Split into
BroadcastCommandService (writes), BroadcastQueryService (cached reads),
LiveSessionCoordinator (LiveKit boundary), PresenceService (Redis live state) and
ResponseProjector (pure response mapping), composed in BroadcastState.

Each service now has one failure policy instead of one per call site:
PresenceService degrades a live count to None when Redis is down rather than
failing the request, and LiveSessionCoordinator logs a room-teardown failure
rather than leaving a broadcast stuck live. Both behaviours are assertable, which
they were not when they were three lines apart in a 1,605-line file.

Also removes the Option unwrap on the broadcast-list path, which sat directly
after an is_none() early return and made that return dead, and deletes the
temporary clippy escape hatch so the workspace deny policy now applies.
```

### Step 9.6 — The same split elsewhere

§5.1 is explicit that broadcast is the _example_, not the exception. Do these after
broadcast is green:

| File                       | Lines | Split into                                                |
| -------------------------- | ----- | --------------------------------------------------------- |
| `notes/repository.rs`      | 998   | `NoteQueryRepo` / `NoteCommandRepo` / `FolderRepo`        |
| `notes/service.rs`         | 641   | `NoteService` / `FolderService` / `NoteSearchService`     |
| `notifications/service.rs` | 452   | `NotificationCommandService` / `NotificationQueryService` |
| `auth/services.rs`         | 390   | `AuthService` / `TokenService` / `OAuthService`           |

Same pattern, lower risk, and by then the pattern is obvious.

---

## Step 10 — Consolidate the nine error enums

**Why.** The Phase 2 gate: _"clients can rely on `code`"_. `Error` exists in
`crates/core` since Step 4 but nothing uses it until now.

Measured from the copied tree:

```
infrastructure + shared/errors.rs   MenoError            10 variants
modules/auth/errors.rs              AuthError            21
modules/broadcast/errors.rs         BroadcastError       35
modules/chat/errors.rs              ChatError            14
modules/notes/errors.rs             NotesError           10
modules/profile/errors.rs           ProfileError         11
modules/settings/errors.rs          SettingsError         4
modules/subscribers/errors.rs       SubscribersError      7
modules/notifications/error.rs      NotificationError      6
```

118 variants across nine enums, nine hand-rolled `IntoResponse` impls, and one
inconsistency that is pure accident: `notifications/error.rs` is singular, every other
file is plural.

### Step 10.1 — One module per commit, smallest first

Nine `#[from]` variants make a blanket `sed` unsafe, so migrate one module at a time,
starting with `settings` (4 variants) and ending with `broadcast` (35).

```bash
# Safe because the enums are file-local.
for m in settings subscribers notes chat profile auth notifications broadcast; do
  f="apps/api/src/modules/$m/errors.rs"; [ -f "$f" ] || f="apps/api/src/modules/$m/error.rs"
  grep -l "\b${m%?}Error\b" apps/api/src/modules/$m/*.rs
done
```

After the rewrite, delete each `errors.rs` and its `mod` line. `cargo check` after each
module — one module's errors are readable, nine are not.

### Step 10.2 — The mapping table

This is judgement, not sed. **Preserve every message string exactly** — the Flutter app
displays them.

| Old variant                                    | New                                                                                                          | Code                    |
| ---------------------------------------------- | ------------------------------------------------------------------------------------------------------------ | ----------------------- |
| `AuthError::EmailTaken`                        | `Error::Conflict { code: EmailTaken, message: "Email already in use".into() }`                               | `EMAIL_TAKEN`           |
| `AuthError::InvalidCredentials`                | `Error::Unauthorized { code: InvalidCredentials, .. }`                                                       | `INVALID_CREDENTIALS`   |
| `AuthError::AccessTokenExpired`                | `Error::Unauthorized { code: TokenExpired, .. }`                                                             | `TOKEN_EXPIRED`         |
| `AuthError::RefreshTokenExpired`               | `Error::Unauthorized { code: RefreshTokenExpired, .. }`                                                      | `REFRESH_TOKEN_EXPIRED` |
| `AuthError::UserNotFound`                      | `Error::NotFound { resource: "user", code: NotFound }`                                                       | `NOT_FOUND`             |
| `AuthError::Database(e)`                       | `infrastructure::database::db_err("auth::repo", e)`                                                          | `INTERNAL_ERROR`        |
| `AuthError::ValidationError(e)`                | `Error::Validation { fields: map(e) }`                                                                       | `VALIDATION_FAILED`     |
| `BroadcastError::NotLive`                      | `Error::Conflict { code: Conflict, message: "Broadcast is not currently live".into() }`                      | `CONFLICT`              |
| `BroadcastError::NotCreator`                   | `Error::Forbidden { code: NotCreator, .. }`                                                                  | `NOT_CREATOR`           |
| `BroadcastError::NotParticipant`               | `Error::Forbidden { code: NotParticipant, .. }`                                                              | `NOT_PARTICIPANT`       |
| `BroadcastError::LiveKitUnavailable`           | `Error::Upstream { service: "livekit", detail }`                                                             | `UPSTREAM_UNAVAILABLE`  |
| `BroadcastError::CohostLimitExceeded(n)`       | `Error::Conflict { code: Conflict, message: format!("Cohost limit exceeded. Maximum {n} cohosts allowed") }` | `CONFLICT`              |
| `NotesError::VersionConflict(e)`               | `Error::Conflict { code: VersionConflict, message }`                                                         | `VERSION_CONFLICT`      |
| `ProfileError::FileTooLarge`                   | `Error::BadRequest { code: BadRequest, message: "File must be under 5MB".into() }`                           | `BAD_REQUEST`           |
| _(all nine)_ `Cursor(CursorError)`             | `Error::BadRequest { code: InvalidCursor, message: format!("Invalid pagination cursor: {e}") }`              | `INVALID_CURSOR`        |
| _(all nine)_ `Database` / `Redis` / `Internal` | `db_err(..)` / `redis_err(..)` / `Error::Internal { .. }`                                                    | `INTERNAL_ERROR`        |

Two entries need a note. **`ValidationError` → `Error::Validation`** is a move, not a
rewrite — the old `IntoResponse` already builds the same
`HashMap<String, Vec<String>>`. Keep 422: on `master`, validation returns 422 while
`BadRequest` returns 400, and that inconsistency should go away, not be preserved.
**`ProfileError::InvalidFileType`/`FileTooLarge`** were 400 via `BadRequest`; they
become `Error::BadRequest` with an explicit code, which is the first time these
responses have a code a client can branch on.

### Step 10.3 — One `IntoResponse` for the whole app

```rust
// apps/api/src/infrastructure/error_response.rs
use axum::{Json, http::StatusCode, response::IntoResponse};
use meno_core::Error;

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        // to_body already decided status, code and message, and already stripped
        // internal/upstream detail. This only renders it.
        let body = meno_core::error::to_body(&self);

        // 5xx means we failed, not the caller. Log here so no path can return a 500
        // without a log line.
        if body.status_code >= 500 {
            tracing::error!(code = body.code, error = %self, "request failed");
        }

        let status =
            StatusCode::from_u16(body.status_code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (status, Json(body)).into_response()
    }
}
```

> `unwrap_or`, not `unwrap`. The fallback is unreachable because `to_body` only produces
> codes from a fixed set — but `unwrap_or` says that in a lint-clean way, whereas `unwrap`
> is a panic waiting for someone to add a status.

Handlers then return `Result<Json<MenoResponse<T>>, Error>` and `?` works with no map.
`MenoResponse`'s `code` field, currently set to the status _as a string_
(`"401 Unauthorized"`), is free to carry the real code.

**Verify — the real test of the gate:**

```bash
grep -rn 'MenoError\|AuthError\|BroadcastError\|ChatError\|NotesError\|ProfileError\|SettingsError\|SubscribersError\|NotificationError' \
  apps/api/src/ | grep -v INTERNAL_ERROR
# expect: zero hits

cargo check --workspace --all-targets
cargo run --bin meno-api &

curl -s localhost:8080/api/v1/broadcasts | jq
# expect 401 with code "UNAUTHORIZED" — a stable code, not "401 Unauthorized"

curl -s localhost:8080/api/v1/broadcasts/00000000-0000-0000-0000-000000000000 \
  -H "Authorization: Bearer $TOKEN" | jq
# expect 404 with code "NOT_FOUND"
```

**Commit per module** (nine commits), e.g.:

```
refactor(auth): migrate AuthError to the canonical core Error

Removes the per-module enum and its hand-rolled IntoResponse in favour of
meno_core::Error, preserving every client-visible message string exactly. Driver
errors are erased at the repository boundary via infrastructure::database::db_err
so no sqlx::Error reaches domain code.

Response code changes from the status as a string ("401 Unauthorized") to the
stable wire code ("UNAUTHORIZED"), which is the contract the Flutter and Next.js
clients depend on.
```

> **Review discipline:** `git show <sha> | grep '^+.*message'` and read every changed
> string. A reworded message breaks the Flutter app's user-facing text with no test
> failure and no compile error.

## Step 11 — The test suite

**Why.** On `master` the entire test tree is **three 0-byte files**:
`tests/mod.rs`, `tests/auth/mod.rs`, `tests/auth/repository_test.rs`. There are zero
tests, and the layout is wrong anyway — Cargo auto-discovers only top-level
`tests/*.rs`, so `tests/mod.rs` was the only real target. The port is the right moment to
fix this, because you have a known-good reference to compare against.

### Step 11.1 — Fix the layout

```bash
rm -rf apps/api/tests
mkdir -p apps/api/tests/contract
```

Cargo treats each top-level `tests/*.rs` as a separate binary, and `tests/<dir>/mod.rs`
is **not** auto-discovered. So:

```
apps/api/tests/
├── cursor_contract.rs     # pure, no infrastructure — the fast gate
├── repositories.rs        # #[sqlx::test], needs Postgres
├── middleware.rs          # needs Redis
└── contract/
    ├── mod.rs
    ├── errors.rs
    └── auth.rs
```

```rust
// apps/api/tests/contract/mod.rs
//! Shared helpers for the contract tests.
#![allow(clippy::unwrap_used, clippy::expect_used)]  // tests may panic freely

pub async fn test_state() -> (PgPool, Arc<MenoState>) { /* … */ }
```

> **`unwrap_used`/`expect_used` are denied in `src/`, not in `tests/`.** A test that
> panics should fail loudly, not wrap its failure in a `Result`. This is why the lint
> policy lives in `[workspace.lints]` with the per-member escape hatch, rather than as
> `#![deny]` in `lib.rs` — a source-level `deny` would apply to tests too.

### Step 11.2 — The four layers, in build order

| Layer           | Tool                         | Needs infra      | What it pins                        |
| --------------- | ---------------------------- | ---------------- | ----------------------------------- |
| **Pure**        | `#[test]`                    | none             | cursors, error mapping, validators  |
| **Unit**        | `mockall` + `FakeLiveKit`    | none             | service logic, failure paths        |
| **Integration** | `#[sqlx::test]`              | Postgres         | repositories, **the dead triggers** |
| **Contract**    | `tower::ServiceExt::oneshot` | Postgres + Redis | router → status/body, 401s          |

#### Layer 1 — pure (the highest value per line of effort)

`crates/core` already has 12 cursor tests and 5 error tests from Steps 3 and 4. Add the
service-level ones that need no infrastructure:

```rust
// apps/api/tests/cursor_contract.rs
//! The cursor contract, pinned at the integration boundary.
//!
//! These duplicate the crates/core unit tests deliberately: if a future refactor moves
//! pagination out of the pure crate, these still hold the wire format stable for the
//! Flutter and Next.js clients.

use meno_core::pagination::{Cursor, CursorPage};

#[test]
fn cursor_wire_format_is_stable() {
    // A golden value. If this changes, every already-persisted cursor in a client's
    // local storage silently breaks — there is no version negotiation.
    let c = Cursor::from_score_id(42, uuid::Uuid::from_u128(1));
    assert_eq!(c.0, "c2Nvcnk0MnwxMQ");   // verify against the current impl
}
```

> Compute that golden string from your actual implementation before committing it — do
> not take it from this document. It is illustrative.

#### Layer 2 — service failure paths, using the doubles

This is what Step 8 bought you:

```rust
// apps/api/src/modules/broadcast/service_tests.rs  (#[cfg(test)] mod)
#[tokio::test]
async fn joining_a_broadcast_that_is_not_live_never_mints_a_token() {
    let livekit = Arc::new(FakeLiveKit::new());
    let service = test_service(livekit.clone());

    let result = service.live.join(uuid::Uuid::new_v4(), NOT_LIVE_ID).await;

    assert!(result.is_err());
    assert!(
        livekit.minted_tokens().is_empty(),
        "a token was minted for a broadcast that is not live — this would grant \\
         audio access to a room nobody is in"
    );
}

#[tokio::test]
async fn livekit_unavailable_surfaces_as_503_not_500() {
    let service = test_service(Arc::new(FakeLiveKit::unavailable()));

    let err = service
        .live
        .mint_join_token(uuid::Uuid::new_v4(), "room")
        .await
        .expect_err("unavailable LiveKit must fail");

    assert_eq!(err.code(), ErrorCode::UpstreamUnavailable);
    let body = meno_core::error::to_body(&err);
    assert_eq!(body.status_code, 503);
    assert!(!body.message.contains("livekit.internal"), "upstream URLs must not leak");
}
```

#### Layer 3 — repository integration, and the dead triggers

This layer is what **would have caught the trigger defect** on `master`. Write it even
though the defect is carried over — the test will fail, and that failing test is the
tracking artefact:

```rust
// apps/api/tests/repositories.rs
//! Repository integration tests. Each runs in a transaction that is rolled back.

use sqlx::test::SqliteTest;

#[sqlx::test(migrations = "../../crates/db/migrations")]
async fn subscribing_increments_the_followers_counter(pool: PgPool) {
    let alice = create_user(&pool).await;
    let bob = create_user(&pool).await;

    let before: i64 = sqlx::query_scalar("SELECT followers FROM users WHERE id = $1")
        .bind(alice).fetch_one(&pool).await.expect("alice");

    subscribe(&pool, bob, alice).await;

    let after: i64 = sqlx::query_scalar("SELECT followers FROM users WHERE id = $1")
        .bind(alice).fetch_one(&pool).await.expect("alice");

    assert_eq!(after, before + 1,
        "users.followers did not increment — the 0013 trigger body contains \\
         'TG_OP' (a string literal) instead of TG_OP (the variable), so it never fires");
}
```

> The `#[sqlx::test(migrations = "...")]` path is relative to the **workspace root**,
> not to `apps/api`. That trips everyone once. It also applies all 18 migrations per
> test and rolls back, so tests are isolated and order-independent.
>
> **This test fails today, by design.** That is the point — it converts a code-reading
> suspicion into a reproducible failure with the cause written in the message. Leave it
> red, reference it from the tracking issue, and fix it in the phase that owns the
> triggers.

#### Layer 4 — contract

```rust
// apps/api/tests/contract/auth.rs
#[tokio::test]
async fn protected_routes_require_authentication() {
    let app = test_app().await;
    for path in ["/api/v1/broadcasts", "/api/v1/notes", "/api/v1/settings"] {
        let resp = app.clone().oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await.expect("router responded");
        assert_eq!(resp.status(), 401, "{path} must require auth");
    }
}
```

This is the cheapest regression net in the suite: it catches a route accidentally
mounted outside the authenticated nest, which is exactly how a protected endpoint ends
up public.

### Step 11.3 — Fixtures

```rust
// tests/factories.rs
//! Factory helpers. `#[sqlx::test]` gives every test a clean migrated database, so
//! factories only need to insert the minimum a given test needs — no truncation, no
//! ordering assumptions, no flaky teardown.

pub async fn create_user(pool: &PgPool) -> Uuid { /* insert + return id */ }
pub async fn create_broadcast(pool: &PgPool, creator: Uuid) -> Uuid { /* … */ }
pub async fn create_subscription(pool: &PgPool, subscriber: Uuid, creator: Uuid) { /* … */ }
```

**Verify:**

```bash
cargo test --workspace 2>&1 | tail -30
```

Expect: pure and unit layers green, repository layer **one known failure** (the trigger
test), contract layer green.

**Commit:**

```
test: replace the empty test tree with a four-layer suite

master's entire test tree was three 0-byte files, and its layout was wrong —
Cargo only auto-discovers top-level tests/*.rs, so tests/mod.rs was the sole real
target. Rebuilds it as pure / unit / integration / contract layers over
crates/core's unit tests.

The subscriber-counter test is deliberately red: it pins the 0013 trigger defect
(its body compares the string literal 'TG_OP' to 'INSERT', so it never fires) and
the assertion message names the cause. Fixing it belongs to the phase that owns
the triggers, not the port.
```

---

# Phase 3 — Infrastructure

None of this touches Rust, so it cannot break Phases 1 or 2.

## Step 12 — `ops/`

**Why.** Three of the files being replaced are currently **broken in ways a contributor
cannot discover**, because they are scattered across the repo root and `resources/`:

- `resources/prometheus/prometheus.yml` scrapes `192.168.111.244:8080` — a hardcoded LAN
  IP from one person's machine. In the compose network the API is `api:8080`.
- `promtail-config.yml` exists at the repo root _and_ under `resources/promtail/`. The
  root copy is stale and mounted by nothing.
- `.docker/postgres/init.sql` runs `create database meno_dev owner postgres`, but compose
  sets `POSTGRES_USER=meno`, so the owner does not exist — and it is mounted by nothing.

Step 1.2 already wrote the backing-services compose file and the ports. **This step
finishes it**: add the observability config files, then add the `api` service.

Start the config files from `master` rather than from memory, so you inherit anything
still correct, then fix them:

```bash
mkdir -p ops/prometheus ops/promtail
git show master:resources/prometheus/prometheus.yml > ops/prometheus/prometheus.yml
git show master:resources/promtail/promtail-config.yml > ops/promtail/promtail.yml
```

`ops/docker-compose.yml` already exists from Step 1.2 — **do not overwrite it.** You are
adding to it, which is the opposite of what the old guide told you to do here.

### Step 12.1 — Fix the scrape target

```yaml
# ops/prometheus/prometheus.yml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: meno-api
    # Compose service name. The old value was a hardcoded LAN IP that resolved on
    # exactly one machine on earth.
    static_configs:
      - targets: ["api:8080"]
        labels:
          service: meno-api
          role: web
```

The worker exposes no port, so it is **not** a scrape target. Its activity appears
indirectly via the Apalis metrics the API records when pushing jobs.

### Step 12.2 — Grafana provisioning (entirely absent today)

Without provisioning files, every fresh `docker compose up` gives an empty Grafana with
no datasource, and the §12 gate _"dashboards load automatically"_ can never pass.

```yaml
# ops/grafana/provisioning/datasources/prometheus.yml
apiVersion: 1
datasources:
  - name: Prometheus
    type: prometheus
    access: proxy
    url: http://prometheus:9090
    isDefault: true
    editable: false
```

```yaml
# ops/grafana/provisioning/datasources/loki.yml
apiVersion: 1
datasources:
  - name: Loki
    type: loki
    access: proxy
    url: http://loki:3100
    editable: false
```

> `editable: false` on both. Editable datasources in a compose stack mean every developer
> has different dashboards and nobody can reproduce another's bug.

### Step 12.3 — Loki config

Step 1.2's compose mounts a Loki config that does not exist yet, so `docker compose up`
would fail on a missing bind mount:

```yaml
# ops/promtail/loki.yml — minimal single-binary Loki for local dev.
auth_enabled: false

server:
  http_listen_port: 3100

common:
  instance_addr: 127.0.0.1
  path_prefix: /loki
  storage:
    filesystem:
      chunks_directory: /loki/chunks
      rules_directory: /loki/rules
  replication_factor: 1
  ring:
    instance_addr: 127.0.0.1
    kvstore:
      store: inmemory

schema_config:
  configs:
    - from: 2024-01-01
      store: tsdb
      object_store: filesystem
      schema: v13
      index:
        prefix: index_
        period: 24h
```

Add a `loki_data` volume to the `loki` service in `ops/docker-compose.yml` so logs
survive a restart.

### Step 12.4 — Add the `api` service

The Step 1.2 compose deliberately has no `api` service, because in development you run
the API natively. Add it here, behind a profile, so nobody switches to it by accident:

```yaml
# ── Verification only ────────────────────────────────────────────────────
# The normal dev loop is `cargo run --bin meno-api` NATIVELY. This service exists
# so the Dockerfile from Step 13 can be verified, and so a container-only failure
# can be reproduced. Do NOT use it as your daily driver: every source change costs
# a full image rebuild instead of an incremental cargo build.
api:
  build:
    context: .. # the repo root — needs crates/, .sqlx/, Cargo.lock
    dockerfile: ops/Dockerfile
  container_name: meno-api
  env_file: [../.env]
  environment:
    RUST_LOG: info
    # Inside the compose network, services address each other BY NAME, never localhost.
    DATABASE_URL: postgres://meno:password@postgres:5432/meno_dev
    REDIS_URL: redis://redis:6379
    STORAGE_ENDPOINT: http://storage:9000
  ports:
    - "8080:8080"
  depends_on:
    postgres: { condition: service_healthy }
    redis: { condition: service_healthy }
    storage: { condition: service_healthy }
  profiles:
    - verify # reached only via: docker compose --profile verify up api
```

`profiles: [verify]` is the key line. Plain `docker compose up` will **not** start it, so
you cannot accidentally collide with the native `cargo run` on port 8080. Reaching it
requires typing `--profile verify` deliberately.

**Verify / commit:**

```bash
docker compose -f ops/docker-compose.yml config >/dev/null && echo "compose valid ✅"

# Full stack except the api service.
docker compose -f ops/docker-compose.yml up -d
docker compose -f ops/docker-compose.yml ps
# expect: postgres, redis, storage, mailpit, prometheus, loki, promtail, grafana healthy

# The API, natively — the actual dev loop.
make run &
curl -s localhost:8080/health | jq
# expect: {"status":"ok","db":true,"redis":true}
```

Then confirm Grafana auto-loaded its datasources: <http://localhost:3001>
(`admin` / `admin`) → Connections → Prometheus and Loki should both be listed with no
manual setup.

```
ops: add prometheus, loki, promtail and grafana provisioning; add api behind the verify profile

Fixes three defects inherited from master: the scrape target was a hardcoded
192.168.111.244 (now api:8080), the root promtail-config.yml was a stale duplicate
mounted by nothing, and .docker/postgres/init.sql created a database owned by a role
compose never creates. Adds the Loki and promtail configs plus Grafana datasource
provisioning so dashboards load without manual setup.

The api service is gated behind the `verify` profile because the normal dev loop runs
the API natively with cargo run; an ungated service would contend for port 8080 and turn
every source edit into a full image rebuild.
```

---

## Step 13 — Dockerfile and `render.yaml`

### Step 13.1 — The Dockerfile

The one on `master` **fails outright**: it sets `SQLX_OFFLINE=true` and copies a `.sqlx/`
without `total_participants`, so `cargo build --release` errors at the 3 `E0063` sites.
Step 5.4 fixed the cache; this fixes the rest.

```dockerfile
# syntax=docker/dockerfile:1.7
# Build context MUST be the repo root (it needs crates/, .sqlx/ and Cargo.lock).

ARG RUST_VERSION=1.88
ARG DEBIAN_RELEASE=bookworm

# ── build ────────────────────────────────────────────────────────────────────
FROM rust:${RUST_VERSION}-slim-${DEBIAN_RELEASE} AS builder
WORKDIR /app

# sqlx needs pkg-config only because of tls-rustls' ring dependency; OpenSSL is
# gone now that the workspace pins rustls everywhere.
RUN apt-get update \
 && apt-get install -y --no-install-recommends pkg-config ca-certificates \
 && rm -rf /var/lib/apt/lists/*

# Dependency layer: manifests only, so a source-only change does not rebuild 41 crates.
COPY Cargo.toml Cargo.lock ./
COPY crates/core/Cargo.toml crates/core/Cargo.toml
COPY crates/db/Cargo.toml   crates/db/Cargo.toml
COPY apps/api/Cargo.toml    apps/api/Cargo.toml

# Placeholder sources so `cargo build` has something to compile for the dep layer.
RUN mkdir -p crates/core/src crates/db/src apps/api/src \
 && echo "" > crates/core/src/lib.rs \
 && echo "" > crates/db/src/lib.rs \
 && echo "fn main() {}" > apps/api/src/main.rs \
 && echo "" > apps/api/src/lib.rs \
 && echo "fn main() {}" > apps/api/src/worker.rs \
 && cargo build --release --locked \
 && rm -rf crates/core/src crates/db/src apps/api/src

COPY crates/ crates/
COPY apps/api/src apps/api/src
COPY .sqlx ./.sqlx

ENV SQLX_OFFLINE=true
RUN cargo build --release --locked --bin meno-api --bin meno-worker

# ── runtime ──────────────────────────────────────────────────────────────────
FROM debian:${DEBIAN_RELEASE}-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/*

# Non-root. A process that does not need to write to the image should not be able to.
RUN useradd --system --create-home --uid 10001 meno
WORKDIR /app

COPY --from=builder /app/target/release/meno-api    /usr/local/bin/meno-api
COPY --from=builder /app/target/release/meno-worker /usr/local/bin/meno-worker
# Migrations are embedded at compile time, so nothing else is needed at runtime.

USER meno
EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=3s --start-period=20s --retries=3 \
  CMD curl -fsS http://localhost:8080/health || exit 1

CMD ["meno-api"]
```

Five changes from `master` that matter:

1. **`--locked`.** Builds against the committed `Cargo.lock` exactly. Without it a
   transitive dependency can resolve differently in CI than locally — "works on my
   machine" at the Docker layer.
2. **Both binaries.** §6 requires one image, two services.
3. **Non-root.** One line.
4. **A real `HEALTHCHECK`.** Render polls this; without it Render routes traffic to a
   process that is up but has no working DB pool.
5. **`crates/` is copied.** `master`'s Dockerfile copies only `apps/api/src`, so it
   breaks the moment a workspace crate exists.

> If Render build times (6–10 min per §13) become painful, add BuildKit cache mounts:
> `--mount=type=cache,target=/usr/local/cargo/registry` and
> `--mount=type=cache,target=/app/target`.

### Step 13.2 — `render.yaml`

```yaml
# ops/render.yaml — Render Blueprint: two services from one image.
services:
  - type: web
    name: meno-api
    runtime: docker
    dockerfilePath: ./ops/Dockerfile
    dockerContext: .
    plan: starter # Free spins down; WebSockets drop. §3.1.
    healthCheckPath: /health
    envVars:
      - key: DATABASE_URL
        fromDatabase: { name: meno-db, property: connectionString }
      - key: REDIS_URL
        fromService: { name: meno-kv, property: connectionString }
      - key: JWT_SECRET
        generateValue: true # Render generates and encrypts it
      - key: JWT_REFRESH_SECRET
        generateValue: true # a DIFFERENT secret; generateValue gives two
      - key: ENV
        value: production
      - key: CORS_ORIGINS
        sync: false # set in the dashboard once domains exist
      - key: PUSH_ENABLED
        value: "false" # optional integrations must not block boot

  - type: worker
    name: meno-worker
    runtime: docker
    dockerfilePath: ./ops/Dockerfile
    dockerContext: .
    plan: starter
    dockerCommand: meno-worker # the ONLY difference from the web service
    envVars:
      # Must be identical to the web service's, or the two talk to different databases.
      - key: DATABASE_URL
        fromDatabase: { name: meno-db, property: connectionString }
      - key: REDIS_URL
        fromService: { name: meno-kv, property: connectionString }
      - key: SKIP_MIGRATIONS
        value: "true" # the web service owns migrations; avoids a race

databases:
  - name: meno-db
    databaseName: meno
    plan: free

# Render Key Value free — 25 MB, Valkey 8, NO persistence (§3.2). The 25 MB ceiling
# is why OTP state must move to Neon and why fan-out must be batched.
```

This blueprint deliberately stops short of provisioning services: R2 needs a custom
domain (§14 question 1) and Neon needs a pooled URL (§3.5). Those are Phase 6 decisions,
not layout decisions.

**Commit:**

```
build: replace the dockerfile and add a Render blueprint

The Dockerfile failed outright: SQLX_OFFLINE=true with the stale .sqlx made
cargo build error at the three total_participants sites. Now builds with --locked,
both binaries (the worker is a separate service from the same image), BuildKit-
friendly layering, a non-root user, and a real HEALTHCHECK that Render can poll.
It also copies crates/, which master's did not, so it breaks as soon as a
workspace crate exists. render.yaml declares api + worker as two services with
identical config apart from the command and the migration-skip flag.
```

---

## Step 14 — Makefile, scripts, CI

**Why.** §8 lists CI as a **P0 gap**. Nothing ran on PRs, which is how a stale `.sqlx`
cache producing 43 compile errors sat in `main` unnoticed. Every commit in this guide
should have been gated by that one command.

### Step 14.1 — The Makefile

```makefile
# Single entry point for every Meno task. `make help` lists everything.
SHELL := /bin/bash
.DEFAULT_GOAL := help

DATABASE_URL ?= postgres://meno:password@localhost:5432/meno_dev
REDIS_URL    ?= redis://localhost:6379
export DATABASE_URL REDIS_URL

.PHONY: help
help: ## Show this help
	@grep -hE '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) \
	  | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2}'

# ── lifecycle ────────────────────────────────────────────────────────────────
.PHONY: bootstrap
bootstrap: ## Install sqlx-cli and fetch dependencies
	rustup show
	cargo install sqlx-cli --no-default-features --features rustls,postgres --locked || true
	cargo fetch

.PHONY: db-up
db-up: ## Start Postgres, Redis and RustFS, and wait until they are healthy
	@docker info >/dev/null 2>&1 || { \
	  echo "ERROR: Docker daemon not responding. Start Docker Desktop (guide Step 1.1)." >&2; \
	  exit 1; }
	docker compose -f ops/docker-compose.yml up -d postgres redis storage
	@echo "waiting for healthy…"
	@until [ "$$(docker compose -f ops/docker-compose.yml ps --format json postgres \
	   | grep -c healthy)" = "1" ]; do sleep 1; done
	@docker compose -f ops/docker-compose.yml run --rm storage-init >/dev/null
	@echo "postgres, redis and storage are ready."

.PHONY: db-down
db-down: ## Stop the dev stack, keep volumes
	docker compose -f ops/docker-compose.yml down

.PHONY: stack-up
stack-up: ## Start the FULL stack including observability (no api service)
	docker compose -f ops/docker-compose.yml up -d

.PHONY: ps
ps: ## Show service status and health
	docker compose -f ops/docker-compose.yml ps

.PHONY: logs
logs: ## Tail all container logs
	docker compose -f ops/docker-compose.yml logs -f

.PHONY: db-reset
db-reset: ## Drop and recreate the dev database from zero
	docker compose -f ops/docker-compose.yml down -v
	$(MAKE) db-up
	sqlx database create || true
	sqlx migrate run

.PHONY: run
run: ## Run the API on :8080
	cargo run --bin meno-api

.PHONY: worker
worker: ## Run the background worker locally
	cargo run --bin meno-worker

.PHONY: migrate
migrate: ## Apply pending migrations
	sqlx migrate run

# ── quality gates (the same set CI runs) ─────────────────────────────────────
.PHONY: fmt
fmt: ## Format
	cargo fmt --all

.PHONY: fmt-check
fmt-check: ## Verify formatting
	cargo fmt --all -- --check

.PHONY: clippy
clippy: ## Lint
	cargo clippy --workspace --all-targets -- -D warnings

.PHONY: check-sqlx
check-sqlx: ## Verify .sqlx matches the schema — the rot detector
	cargo sqlx prepare --workspace -- --all-targets
	cargo sqlx prepare --workspace --check

.PHONY: prepare-sqlx
prepare-sqlx: ## Regenerate the offline cache after changing a query
	cargo sqlx prepare --workspace -- --all-targets

# Ratcheting threshold: lower as files are split, never raise. Target is 400.
MAX_FILE_LINES ?= 1605

.PHONY: file-size
file-size: ## Fail if any Rust file exceeds $(MAX_FILE_LINES) lines (target 400)
	@over=$$(find apps/api/src crates -name '*.rs' -exec wc -l {} + \
	         | awk -v m=$(MAX_FILE_LINES) '$$1 > m && $$2 != "total" {print $$1" "$$2}'); \
	 if [ -n "$$over" ]; then echo "Files over budget:"; echo "$$over"; exit 1; fi; \
	 echo "All Rust files within the line budget."

.PHONY: test
test: ## Run all tests
	cargo test --workspace --all-targets

.PHONY: deny
deny: ## Licence and advisory policy
	cargo deny check

.PHONY: verify
verify: fmt-check clippy check-sqlx file-size test ## Everything CI runs
```

> `file-size` is a crude heuristic and it will annoy people. It is also the only
> mechanical defence against the §5.1 failure mode recurring. **Twelve** files exceed 400
> lines on `master`, so a hard 400 gate would fail on day one and get deleted. Start at
> the current maximum and ratchet down; never let it go back up.

### Step 14.2 — Scripts and `deny.toml`

```bash
#!/usr/bin/env bash
# scripts/bootstrap.sh — one-shot setup on a clean machine.
#
# This is the single command referenced by the definition of done:
# "make bootstrap && make run works on a clean machine with no manual steps."
set -euo pipefail

echo "==> Checking the Docker daemon"
# Fails fast and legibly, because "failed to connect to the docker API" is the single
# most common first-run failure and the raw message does not say what to do.
if ! docker info >/dev/null 2>&1; then
  echo "ERROR: the Docker daemon is not responding." >&2
  echo "       Start Docker Desktop and re-run. See Step 1.1 of the guide." >&2
  exit 1
fi

echo "==> Checking the toolchain"
rustup show

echo "==> Installing sqlx-cli"
cargo install sqlx-cli --no-default-features --features rustls,postgres --locked || true

echo "==> Fetching dependencies"
cargo fetch

if [ ! -f .env ]; then
  cp .env.example .env
  echo "==> Created .env — fill in the two JWT secrets before starting the API:"
  echo "      openssl rand -hex 64   (run twice; they must differ)"
fi

echo "==> Starting infrastructure"
make db-up

echo "==> Creating the storage bucket"
docker compose -f ops/docker-compose.yml run --rm storage-init >/dev/null

echo "==> Applying migrations"
sqlx database create || true
sqlx migrate run
sqlx migrate info

echo "==> Done. Run: make run"
```

```bash
#!/usr/bin/env bash
# scripts/sqlx-prepare.sh — regenerate the offline cache against a migrated database.
# Run after ANY change to a query! macro. CI runs the --check form.
set -euo pipefail
: "${DATABASE_URL:?DATABASE_URL must be set and point at a migrated database}"
unset SQLX_OFFLINE || true
cargo sqlx prepare --workspace -- --all-targets
echo "==> .sqlx regenerated — commit it."
```

```toml
# deny.toml — cargo-deny: licences and advisories.
[advisories]
version = 2
yanked = "deny"

[licenses]
version = 2
allow = ["MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception", "BSD-3-Clause", "ISC", "Unicode-3.0", "Zlib", "MPL-2.0"]

[bans]
multiple-versions = "warn"   # not "deny": your graph legitimately has two of time/hashbrown
wildcards = "deny"
```

### Step 14.3 — CI

```yaml
name: CI
on:
  push: { branches: [main, "refactor/**"] }
  pull_request:

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

jobs:
  quality:
    name: fmt, clippy, sqlx cache, tests
    runs-on: ubuntu-latest
    services:
      postgres:
        image: postgres:18
        env:
          POSTGRES_USER: meno
          POSTGRES_PASSWORD: password
          POSTGRES_DB: meno_test
        ports: ["5432:5432"]
        options: >-
          --health-cmd "pg_isready -U meno -d meno_test"
          --health-interval 5s --health-timeout 5s --health-retries 10
      redis:
        image: redis:8-alpine
        ports: ["6379:6379"]
        options: >-
          --health-cmd "redis-cli ping"
          --health-interval 5s --health-timeout 5s --health-retries 10

    env:
      DATABASE_URL: postgres://meno:password@localhost:5432/meno_test
      REDIS_URL: redis://localhost:6379

    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@1.88
        with: { components: rustfmt, clippy }
      - uses: Swatinem/rust-cache@v2
      - run: cargo install sqlx-cli --no-default-features --features rustls,postgres --locked
      - run: sqlx migrate run
      - run: cargo fmt --all -- --check

      # The gate that would have caught master's 43-error incident on day one.
      - name: Verify the offline sqlx cache matches the schema
        run: |
          cargo sqlx prepare --workspace -- --all-targets
          cargo sqlx prepare --workspace --check

      - run: cargo clippy --workspace --all-targets -- -D warnings

      - run: cargo test --workspace --all-targets

  deny:
    name: licences and advisories
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@1.88
      - run: cargo install cargo-deny --locked
      - run: cargo deny check

  docker:
    name: image builds both binaries
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: docker/setup-buildx-action@v3
      - uses: docker/build-push-action@v6
        with:
          context: .
          file: ops/Dockerfile
          push: false
          cache-from: type=gha
          cache-to: type=gha,mode=max
```

> **The trigger test from Step 11.2 is red, so CI is red.** Mark it
> `continue-on-error: true` with a comment naming the tracked issue, and remove the
> override in the phase that fixes the triggers. A permanently red CI trains people to
> ignore CI, which is worse than no CI at all.

`cargo sqlx prepare --workspace --check` is the most valuable line: a **two-second test
that the committed offline cache still matches the committed schema** — the exact failure
that made `master` unbuildable.

---

## Step 15 — `apps/web` and `apps/mobile`

**Why.** §2 shows both in the tree, and they are **not Cargo workspace members** —
different toolchains, different lockfiles, different release cycles. Listing them in
`members` would make `cargo` parse their manifests and break the moment either needs a
conflicting feature set.

Per §14 question 3, these are placeholders only. Do not scaffold:

```bash
mkdir -p apps/web apps/mobile
```

```markdown
<!-- apps/web/README.md -->

# Meno Web (Next.js)

Placeholder. Not yet scaffolded.

**Not a Cargo workspace member** — it has its own toolchain and lockfile, and must
not appear in the root `Cargo.toml` `members` array.

Status: §14 question 3 (is web in scope?) decides whether this becomes a Next.js
app at all.

When it is built:

- Generate the API client from `apps/api/openapi.yaml`. Never hand-write types.
- Branch on the stable `code` from `crates/core::Error`, never on `message` text —
  messages are rewordable, codes are a versioned contract.
```

```markdown
<!-- apps/mobile/README.md -->

# Meno Mobile (Flutter)

Placeholder. Not yet scaffolded.

**Not a Cargo workspace member.**

Status: §14 question 3 decides scope.

When it is built:

- Generate the API client from `apps/api/openapi.yaml`.
- Branch on `code`, never on `message`.
- Device sessions (§4.7) are required for this client — mobile is why
  `auth_sessions` exists.
```

And fix the `.gitignore`, which on `master` points at `mobile/` at the repo root:

```gitignore
# --- apps ---  (master's entries target mobile/ at the repo root, which will not
#     match apps/mobile/ once these directories exist)
apps/web/node_modules/
apps/web/.next/
apps/web/out/
apps/mobile/.dart_tool/
apps/mobile/build/
apps/mobile/.flutter-plugins
apps/mobile/.flutter-plugins-dependencies

# Rust — anchored, so this cannot also ignore a nested target/ by accident
/target
```

> Bare patterns like `node_modules/` match at any depth so those still work, but
> `mobile/build/` will **not** match `apps/mobile/build/`. Fix it now rather than after
> the first accidental 40 MB commit.

---

# Verification

## The whole guide, end to end

```bash
# 0. Infrastructure is up and healthy (Step 1)
docker info >/dev/null && echo "daemon ✅"
docker compose -f ops/docker-compose.yml ps --format '{{.Service}} {{.Health}}'
# expect: postgres healthy, redis healthy, storage healthy

# 1. One project, one lockfile, three crates
cargo metadata --format-version 1 --no-deps | jq '.workspace_members | length'   # 3
cargo tree -d                              # duplicates, each explainable

# 2. Everything compiles
cargo check --workspace --all-targets

# 3. The pure crate is genuinely pure
grep -rE '\b(sqlx|axum|fred|tokio)::' crates/core/src/ && echo "LEAK" || echo "core is pure ✅"

# 4. The offline cache matches the schema
cargo sqlx prepare --workspace --check

# 5. The pure tests pass with NO infrastructure running
docker compose -f ops/docker-compose.yml stop
cargo test -p meno-core
# must pass with Postgres and Redis down. If not, something leaked in.
docker compose -f ops/docker-compose.yml up -d postgres redis storage

# 6. Both binaries exist; the web one cannot run jobs
cargo build --bin meno-api --bin meno-worker
grep -n 'run_jobs' apps/api/src/main.rs    # expect: only a comment, no call

# 7. No file over the budget
make file-size

# 8. The nine error enums are gone
grep -rn 'MenoError\|BroadcastError\|AuthError' apps/api/src/ && echo "present" || echo "consolidated ✅"

# 9. shared/ no longer exists
ls apps/api/src/shared 2>/dev/null && echo "still there" || echo "restructured ✅"

# 10. Both binaries boot against one schema
make run &
curl -s localhost:8080/health | jq '{status, db, redis}'
make worker &     # then confirm no duplicate execution
```

## Definition of done

- [ ] `docker info` succeeds and postgres, redis, storage all report `healthy`
- [ ] One `Cargo.lock` for three crates; `apps/web`/`apps/mobile` are not members
- [ ] `cargo check --workspace --all-targets` → 0 errors
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` → 0 warnings
- [ ] `cargo test -p meno-core` passes with Docker stopped
- [ ] `cargo sqlx prepare --workspace --check` passes
- [ ] `crates/core` has no `sqlx`/`axum`/`fred`/`tokio` reference
- [ ] Migrations live in `crates/db/migrations` and run automatically at boot
- [ ] Two binaries from one crate; only the worker starts the Apalis monitor
- [ ] `shared/` gone; `infrastructure/` and `middleware/` separate
- [ ] The four adapter traits exist, each with a double
- [ ] Nine error enums retired; every error response carries a stable `code`
- [ ] No Rust file over the line budget
- [ ] `make bootstrap && make run` works on a clean machine
- [ ] `GET /health` → `{"status":"ok","db":true,"redis":true}`

## The diff against the reference

The single most useful review tool, and only available because `master` is intact:

```bash
# Everything that changed, by size of change.
git diff master --stat

# Files whose logic changed at all — this should be a SHORT list.
git diff master --numstat | awk '$1+$2 > 0 {print}'
```

That second command is your audit. Everything not on its output was copied byte-for-byte.
If a file you expected to change is not on the list, or vice versa, you now know before
anyone reviews it.

---

## Rollback

Better than the in-place version, because `master` is intact:

```bash
# Restore any file from the reference, no commit needed.
git checkout master -- apps/api/src/modules/broadcast/service.rs

# Throw away everything since the port checkpoint.
git reset --hard <port-commit-sha>

# Nuclear: start the branch again from the port checkpoint.
git reset --hard <port-commit-sha> && git clean -fd
```

The port commit from Step 5.5 is the fallback: at that point the API compiles, boots and
is byte-for-byte the old behaviour in the new layout. If Phase 2 becomes unworkable,
stop there — you still have a better codebase than `master`.

### The steps that can actually hurt

| Step                         | Risk                                                      | Mitigation                                                                               |
| ---------------------------- | --------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| **2** (the sed)              | A path form is missed                                     | `grep -rn 'shared::' apps/api/src` must return nothing before committing                 |
| **5** (the port)             | An edit slipped in among the copies                       | `git diff master --numstat` — anything not on the list must be byte-identical            |
| **9** (service split)        | Behaviour regression in live audio                        | Extract bottom-up, compiling after each pass. Phase 1's checkpoint makes this bisectable |
| **10** (error consolidation) | A client-visible message changes and Flutter breaks on it | One module per commit. `git show <sha> \| grep '^+.*message'` and read every string      |

### Temporary escape hatches

All four are annotated and all four have a removal step:

| Hatch                                         | Where                    | Removed                           |
| --------------------------------------------- | ------------------------ | --------------------------------- |
| `unwrap_used`/`expect_used`/`panic` = `allow` | `apps/api/Cargo.toml`    | Step 9.5                          |
| `SKIP_MIGRATIONS`                             | `render.yaml` worker env | Phase 6, once deploys are stable  |
| `MAX_FILE_LINES = 1605`                       | `Makefile`               | as files are split                |
| `continue-on-error` on the trigger test       | `ci.yml`                 | the phase that fixes the triggers |

None may outlive its step. A temporary escape hatch that survives becomes permanent,
undocumented, load-bearing behaviour — which is exactly how the dead rate limiter and
idempotency middleware got there in the first place.

---

## What is deliberately carried over as broken

You chose to fix compile, tests and structure only. These are **known** — each was
verified against the source or the migrated database during this guide, not assumed —
and each needs a follow-up phase.

| Defect                                                    | Evidence                                                                                                                     | Why deferred                                                        |
| --------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| Dead `'TG_OP'` triggers in `0013`/`0014`                  | Verified in Step 1.12 against a live database: `users.followers`/`following`/`broadcasts` have never updated                 | Needs a new migration; Step 11.2 leaves a red test naming the cause |
| `total_participants` drifts                               | `COUNT(*)` with no `left_at IS NULL`, while departures are `UPDATE … SET left_at`                                            | Needs a semantic decision plus migration `0020`                     |
| Rate limiting is dead                                     | `with_rate_limit(25,60)` returns an `Extension` nothing reads; `maybe_custom` commented out; `Err(_) => next.run` fails open | Needs real middleware wiring and a policy decision per route class  |
| Idempotency is dead _and_ unsafe                          | Reads an `Arc<Redis>` nothing inserts; key is global, not per-user; no in-flight lock; loses headers on replay               | Needs design (per-user scoping, `SET NX` lock)                      |
| `BroadcastScheduledFanOutJob` never enqueued              | `service.rs:175` is a `// TODO`; the job is fully implemented                                                                | Needs `ScheduledAt` semantics                                       |
| Cleanup loops are per-process `tokio::time::interval`     | Duplicated `shutdown_signal` fixed, intervals not                                                                            | Needs Apalis scheduler migration                                    |
| `reqwest`/`oauth2` `blocking` features enabled but unused | Verified in Step 5.2: zero source references                                                                                 | **Done** — manifest-only change                                     |
| Google OAuth links by email without `email_verified`      | `upsert_google_user`                                                                                                         | Security fix; needs its own review                                  |
| Argon2 params hardcoded `Params::new(19456, 2, 1)`        | `auth/password.rs`                                                                                                           | Needs config plus re-hash-on-login                                  |
| OTP state in a non-persistent Redis                       | §3.2; Render KV free has no persistence                                                                                      | **Critical** — a restart locks every in-flight password reset       |
| `BREVO`/lettre SMTP                                       | `infrastructure/mail/` still lettre                                                                                          | Migration, not a port concern                                       |

## Open questions still unanswered

The six §14 questions stand. Two affect decisions already made above:

1. **Domain for `cdn.<domain>`?** Determines whether `ObjectStore::public_url` is ever
   `Some`. R2 buckets are private by default; without a custom domain you need presigned
   URLs, which you said you wanted to avoid.
2. **Are `apps/web` and `apps/mobile` in scope?** Step 15 writes placeholders on the
   assumption they are deferred.

Also relevant: Render Free vs $7/mo Starter ×2 (§3.1 — Free spins down and drops
WebSockets mid-broadcast), and whether 25 MB of Redis is enough long-term.

**Step 0 note:** this guide was written against reference commit `903c3ba`. Before
porting, confirm `git log -1 --oneline master` still matches, and recompute the line
numbers in Step 9.2 rather than trusting a document.
