# Meno Monorepo — API Refactor & Rebuild Plan

**Status:** Draft for review
**Author:** David Michael (code review of `meno-api` @ `903c3ba`)
**Scope:** Restructuring the existing Rust/Axum API into a monorepo layout, fixing all
defects found in review, and deploying to Render + Neon + Cloudflare R2 + Brevo.

---

## 1. Executive summary

The existing `meno-api` is **~19.9k lines** of Rust with genuinely good architecture:
a consistent module-per-domain layout, repository traits enabling test doubles, a
well-designed opaque cursor pagination system, and security instincts that are well ahead
of the project's maturity (Argon2id off-thread, hashed refresh tokens, PKCE + consumed
OAuth state, constant-time-ish login).

However the codebase **does not compile** (43 `cargo check` errors), **cannot run**
(43 errors, no migration runner, missing env), and has **several silently dead
subsystems** (rate limiting, idempotency) plus **three dead PostgreSQL triggers**.

This plan does **not** rewrite from scratch. It preserves the proven architecture and
domain logic while fixing every defect, adding the missing connective tissue
(migrations, sqlx cache, env, CI, deploy), and restructuring into a monorepo.

**Guiding principle:** the architecture is sound; the gap is between the code and the
outside world. Every task below is mechanical unless explicitly marked `DESIGN`.

---

## 2. Target monorepo layout

```
Meno/
├── Cargo.toml                  # workspace root — owns the Rust lockfile
├── Cargo.lock
├── rust-toolchain.toml         # pins the toolchain (currently unpinned!)
├── rustfmt.toml
├── clippy.toml
├── deny.toml                   # cargo-deny: license + advisory policy
├── Makefile                    # or justfile — single entry point for all tasks
├── README.md
├── .env.example
├── .github/
│   └── workflows/
│       ├── ci.yml              # fmt, clippy, sqlx --check, build, test
│       └── deploy.yml          # Render deploy hooks (optional)
│
├── crates/
│   ├── db/                     # migrations ONLY. No Rust logic.
│   │   ├── migrations/         # the 18 existing migrations, corrected
│   │   └── README.md
│   └── core/                   # shared, dependency-free domain types
│       ├── src/lib.rs
│       ├── src/error.rs        # the single canonical error taxonomy
│       ├── src/pagination.rs   # Cursor / CursorPage / Order (moved from shared/)
│       ├── src/ids.rs
│       └── src/time.rs
│
├── apps/
│   ├── api/                    # THE RUST API (single binary source, two bins)
│   │   ├── Cargo.toml
│   │   ├── migrations -> ../../crates/db/migrations   (symlink or path dep)
│   │   ├── openapi.yaml        # generated, committed
│   │   ├── Dockerfile
│   │   ├── render.yaml
│   │   └── src/
│   │       ├── main.rs         # web binary
│   │       ├── worker.rs       # background-job binary   <-- NEW (see §6)
│   │       ├── lib.rs
│   │       ├── bootstrap.rs    # shared startup wiring used by both bins
│   │       ├── config.rs
│   │       ├── telemetry.rs
│   │       ├── state.rs
│   │       ├── error.rs
│   │       ├── routes/
│   │       ├── middleware/
│   │       ├── modules/
│   │       │   ├── auth/
│   │       │   ├── profile/
│   │       │   ├── broadcast/
│   │       │   ├── subscribers/
│   │       │   ├── notifications/
│   │       │   ├── chat/
│   │       │   ├── notes/
│   │       │   └── settings/
│   │       ├── jobs/
│   │       ├── infrastructure/  # clients: db, redis, storage, email, livekit, push
│   │       └── ...
│   │   └── tests/              # integration tests (see §10)
│   │
│   ├── web/                    # Next.js (separate toolchain, NOT a cargo member)
│   └── mobile/                 # Flutter   (separate toolchain, NOT a cargo member)
│
├── ops/
│   ├── docker-compose.yml      # dev stack: postgres, redis, minio, mailpit
│   ├── render.yaml             # Render Blueprint (api + worker + kv)
│   ├── prometheus/prometheus.yml
│   ├── grafana/provisioning/   # datasources + dashboards (auto-loaded)
│   ├── promtail/promtail.yml
│   └── grafana/dashboards/
│
└── scripts/
    ├── bootstrap.sh
    └── sqlx-prepare.sh
```

### 2.1 Why a Cargo workspace with `crates/` (the chosen option)

**Decision rationale —** the repo will also contain a Flutter app and a Next.js app.
Neither participates in the Cargo workspace (different toolchains, different lockfiles).
The workspace exists for the **Rust side**, and earns its place by giving us:

1. **One `Cargo.lock`** — `cargo build` and dependency versions are unified and auditable.
2. **A migrations-only crate** — `crates/db` has no code, just `migrations/`. It cannot
   acquire business logic, so schema stays separable from the API. This is how we get a
   single source of truth for the schema that both the API and CI reference.
3. **`crates/core` for pure, dependency-free types** — `Cursor`, `CursorPage`, `Order`, and
   the canonical error taxonomy. Pure Rust, no `sqlx`, no `axum`, no I/O. Testable in
   milliseconds with zero infrastructure. This is the single highest-leverage SOLID win
   in the whole refactor (see §5.1).
4. **It scales without cost** — when a second Rust service appears (e.g. a media
   transcoder), it becomes `members = ["apps/api", "apps/worker-future", ...]` and reuses
   `crates/core` for free.

**Anti-pattern to avoid:** do _not_ put `packages/db/migrations` style shared _business_
logic in `crates/`. A crate that is shared by construction becomes shared by accident.
`crates/core` stays pure; everything impure stays in `apps/api`.

### 2.2 Workspace `Cargo.toml`

```toml
[workspace]
resolver = "3"                      # edition 2024 → resolver 3
members = ["apps/api", "crates/core", "crates/db"]

[workspace.package]
version = "0.2.0"
edition = "2024"
rust-version = "1.88"
license = "MIT"

[workspace.dependencies]
# Every third-party crate is declared ONCE here, with features pinned.
sqlx = { version = "0.8", features = ["postgres", "runtime-tokio", "uuid", "time", "macros", "migrate"] }
axum = { version = "0.8", features = ["ws", "macros", "multipart"] }
tokio = { version = "1", features = ["full"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
fred = { version = "10", features = ["tokio-rustls", "serde-json", "i-scripts", "i-pubsub"] }
time = { version = "0.3", features = ["serde", "serde-well-known"] }
uuid = { version = "1", features = ["v4", "serde"] }
thiserror = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
validator = { version = "0.20", features = ["derive"] }
apalis = { version = "1.0.0-rc", features = ["limit", "prometheus"] }
apalis-postgres = "1.0.0-rc"
# ... (full list in the real file)

[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
strip = true
panic = "abort"    # <-- NEW. See §9.4
```

> **`panic = "abort"`** — the API panicking on a bad request is not recoverable, and
> `axum` will drop the connection anyway. Aborting is cheaper and fail-fast. _Caveat:_
> `panic = "abort` conflicts with `catch_unwind` in integration tests; if we need that,
> set it per-profile (`[profile.release]` only, which is what is specified here) so
> `cargo test` still works.

---

## 3. Hosting & infrastructure stack

| Concern                 | Provider                    | Plan                                | Notes                                 |
| ----------------------- | --------------------------- | ----------------------------------- | ------------------------------------- |
| **API (web)**           | Render                      | Starter ($7/mo) — free while deving | Rust native build, long `cargo build` |
| **Worker**              | Render                      | Starter ($7/mo)                     | Separate service, runs Apalis         |
| **Postgres**            | **Neon**                    | Free tier                           | Pooled connection string              |
| **Redis**               | **Render Key Value**        | **Free (25 MB)**                    | Valkey 8; see §3.2                    |
| **Object storage**      | **Cloudflare R2**           | Free tier                           | 10 GB/mo; see §3.3                    |
| **Transactional email** | **Brevo**                   | Free tier                           | 300 emails/day                        |
| **Push notifications**  | FCM                         | Free                                | Make optional — see §4.6              |
| **Live audio/video**    | LiveKit Cloud               | Free tier                           | Already integrated                    |
| **Metrics/logs**        | Render + Grafana Cloud free | Free                                | Self-host in dev                      |

### 3.1 Render — the cold-start and WebSocket reality

Three things about Render that shape the design:

1. **Free web services spin down after 15 minutes of inactivity** and have no uptime
   guarantee. A free tier is fine for development; it will drop every WebSocket
   connection and produce cold starts. Plan for Starter ($7/mo) before real users.
2. **WebSockets work on Render**, but each replica holds its own in-process connection
   pool. This is _precisely_ why the Apalis scheduler must not live in the web process
   (§6) — two replicas would each run every job twice.
3. **Rust builds are slow** (first build ~6–10 min). Mitigations: Cargo layer caching
   via BuildKit `--mount=type=cache`, prebuilt release images, or a `Dockerfile` with
   dependency pre-warming (which the current one attempts — see §3.4).

### 3.2 Redis — Render Key Value (free) with honest caveats

Verified: Render Key Value **does have a free compute plan** (25 MB), so it satisfies
the "recommended if free" constraint. Important details:

- New instances run **Valkey 8** (Redis 7.2 fork). `fred` speaks RESP, so it is a
  drop-in — but do not rely on Redis-6-only commands.
- **No persistence on the free plan.** Data loss on restart is expected and must be
  survivable. This drives the rule below.
- **Connection limits are low on free.** Configure `fred` with a small pool
  (`pool_min = 2`, `pool_max = 8`), not the defaults.
- Use the **internal URL** (`redis://`) from same-region Render services; it is
  unauthenticated over Render's private network and avoids a latency hop. External
  access must be explicitly enabled _and_ IP-allowlisted.

> ### ⚠ Design constraint: Redis is 100% cache and ephemeral
>
> Every key must be reconstructible from Postgres or recomputable. That means:
>
> - **Token blocklists** — a lost blocklist means revoked access tokens stay valid until
>   their 15-minute TTL expires. Acceptable **only** because access tokens are short-lived.
>   This is now a _documented security assumption_, not an accident.
> - **Rate limiting** — a lost counter means a burst is briefly allowed. Acceptable.
> - **OTPs** — **NOT acceptable.** OTP state must live in **Neon**, not Redis, or an
>   unlucky restart locks every in-flight password reset. _(This is a change from the
>   current implementation.)_
> - **Live participant counts** — recomputable from `broadcast_participants`.
> - **WS pub/sub** — pub/sub is already ephemeral by nature.

**Eviction policy:** create the instance with `maxmemory-policy = allkeys-lru`.
Without it, a full 25 MB instance returns write errors and takes down auth.
Set **mandatory TTLs on every key** — this is enforced in code review and by a
`RedisKey` type whose constructors all require an expiry.

**Free-tier alternative if we leave Render:** Upstash Redis free (256 MB, 500k
commands/month, TLS-only endpoint). More memory, but a per-command bill that this
app's WS fan-out will exhaust quickly. Same `REDIS_URL` seam, so swapping is a
one-line config change.

### 3.3 Cloudflare R2 — and the "no pre-signing" constraint

R2 buckets are **private by default**. Objects can only be served publicly via:

1. Presigned URLs, **or**
2. A **custom domain** bound to the bucket (e.g. `cdn.meno.app`), **or**
3. The `r2.dev` dev subdomain — explicitly documented as _not for production_
   (unstable, rate-limited, no caching guarantees).

Since you want **no pre-signing**, the plan is:

- **Public assets** (avatars, broadcast images, note attachments) →
  bind a **custom domain** to the R2 bucket. Public read, no signing required.
  This is the standard, fast, CDN-backed path and it is what we'll do.
- **Private assets** (anything user-private, e.g. a future "private recording") →
  these _cannot_ avoid pre-signing. Plan: **proxy through the API** with an
  authorisation check, streaming the object. Slower and costs egress on the API,
  but correct. We will not build this until a private-asset feature actually exists.

> **Action required from you:** register a domain and point a subdomain
> (e.g. `cdn.meno.app`) at the R2 bucket. Until that exists, the app can run in dev
> against MinIO, and R2 uses presigned URLs behind a trait so the swap is one
> implementation of one trait.

**Abstraction:** `ObjectStore` trait with two implementations — `R2Store` and
`MinioStore` (dev/docker) — so storage is swappable and testable. The current code
couples to `object_store` directly; that becomes a `DESIGN` task (§5.6).

### 3.4 Docker / build fixes required

The current [dockerfile](dockerfile) is **build-hostile and will fail**, because it
bakes `SQLX_OFFLINE=true` with a **stale** `.sqlx` cache (§7.1). Required changes:

- Use BuildKit cache mounts so the dependency layer survives:
  ```dockerfile
  RUN --mount=type=cache,target=/usr/local/cargo/registry \
      --mount=type=cache,target=/app/target \
      cargo build --release --locked
  ```
- **`--locked`** in every cargo invocation — fails loudly if `Cargo.lock` drifts.
- Reproducible base images pinned by digest (`rust:1.88-slim-bookworm@sha256:…`).
- A **non-root** runtime user; the current image runs as root.
- Build **both** binaries (`meno-api`, `meno-worker`) from one image; Render picks
  which to start via `render.yaml`.
- Install a shell for a real healthcheck (`wget`/`curl`), or use a healthcheck that
  shells to `/dev/tcp`.

### 3.5 Neon specifics

- Use the **pooled** connection string (`…-pooler.<region>.neon.tech`) from Render.
  This is PgBouncer in transaction mode — **long-lived or interactive transactions
  are not safe** there.
- Consequence: the pool config in [database.rs](apps/api/src/database.rs)
  (`max_connections: 20`, `max_lifetime: 30 min`) is wrong for Neon. With PgBouncer,
  use `max_connections: 5–10` and a _short_ `max_lifetime` (~5 min) so the pooler can
  recycle. Set from env, not hardcoded.
- Neon gives you **branching** — use a branch per PR for migration testing (§11).
- `CREATE INDEX CONCURRENTLY` is supported, but must run **outside a transaction**,
  so migrations that build indexes need the `-- no-transaction` sqlx flag.

### 3.6 Brevo

- Brevo replaces the direct SMTP dependency. `lettre` + `AsyncSmtpTransport` is
  removed; we call Brevo's HTTPS API over `reqwest`. This removes an entire class of
  deliverability/connection-pool concerns and lets us send **transactional templates**
  instead of hand-built HTML strings (current `email_jobs.rs` inlines raw HTML).
- **Free tier: 300 emails/day.** Verification and password-reset emails are the
  dominant cost. Mitigations: mandatory rate limiting on OTP resend (already
  designed, must now actually work), and Brevo's suppression list.

---

## 4. Architecture decisions

### 4.1 Module layering (keep it — it is the best thing in the codebase)

Retain the per-domain layout, with one refinement: repositories currently use a
_generic_ service (`BroadcastService<R: BroadcastRepo>`) plus a **builder with eight
`expect()` calls**. Replace with **explicit trait objects** for consistency with the
auth module:

```rust
pub struct BroadcastService {
    repo: Arc<dyn BroadcastRepo>,
    cache: Arc<dyn BroadcastCache>,
    // ...
}
impl BroadcastService {
    pub fn new(deps: BroadcastDeps) -> Self { /* no Option, no expect, no panic */ }
}
```

This deletes the builder, removes eight panic paths, and makes the dependency list
visible at the call site. Compile-time errors instead of runtime panics.

### 4.2 The error taxonomy — one canonical enum

Today there are **nine** independent error enums (`MenoError`, `AuthError`,
`BroadcastError`, `ChatError`, `NotesError`, `NotificationError`, `ProfileError`,
`SettingsError`, `SubscriberError`) and several inconsistent implementations
(`notifications/error.rs` is singular, everything else is plural). Result: error
mapping logic is duplicated nine times and drifts.

**Plan:** a single `crates/core::Error` enum implementing `thiserror` + `IntoResponse`,
with **domain-specific detail carried in variants**, not in nine parallel enums:

```rust
pub enum Error {
    BadRequest { code: &'static str, message: String },
    Unauthorized { code: &'static str },
    Forbidden,
    NotFound { resource: &'static str },
    Conflict { code: &'static str },
    Validation { fields: HashMap<String, Vec<String>> },
    RateLimited { retry_after_secs: u64 },
    // Infrastructure — always logged, never leaked to the client:
    Database(sqlx::Error),
    Redis(fred::error::Error),
    Storage(String),
    Mail(String),
    Upstream { service: &'static str, source: String },
    Internal(anyhow::Error),
}
```

Every response carries a **stable machine-readable `code`** so the Flutter and Next.js
clients never string-match on human-readable messages. This is a prerequisite for
both frontends — do it early.

### 4.3 Make migrations run (§7.2) — `DESIGN`

Use embedded, versioned migrations via `sqlx::migrate!()`:

```rust
sqlx::migrate!("../../crates/db/migrations").run(&pool).await?;
```

- Runs at startup, tracked in `_sqlx_migrations`, idempotent.
- **Refuse to serve traffic until migrations succeed** — but _log clearly and exit_,
  never `panic!`.
- Add a `SKIP_MIGRATIONS=true` escape hatch for the web binary if we ever want
  separate migration ownership from the worker.

### 4.4 Rate limiting — make it real (§7.3)

- Delete the `Extension<Option<RateLimitConfig>>` theatre. Apply
  `rate_limit_middleware` as an actual layer with **typed state** via
  `from_fn_with_state`.
- Fix the **fail-open** on Redis error → fail-**closed** on auth-adjacent routes
  (login, register, OTP), fail-open on public reads.
- Sliding-window Lua script stays (it's correct), but it must be **single-instance
  consistent** — it is, since it's in Redis, which is what makes Render horizontal
  scaling viable.
- **Tiered limits**: stricter on `/auth/*` (5/min) and `/broadcasts/*/join`
  (expensive: LiveKit token minting), looser on reads.
- Identify by authenticated user ID post-`auth_middleware`, falling back to IP.
  The current "last 16 chars of the token" proxy is fragile.
- Return `Retry-After` (already done) and standard `X-RateLimit-*`.

### 4.5 Idempotency — make it real and safe (§7.4)

Three separate bugs: unwired (§7.4), **globally scoped** (any user replaying a key gets
another user's cached response — a data-leak vector), and **no in-flight lock** (two
concurrent retries both execute the handler).

- Key the cache on `(user_id, route_pattern, idempotency_key)` — route pattern, not the
  concrete path, so `POST /notes/1` and `POST /notes/2` don't collide.
- Add a **`SET NX` in-flight lock** with short TTL: second concurrent request returns
  `409 Conflict` (or waits briefly), rather than duplicating the side effect.
- Buffer and store the response **with the `Content-Type` header**, and replay headers
  too — currently the cached replay loses content type.
- Add a **background sweeper** job for keys past TTL (cheap, bounded) or rely on
  Redis TTL alone.
- Requires `Idempotency-Key` on all mutating routes. **Document it in `openapi.yaml`.**

### 4.6 Make external integrations optional (`DESIGN`)

`Config::from_env` currently **hard-fails** if `FIREBASE_SERVICE_ACCOUNT_PATH` is
missing, and then `read_to_string`s it from disk. That single variable blocks local
boot even though it's only needed for push notifications.

Make every optional integration `Option<T>` with a feature flag:

- `PUSH_ENABLED=false` → `PushNotificationService` becomes a **no-op**; FCM config not read.
- `STORAGE_ENABLED`, `LIVEKIT_ENABLED` similarly.
- **Required** config only: `DATABASE_URL`, `REDIS_URL`, `JWT_SECRET`,
  `JWT_REFRESH_SECRET`, `CORS_ORIGINS`, `ENV`.

Also: replace `env::var` + `Context` chains with a **typed, layered config** using the
`config` crate — validate once, at the boundary, with a single aggregated error
listing _every_ missing variable (not just the first). Derive `Secret<T>` for secrets so
they can't be accidentally `Debug`-logged.

### 4.7 Auth — keep JWT + refresh rotation, add device sessions

Confirmed direction. Changes to the existing (already good) implementation:

1. **Device-bound refresh tokens.** New `auth_sessions` table:
   `(id, user_id, refresh_jti, device_label, user_agent, ip, created_at, last_used_at,
revoked_at, rotated_from)`. A refresh token is valid only for the session it was
   issued to.
2. **Reuse detection.** If a refresh token is presented whose `jti` has already been
   rotated, treat it as theft: revoke **every** session for that user and force
   re-login. This is the standard OAuth BCP behaviour and is a genuine upgrade.
3. **Revoke-by-device.** `POST /auth/sessions/{id}/revoke`, plus "log out all devices".
   Mobile apps need this.
4. **Separate secrets stay.** Access and refresh keep distinct secrets.
5. **Keep** the access-token blocklist, but note the Redis caveat (§3.2) — a restart
   loses it, bounded by the 15-minute access TTL.
6. **Blocklist on Redis is one round trip** — keep, but `try_join` the JTI and user
   lookups (already done) and add a **tiny negative cache**? No — leave as is.
7. **Google OAuth**: keep PKCE + single-use server-stored state. Add **account
   linking** guard: currently `upsert_google_user` links by email silently — verify the
   provider's `email_verified` before linking to an existing account, or you inherit
   an unverified-email takeover. _(This is a security bug worth flagging — §7.9.)_
8. **Rate-limit** all `/auth/*` endpoints; already partially designed, not wired.

### 4.8 Observability

- `/metrics` must be **private** (§7.5). Bind to an internal Render URL, or gate behind
  a static token, or scrape over Render's private network only.
- **Structured fields only** — never `format!` into log messages (current code does this
  in `broadcast/service.rs` delete/tracing calls).
- Add `tracing::instrument` to every service method with `field::Empty` for outcomes.
- **SLO metrics**: request latency histograms by route, error rate, WS connection
  count, job queue depth/oldest-job-age, DB pool saturation.
- Grafana **provisioning files** committed so dashboards load automatically (§7.8).
- Replace `prometheus.yml`'s hardcoded `192.168.111.244:8080` with `api:8080` (§7.7).

### 4.9 Performance & scalability budget

| Path                | Target                  | Technique                                                      |
| ------------------- | ----------------------- | -------------------------------------------------------------- |
| Broadcast list feed | < 200 ms p95            | Redis cache + coalescing (exists) + cursor pagination (exists) |
| Join broadcast      | < 400 ms p95            | Rate-limited; LiveKit token mint is the ceiling                |
| Chat history        | < 150 ms p95            | Cursor pagination; consider read-replicas if Neon plan allows  |
| Auth (login)        | < 500 ms                | Argon2id cost dominates; measure and tune params               |
| Fan-out (1k+ users) | async, off request path | Apalis job, paginated publisher, batching (§6)                 |

Concrete performance work:

- `tokio::join!` is already used well for independent queries — extend that discipline.
- **N+1 detection**: audit every `find_users_batch` call site; ensure batch fetching.
- Add `EXPLAIN (ANALYZE, BUFFERS)` to CI as a smoke test on key queries.
- Make Argon2 params **configurable via env** so cost can be tuned without a rebuild
  (§7.11), re-hashing on next successful login.
- Consider **HTTP/2 + connection pooling** tuning to Neon.

---

## 5. SOLID remediation map

The existing code is _reasonably_ SOLID. Here is where it leaks, and the fix.

### 5.1 Single Responsibility — the biggest violation

`BroadcastService` is **1,605 lines** and orchestrates: CRUD, LiveKit token minting,
Redis live-count bookkeeping, WS pub/sub, job enqueueing, cache invalidation, response
projection, and permission checks. That is at least six responsibilities.

**Split into collaborating services**, composed by the module's `state.rs`:

| Extracted service         | Responsibility                                                      |
| ------------------------- | ------------------------------------------------------------------- |
| `BroadcastCommandService` | create / update / delete / start / end (write paths, transactional) |
| `BroadcastQueryService`   | list / get / participants (read paths, cached)                      |
| `LiveSessionCoordinator`  | LiveKit room lifecycle + token minting + permission changes         |
| `PresenceService`         | Redis live-count, host-grace, started-at keys                       |
| `ResponseProjector`       | `Broadcast` + relations + ctx → `BroadcastResponse`                 |

**Critically:** this is the example to apply _everywhere_. `notes/service.rs` (641),
`auth/services.rs` (390) and `notifications/service.rs` (452) have the same shape. No
single file should exceed ~400 lines. That becomes an enforced lint (see §9.2).

### 5.2 Open/Closed — already good, keep it

The repository/cache traits (`BroadcastRepo`, `AuthRepo`, `AuthCache`, `BroadcastCache`)
and the `QueryBuilder` + `build_query_as` pattern for dynamic SQL are genuinely
extensible. Preserve. Add a `ObjectStore` trait (§3.3) and an `EmailSender` trait
(§3.6) — the same pattern applied to two new seams.

### 5.3 Liskov Substitution — sound

`BroadcastService<R: BroadcastRepo>` generic + trait-object mixin is fine. Moving to
pure `Arc<dyn ...>` (§4.1) keeps substitutability and removes the generic-noise.

### 5.4 Interface Segregation — good, keep

`IdentityReader` is a textbook example: a narrow read-only trait so `subscribers`
doesn't depend on the whole `AuthRepo`. **Apply this pattern more** — audit for any
service taking a fat trait it only partially uses.

### 5.5 Dependency Inversion — good, one leak

Everything depends on traits. **The leak:** `MenoError` (in `shared/`) is imported by
module `errors.rs` files, and `AuthError`/`BroadcastError` wrap `sqlx::Error` directly —
so domain errors are coupled to the database driver. Move the `sqlx::Error` →
`Error::Database` conversion to the repository boundary so domain code never sees
driver types.

### 5.6 Infrastructure as adapters (`DESIGN`)

`state.rs` currently does `Arc::new(RoomClient::with_api_key(...))` inline, and
`broadcast/service.rs` calls `.mint_token()` directly. Introduce adapter traits —
`ObjectStore`, `EmailSender`, `LiveKitAdapter`, `PushSender` — each with a real
implementation and an in-memory test double. This makes LiveKit/FCM/email logic fully
unit-testable without network, which is a prerequisite for the test plan in §10.

---

## 6. Background jobs — split into a dedicated worker binary

**Decision: separate Render service.** The current code spawns Apalis workers inside
the web process ([state.rs](apps/api/src/state.rs) `start_background_workers`). On
Render, scaling the web service to 2 replicas runs every job **twice** — duplicate
emails, duplicate fan-out, duplicate LiveKit room teardown.

**Split:**

```
apps/api/src/main.rs      -> binary "meno-api"    (HTTP only)
apps/api/src/worker.rs    -> binary "meno-worker"  (Apalis Monitor only)
apps/api/src/bootstrap.rs -> shared: config, telemetry, pool, redis, state
```

- Both call the same `bootstrap::build_context()` so config/DB/Redis wiring is identical.
- **`main.rs` MUST NOT start the Apalis Monitor.** Guard with an `AppRole` enum so it is
  impossible to accidentally re-enable: `AppRole::Web | AppRole::Worker`, set by binary.
- Render deploys the worker as its own service, same image, `render.yaml` selects the
  binary.

### 6.1 Job inventory (existing) and gaps

| Job                           | Trigger         | Notes                                                                                   |
| ----------------------------- | --------------- | --------------------------------------------------------------------------------------- |
| `SendEmailJob`                | on demand       | Migrate to **Brevo** API                                                                |
| `BroadcastStartedFanOutJob`   | broadcast start | ✅ wired                                                                                |
| `BroadcastScheduledFanOutJob` | **NEVER FIRED** | ❌ see below                                                                            |
| `EndBroadcastJob`             | broadcast end   | ✅                                                                                      |
| `CleanupExpiredTokensJob`     | hourly interval | ✅ — move interval to a scheduler that isn't a per-process `tokio::time::interval` loop |
| `PurgeStaleNotesJob`          | daily           | ✅ 30-day tombstone retention                                                           |

### 6.2 The missing job

[BroadcastService::create](apps/api/src/modules/broadcast/service.rs) contains:

```rust
if broadcast.start_time.is_some() {
    // TODO: schedule apalis BroadcastStartJob here
}
```

**`BroadcastScheduledFanOutJob` is fully implemented but can never run** — nothing ever
pushes it. Scheduled broadcasts therefore send no notification to subscribers. Fix:
push the job with the broadcast's `start_time` and let the worker **delay** the job
until due (Apalis `ScheduledAt`), so the worker process needs no timer of its own.

### 6.3 Scheduler vs. interval

Current `schedule_cleanup_job` / `schedule_notes_cleanup_job` spawn
`tokio::time::interval` loops **inside the process**. Move to Apalis `ScheduledAt`
delays so timing is durable across worker restarts and visible in the job table.

### 6.4 Fan-out at scale

`publish_to_users(&ids, payload)` over **every** subscriber is a single unbounded
operation. For a creator with 100k subscribers this is a memory spike and a long job.
**Plan:** batch (e.g. 500 ids/call) + `SELECT … LIMIT` chunked pagination + a single
"there are more broadcasts" coalesced event to save on payload size. Cache subscriber
count separately.

---

## 7. Defect register — every issue from the code review

This is the section that unblocks a running system. **P0 = blocks everything.**

### 7.1 🔴 P0 — `.sqlx` offline cache is stale (43 compile errors)

**Symptom:** ~40 × `SQLX_OFFLINE=true but there is no cached data for this query`, plus
3 × `error[E0063]: missing field total_participants in initializer of Broadcast`
([repository.rs:299](apps/api/src/modules/broadcast/repository.rs#L299),
[:345](apps/api/src/modules/broadcast/repository.rs#L345),
[:363](apps/api/src/modules/broadcast/repository.rs#L363)) — the `query_as!` macros read
a column list that predates the `total_participants` column added in commit `2fc1a02`
and in migration `0003`.

**Also breaks Docker:** [dockerfile](dockerfile) sets `SQLX_OFFLINE=true` + `COPY .sqlx`,
so `cargo build --release` fails. Not just a local issue.

**Fix:**

```bash
make db-up                     # start postgres, apply migrations
cargo sqlx prepare --workspace -- --all-targets
```

then **commit the regenerated `.sqlx/`** and add `cargo sqlx prepare --check` to CI so
this can never silently rot again.

### 7.2 🔴 P0 — No migration runner exists

Zero hits for `migrate!` / `sqlx::migrate` / `run_migrations` anywhere in `src/`.
`PostgresStorage::setup()` only creates **Apalis' own** tables. The 18 migrations in
`packages/db/migrations/` are **never applied** — a fresh Neon database has no schema.

**Fix:** `sqlx::migrate!()` at startup (§4.3). Verify by creating a brand-new Neon
branch and confirming the schema builds from zero.

### 7.3 🔴 P0 — Rate limiting is entirely dead

`rate_limit_middleware` ([rate_limit.rs:81](apps/api/src/shared/middleware/rate_limit.rs#L81))
is **never wired into any router**. `routes/mod.rs` calls `with_rate_limit(25, 60)`,
which only returns an `Extension(...)` that nothing reads. Additional defects inside it:

- `maybe_custom` is commented out → always uses `default_rate_limit` (60/60), silently
  ignoring the 25/60 that was asked for.
- `Err(_) => next.run(req)` **fails open** on Redis errors.
- `/health`, `/metrics`, `/ws` sit outside it entirely.

**Impact:** no rate limiting anywhere. Combined with the expensive `/broadcasts/{id}/join`
(LiveKit token minting) this is a straightforward abuse/DoS vector.
**Fix:** §4.4.

### 7.4 🔴 P0 — Idempotency middleware is dead + unsafe when fixed

[idempotency.rs:42](apps/api/src/shared/middleware/idempotency.rs#L42) does
`req.extensions().get::<Arc<Redis>>()`, but nothing ever inserts `Arc<Redis>` — only
`AuthUser` is inserted, by auth middleware. Every request takes the `None` branch and
passes through. All eight route modules apply a no-op.

When fixed, three latent bugs surface: **globally scoped** (one user's cached response
replayable by another — a data-leak vector), **no in-flight lock**, and **lost headers**
on replay.
**Fix:** §4.5.

### 7.5 🟠 P1 — Dead PostgreSQL triggers (`'TG_OP'` as a string literal)

[0013](packages/db/migrations/0013_create_user_follow_count_trigger.sql) and
[0014](packages/db/migrations/0014_create_user_broadcast_count_trigger.sql):

```sql
IF 'TG_OP' = 'INSERT' THEN   -- compares the literal text "TG_OP" to "INSERT" → always false
```

`'TG_OP'` is a **string literal**, not the PL/pgSQL variable. Both bodies are
unreachable. **`users.followers`, `users.following`, and `users.broadcasts` never
update.**

The giveaway: [0003](packages/db/migrations/0003_broadcasts_schema.sql) writes
`IF TG_OP = 'INSERT'` **correctly** for the same pattern — so this is a copy-paste slip
in two files, not a conceptual misunderstanding.

**Fix:** new migration `0019_fix_dead_triggers.sql` dropping and recreating both
functions unquoted. **Do not edit `0013`/`0014` in place** — they may have been applied
to a shared Neon database already, and editing applied migrations breaks checksum
verification.

### 7.6 🟠 P1 — `total_participants` drifts permanently

Trigger fires only on `INSERT`/`DELETE`, but the app never deletes on leave —
`get_participant_ids_and_clear` does `UPDATE … SET left_at = NOW()`.
Meanwhile `get_total_participants` does `COUNT(*)` with **no `left_at IS NULL` filter**,
so it counts people who already left.

You now have **two disagreeing definitions** of one metric; the denormalised column is
permanently wrong and feeds both a `ORDER BY` and a cursor.

**Fix (new migration `0020`):**

- Pick one definition. Recommend: **`total_participants` = peak/current joined**, i.e.
  rows where `left_at IS NULL`.
- Add trigger on `UPDATE OF left_at` to increment/decrement on transition.
- Make `get_total_participants` filter `left_at IS NULL` so both agree.
- Backfill existing rows to the corrected value.
- Consider dropping the denormalised column entirely if `COUNT(*)` on an indexed
  `(broadcast_id, left_at)` is fast enough — **one source of truth beats a cache that
  drifts.** Recommend measuring before deciding.

### 7.7 🟠 P1 — `update_broadcasts_count` has an unreachable branch

Registered `AFTER INSERT OR DELETE` but the body handles `'UPDATE'`. Un-deleting a
soft-deleted broadcast never decrements back. Fix alongside §7.5.

### 7.8 🟠 P1 — Env is incomplete and blocks startup

- **compose `api` service is missing ~15 required env vars.** It sets only `RUST_LOG`,
  `DATABASE_URL`, `FIREBASE_PROJECT_ID`, `FIREBASE_SERVICE_ACCOUNT_PATH`. Also required:
  `REDIS_URL`, `JWT_SECRET`, `JWT_REFRESH_SECRET`, 5 × `GOOGLE_*`, 4 × `SMTP_*`,
  6 × `STORAGE_*`, 3 × `LIVEKIT_*`. **The app panics on the first missing one.**
- **No `env_file:` directive**, no `.env` in repo.
- **`.env.example` has the wrong name:** it declares `FIREBASE_SERVICE_ACCOUNT_URL`, but
  `Config::from_env` reads `FIREBASE_SERVICE_ACCOUNT_PATH` and `read_to_string`s it as a
  **filesystem path**.
- `AWS_REGION` / `EMAIL_URL` / `CLOUDINARY_URL` are listed in `.env.example` but
  commented out in [config.rs:8](apps/api/src/config.rs#L8).
- **`origins` is hardcoded** to `yourdomain.com` — every real frontend request is
  CORS-rejected, and it isn't env-configurable.
- **`FIREBASE_SERVICE_ACCOUNT_PATH` hard-fails at startup** though only push needs it.
- **Redis has no `maxmemory`/`maxmemory-policy`** → unbounded cache can OOM it.

**Fix:** §4.6, §3.2, plus a generated `.env.example` derived from the config struct so
it **cannot** drift again.

### 7.9 🟠 P1 — Prometheus scrapes a hardcoded LAN IP

[resources/prometheus/prometheus.yml](resources/prometheus/prometheus.yml) targets
`192.168.111.244:8080`. Inside compose it must be `api:8080`. Prometheus currently
scrapes **nothing**.

Also: `/metrics` is **unauthenticated** and merged before the auth layer → public
metrics exposure. And **Grafana has no provisioning files** → blank on every fresh
start. And **promtail's config path is wrong** (compose mounts
`./resources/promtail/promtail-config.yml` while a near-identical stale duplicate sits
at repo root). Delete the root duplicate.

### 7.10 🟠 P1 — `.unwrap()` on a live request path

[broadcast/service.rs:1055](apps/api/src/modules/broadcast/service.rs#L1055) unwraps
`broadcast_list_cache_key` right after an `is_none()` early-return. Probably safe, but
an `Option` unwrap in request-serving code should be a `let … else`. Also the eight
`.expect()` calls in `BroadcastServiceBuilder::build` (§4.1).

### 7.11 🟠 P1 — Startup panics instead of failing cleanly

[database.rs](apps/api/src/database.rs) `.expect("Failed to connect to PostgreSQL DB")`,
[state.rs:125](apps/api/src/state.rs) `.expect("WsPubSubBridge failed to initialise")`,
[state.rs:146](apps/api/src/state.rs) `.unwrap()` on the SMTP relay build. A bad config
should exit with a readable message and non-zero code, not a panic trace.
**Fix:** `bootstrap` returns `Result`, `main` maps to `anyhow` exit.

### 7.12 🟡 P2 — Google OAuth account-linking risk

`upsert_google_user` links a Google identity to an existing account **by email alone**,
without checking `info.email_verified`. An attacker controlling an unverified Google
account with a victim's email address could take over the account. **Check
`email_verified` before linking**, and require a re-auth for the link.

### 7.13 🟡 P2 — `.docker/postgres/init.sql` is broken and unmounted

`create database meno_dev owner postgres` — but with `POSTGRES_USER: meno` the superuser
is `meno`, not `postgres`, so this **errors**. It's also never mounted by compose, and
the postgres image already creates `meno_dev` from `POSTGRES_DB`. Dead file that would
fail if anyone ever wired it up. **Delete it.**

### 7.14 🟡 P2 — Duplicated `shutdown_signal`

Implemented twice: [shared/signals.rs](apps/api/src/shared/signals.rs) and
[jobs/monitor.rs:175](apps/api/src/jobs/monitor.rs#L175). They can drift.
Consolidate in `bootstrap`.

### 7.15 🟡 P2 — Argon2 params are compile-time constants

`Params::new(19456, 2, 1)` (19 MiB, t=2, p=1) with the comment "tune upward until ~500ms"
— but they can't be tuned without a recompile. Make them env-configurable and re-hash
on next successful login.

---

## 8. What is genuinely missing (beyond tests)

| Gap                            | Priority | Notes                                                               |
| ------------------------------ | -------- | ------------------------------------------------------------------- |
| **CI pipeline**                | P0       | No `.github/workflows`. Nothing stops the sqlx rot from recurring.  |
| **README**                     | P0       | No run instructions for a new contributor.                          |
| **rust-toolchain.toml**        | P1       | Toolchain unpinned; Docker uses 1.88, local may differ.             |
| **rustfmt.toml / clippy.toml** | P1       | No formatting or lint policy.                                       |
| **Makefile / justfile**        | P1       | No single entry point for the ~20 commands this project needs.      |
| **OpenAPI spec**               | P1       | ~50 endpoints, no generated contract. **Both frontends need this.** |
| **cargo-deny**                 | P2       | No license/advisory policy.                                         |
| **Structured error codes**     | P0       | Prerequisite for both clients (§4.2).                               |
| **Backup/restore runbook**     | P2       | Neon branching gives point-in-time recovery — document it.          |
| **Incident/runbook docs**      | P2       | No on-call documentation.                                           |

---

## 9. Engineering standards to adopt

### 9.1 Error handling rules

- **Never `panic!`/`unwrap()`/`expect()` in request-serving code.** Enforced by Clippy
  (`unwrap_used`, `expect_used`, `panic`) as **deny** in `apps/api/src`, **allow** in
  `tests/`.
- Infrastructure errors are **logged with full context** and returned to the client as a
  generic message. `MenoError::Database` already does this — keep the pattern.
- **No leaked internals**: no SQL strings, no stack traces, no upstream URLs in responses.

### 9.2 Lint policy (`clippy.toml` + `Cargo.toml`)

```toml
[lints.clippy]
unwrap_used       = "deny"
expect_used       = "deny"
panic             = "deny"
todo               = "warn"
dbg_macro         = "deny"
print_stdout      = "deny"     # we use tracing, not prints
missing_docs      = "warn"
must_use_candidate = "warn"
```

> `print_stdout = "deny"` is a small but high-value rule: it guarantees every event goes
> through `tracing` and therefore ends up in Grafana, rather than vanishing into
> container stdout that nobody is collecting.

**CI gate: `cargo clippy --all-targets -- -D warnings`.**

### 9.3 Naming & module conventions

- Modules are `snake_case`; traits are `*Repo` / `*Cache` / `*Service` / `*Sender`.
- Handlers are thin: parse → authorise → delegate → map response. **No business logic in
  `handlers.rs`.** (Mostly true today — preserve it.)
- State structs expose **traits, not concrete types**, to handlers.
- **File-size lint**: no file over ~400 lines (enforce with a CI `wc -l` check).

### 9.4 Performance rules

- `tokio::join!` for independent I/O — already done well, keep.
- No DB query inside a loop; batch with `= ANY($1)`.
- Every Redis key has a TTL (enforced by the `RedisKey` type).
- `panic = "abort"` in release (see §2.2 note).

### 9.5 Security rules

- Secrets via a `Secret<T>` wrapper — never `Debug`-printed, never in error messages.
- Validate all input with `validator` at the DTO boundary (**already done** — keep).
- Parameterised SQL only — **currently 100% clean, preserve**.
- AuthZ checked in the **service layer**, never only in middleware.
- Dependabot/Renovate for `cargo audit`.

---

## 10. Testing strategy

> Excluded from the original review request, but **Phase 0 cannot complete without the
> harness**, so it is in scope here. The current test tree is
> `tests/mod.rs → tests/auth/mod.rs → tests/auth/repository_test.rs` where
> `repository_test.rs` is **a 0-byte file**. There are **zero tests**, and the layout
> is wrong anyway — Cargo only auto-discovers top-level `tests/*.rs`, so `tests/mod.rs`
> is the only real test target.

**Layered pyramid:**

| Layer           | Tool                         | Scope                                                                | Needs infra    |
| --------------- | ---------------------------- | -------------------------------------------------------------------- | -------------- |
| **Unit**        | `#[test]`                    | `crates/core` (cursor encode/decode, error mapping), validators      | none           |
| **Unit**        | `mockall`/`wiremock`         | Services with trait doubles + adapter traits (§5.6)                  | none           |
| **Integration** | `#[sqlx::test]`              | Repositories against real Postgres (transactional rollback per test) | Docker         |
| **Contract**    | `tower::ServiceExt::oneshot` | Router → status/body shape, auth 401s, rate-limit 429s               | Docker + Redis |
| **E2E**         | `reqwest`                    | Register → verify → login → create broadcast → join → chat           | full stack     |

**Priority order** (highest value first):

1. **`crates/core` cursor round-trip tests** — pure, fast, and the cursor system is the
   most intricate logic in the codebase (5 distinct cursor shapes).
2. **Repository integration tests** with `#[sqlx::test]` — these are what would have
   caught the dead `'TG_OP'` triggers. Test `followers`/`following`/`broadcasts`
   actually increment on subscribe/unsubscribe.
3. **Router contract tests** — assert unauthenticated requests to protected routes return 401. Cheap regression net for auth routing.
4. **Rate-limit / idempotency middleware tests** — these are the two subsystems that
   were _silently dead_. A test asserting "429 after N requests" is the proof they work.
5. Service-level tests for `start` / `end` / `join` with a fake LiveKit adapter.

**Fixture strategy:** `#[sqlx::test]` auto-rolls-back and runs each migration per test —
no shared DB state, no flaky ordering. Factory functions (`UserFactory`, `BroadcastFactory`)
keep tests readable.

---

## 11. Execution phases

Each phase ends in a **runnable, deployable state**. Do not proceed until the
definition-of-done checklist for that phase passes.

### Phase 0 — Unblock the build & get it running locally 🟥

_Nothing else is possible until this passes._

- [ ] Add `rust-toolchain.toml` (pin `1.88`), `rustfmt.toml`, `clippy.toml`
- [ ] `make db-up`: working Postgres via Docker, **migrations applied by `sqlx-cli`**
- [ ] **Fix `.sqlx`:** `cargo sqlx prepare --workspace -- --all-targets`, commit
      → **`cargo check --workspace --all-targets` passes with 0 errors** ✅ _gate_
- [ ] Add the migration runner (§4.3) and verify a **fresh DB builds from zero**
- [ ] Rewrite `.env.example` from the config struct; make Firebase optional (§4.6)
- [ ] Fix `docker-compose.yml` (all env vars, `env_file:`, `REDIS_URL`, `maxmemory-policy`)
- [ ] Make `config.rs` return `Result` and list **all** missing vars at once
- [ ] `make run` → `curl localhost:8080/health` returns `{"status":"ok","db":true,"redis":true}` ✅ _gate_

### Phase 1 — Fix silently-dead subsystems

- [ ] Wire rate limiting properly, fail-closed on `/auth/*` (§4.4) ✅ _gate: test proves 429_
- [ ] Wire idempotency + scope by user/route + in-flight lock + header replay (§4.5)
- [ ] Fix `'TG_OP'` triggers via new migration `0019` ✅ _gate: follower count test passes_
- [ ] Reconcile `total_participants` semantics via `0020` (§7.6)
- [ ] Remove all `unwrap`/`expect` from request paths; lint gate passes (§9.2)

### Phase 2 — Restructure into the monorepo

- [ ] Create `Meno/` with `apps/api`, `apps/web`, `apps/mobile`, `crates/`, `ops/`
- [ ] Root `Cargo.toml` workspace; move `packages/db/migrations` → `crates/db/migrations`
- [ ] Extract `crates/core` (pagination, error, ids, time) — pure, no I/O
- [ ] Move `shared/` → `infrastructure/` + `middleware/`; introduce adapter traits (§5.6)
- [ ] Split `BroadcastService` into 5 collaborators (§5.1); no file > 400 lines
- [ ] Consolidate the nine error enums into one (`crates/core::Error`) ✅ _gate: clients can rely on `code`_

### Phase 3 — Jobs & integrations

- [ ] Split `main.rs` (web) / `worker.rs` (jobs) with `AppRole` guard (§6)
- [ ] Move intervals to Apalis `ScheduledAt` (§6.3)
- [ ] **Implement the missing `BroadcastScheduledFanOutJob` scheduling** (§6.2) ✅ _gate: scheduled broadcast notifies subscribers_
- [ ] Batch + chunk fan-out (§6.4)
- [ ] Brevo adapter replacing lettre/SMTP (§3.6)
- [ ] R2 `ObjectStore` + MinIO dev impl; register `cdn.<domain>` (§3.3)

### Phase 4 — Observability & CI

- [ ] Fix Prometheus target → `api:8080`; delete stale promtail duplicate; delete `init.sql`
- [ ] Gate `/metrics` behind Render's private network (§7.9)
- [ ] Commit Grafana provisioning (datasources + dashboards) → auto-load ✅ _gate: dashboards appear on cold start_
- [ ] CI: `fmt --check`, `clippy -D warnings`, `sqlx prepare --check`, `build`, `test`
- [ ] `cargo-deny`; Dependabot for `cargo audit`
- [ ] README + Makefile with the full task list

### Phase 5 — Device sessions & hardening

- [ ] `auth_sessions` table + migration; device-bound refresh tokens
- [ ] Refresh-token **reuse detection** → revoke-all (§4.7)
- [ ] Revoke-by-device and log-out-all-devices endpoints
- [ ] Fix Google account-linking `email_verified` check (§7.12)
- [ ] Make Argon2 params env-configurable with re-hash-on-login (§7.15)
- [ ] Move OTP state from Redis to Neon (§3.2) — _critical given no persistence_

### Phase 6 — Deploy to Render

- [ ] Provision Neon (pooled URL), Render KV (free, `allkeys-lru`), R2 bucket + custom domain, Brevo
- [ ] `ops/render.yaml` Blueprint: `api` (Web Service) + `worker` (Background Worker)
- [ ] Both from one image, `--locked` builds, BuildKit cache mounts, non-root (§3.4)
- [ ] Set `CORS_ORIGINS` to real domains (removes the hardcoded `yourdomain.com`)
- [ ] Neon **branch per PR** to test migrations
- [ ] Smoke test: deploy → register → verify → login → create → go live → join → chat → receive WS event

---

## 12. Definition of done

The project is "started up from scratch" when **all** of these are true:

- [ ] `make bootstrap && make run` works on a **clean machine** with no manual steps
- [ ] `cargo check --workspace --all-targets` → **0 errors, 0 warnings**
- [ ] `cargo clippy --all-targets -- -D warnings` → **0 warnings**
- [ ] A **fresh Neon database** migrates from zero on boot
- [ ] `cargo sqlx prepare --check` passes in CI
- [ ] `GET /health` returns `{"status":"ok","db":true,"redis":true}`
- [ ] Rate limiting **provably** returns 429 (covered by test)
- [ ] Idempotency **provably** replays a cached response (covered by test)
- [ ] `followers` / `following` / `broadcasts` counters actually update (covered by test)
- [ ] `total_participants` matches `COUNT(… WHERE left_at IS NULL)` (covered by test)
- [ ] **No `unwrap`/`expect`/`panic!` in request-serving code** (lint-enforced)
- [ ] `cargo test` passes; CI green on a PR
- [ ] Grafana dashboards load automatically
- [ ] API and worker deployed as **separate** Render services; exactly one runs jobs
- [ ] A scheduled broadcast actually notifies its subscribers

---

## 13. Risks & mitigations

| Risk                                             | Impact                                   | Mitigation                                                                                              |
| ------------------------------------------------ | ---------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| Neon free tier suspends on inactivity            | Total outage                             | Neon keeps free projects alive with light activity; keep the worker pinging. Upgrade to paid if needed. |
| Render free spins down; WS drops                 | Users dropped from live broadcasts       | Starter plan before real users; client reconnect-with-backoff is mandatory anyway                       |
| Render KV free = 25 MB, no persistence           | Cache loss; **OTP loss is unacceptable** | Strict TTLs + `allkeys-lru`; **move OTP state to Neon** (§3.2)                                          |
| Neon PgBouncer + interactive transactions        | Subtle runtime failures                  | Pooled URL + short `max_lifetime`; forbid interactive transactions (§3.5)                               |
| `.sqlx` / migrations drift again                 | Build breaks in prod                     | `sqlx prepare --check` + migration smoke test in CI                                                     |
| sqlx macros resist dynamic queries               | Blocks refactor                          | Already solved via `QueryBuilder` + `build_query_as` — extend that pattern                              |
| Render build times (~6–10 min)                   | Slow deploys                             | BuildKit cache mounts; consider prebuilt images                                                         |
| Splitting `BroadcastService` regresses behaviour | Subtle bugs in live audio                | Tests in Phase 0/1 **before** the split; pure extraction otherwise                                      |
| R2 custom domain not registered                  | Public assets unavailable                | MinIO in dev; presigned fallback behind the `ObjectStore` trait                                         |
| Fan-out to very large subscriber lists           | Slow/expensive jobs                      | Batching + chunked pagination (§6.4)                                                                    |

---

## 14. Open questions for you

1. **Domain for R2** — do you have a domain to point `cdn.<domain>` at? Without it we
   need presigned URLs after all (§3.3).
2. **Budget** — everything above is free-tier except Render's $7/mo Starter ×2. Confirm
   whether to stay on Render Free during development (accepting spin-down) or go
   straight to Starter.
3. **Next.js / Flutter** — should the monorepo include them in _this_ document's scope,
   or API-only for now? I've scoped this to the API and left `apps/web` and
   `apps/mobile` as placeholders.
4. **LiveKit** — Cloud (managed) or self-hosted? Affects token/permission semantics and
   cost.
5. **Push (FCM)** — still needed if you're doing iOS+Android, or can we drop it and cut
   the Firebase config dependency entirely?
6. **Real-time at scale** — is the 25 MB Redis enough, or should I plan a paid Redis from
   the start once WS fan-out is load-bearing?

---

_End of document. Every claim in §7 is traceable to a specific file/line in the current
codebase; every recommendation is either a Rust/backend best practice or a direct
mitigation for a defect found in review._
