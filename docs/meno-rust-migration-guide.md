# Meno — NestJS → Rust Migration Guide

## Mono-repo Architecture & Epic Breakdown

---

## Strategic Overview

### Why This Stack

| Concern       | Choice                                   | Rationale                                                               |
| ------------- | ---------------------------------------- | ----------------------------------------------------------------------- |
| Web framework | **Axum**                                 | Tower ecosystem, first-class WebSocket/SSE, excellent Tokio integration |
| Async runtime | **Tokio**                                | De-facto standard; Axum, sqlx, redis-rs all build on it                 |
| Database      | **sqlx**                                 | Compile-time verified queries, async, no ORM magic                      |
| Real-time     | **axum-ws + tokio-tungstenite**          | Native Rust WebSocket — eliminates Socket.IO JS dependency entirely     |
| LiveKit       | **livekit-api** (Rust SDK)               | Official crate for room/token management                                |
| Auth          | **argon2 + jsonwebtoken**                | Direct ports of your current setup                                      |
| Observability | **tracing + opentelemetry + Prometheus** | Replaces Grafana/Prometheus stack, same dashboards work                 |
| Caching       | **fred** (Redis)                         | Async, connection pooling, better than redis-rs for production          |
| Email         | **lettre**                               | SMTP + templating                                                       |
| Jobs          | **apalis**                               | Background job queue backed by Postgres (no extra infra)                |
| Google OAuth  | **oauth2**                               | RFC-compliant, pairs with reqwest                                       |
| File uploads  | **aws-sdk-s3 / object_store**            | S3-compatible (works with any provider)                                 |

### Mono-repo Structure

```
meno/
├── Cargo.toml                  # workspace root
├── apps/
│   ├── api/                    # Rust backend (Axum)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── config.rs
│   │       ├── state.rs        # AppState (db pool, redis, livekit client)
│   │       ├── router.rs       # all route registration
│   │       ├── modules/
│   │       │   ├── auth/
│   │       │   ├── user/
│   │       │   ├── broadcast/
│   │       │   ├── chat/
│   │       │   ├── notification/
│   │       │   ├── subscriber/
│   │       │   ├── note/
│   │       │   └── settings/
│   │       └── shared/
│   │           ├── middleware/
│   │           ├── errors.rs
│   │           ├── pagination.rs
│   │           └── ws/         # WebSocket hub
│   ├── web/                    # Next.js (existing, moved in)
│   └── mobile/                 # Flutter (existing, moved in)
├── packages/
│   └── db/
│       └── migrations/         # sqlx migration files (from Prisma)
├── docker-compose.yml
├── dev-docker-compose.yml
└── .github/
    └── workflows/
        └── ci.yml
```

### Module Internal Structure (consistent across all modules)

```
modules/broadcast/
├── mod.rs          # re-exports + router fn
├── routes.rs       # axum handlers (thin, call service)
├── service.rs      # business logic
├── repository.rs   # sqlx queries
├── models.rs       # domain structs
├── dto.rs          # request/response types (serde)
└── errors.rs       # module-specific error variants
```

---

## Deployment Strategy (Free-Tier Friendly)

| Service       | Provider                          | Free Tier                                             |
| ------------- | --------------------------------- | ----------------------------------------------------- |
| Rust API      | **Railway** (Hobby) or **Fly.io** | 512MB RAM, enough for 10k–100k with Rust's efficiency |
| Postgres      | **Neon** (serverless Postgres)    | 0.5 GB free, scales on demand                         |
| Redis         | **Upstash** (serverless Redis)    | 10k commands/day free                                 |
| LiveKit       | **LiveKit Cloud**                 | Free tier for audio; pay-as-you-scale                 |
| Next.js       | **Vercel**                        | Free hobby tier                                       |
| Flutter       | **App Store / Play Store**        | Standard distribution                                 |
| Observability | **Grafana Cloud**                 | Free tier includes Prometheus + dashboards            |

> Railway is recommended for the API — Rust binaries are tiny and cold starts are near-zero, making it ideal.

---

## Epic 1 — Mono-repo Setup & CI/CD

**Goal:** Create the workspace, move existing frontends in, set up CI, and establish shared tooling.

### Ticket 1.1 — Initialise Cargo Workspace

```toml
# Cargo.toml (workspace root)
[workspace]
members = ["apps/api", "packages/db"]
resolver = "2"

[workspace.dependencies]
tokio       = { version = "1", features = ["full"] }
axum        = { version = "0.7", features = ["ws", "multipart"] }
sqlx        = { version = "0.8", features = ["postgres", "uuid", "time", "runtime-tokio"] }
serde       = { version = "1", features = ["derive"] }
serde_json  = "1"
uuid        = { version = "1", features = ["v4", "serde"] }
time        = { version = "0.3", features = ["serde"] }
tracing     = "0.1"
anyhow      = "1"
thiserror   = "1"
```

```
# Required crates for this epic
tokio, axum, serde, tracing, tracing-subscriber, dotenv, config
```

### Ticket 1.2 — Move Frontends Into Mono-repo

```
PSEUDO-CODE:
  cp -r ../meno-web    meno/apps/web
  cp -r ../meno-mobile meno/apps/mobile

  # Update web's API base URL to env var
  # Update Flutter's base URL to env var

  # Create root package.json for web workspace tooling
  # Add Makefile with:
  #   make dev-api
  #   make dev-web
  #   make dev-mobile
  #   make migrate
```

### Ticket 1.3 — Docker Compose (Development)

```yaml
# dev-docker-compose.yml
services:
  postgres:
    image: postgres:16
    environment:
      POSTGRES_DB: meno_dev
      POSTGRES_USER: meno
      POSTGRES_PASSWORD: secret
    ports: ["5432:5432"]

  redis:
    image: redis:7-alpine
    ports: ["6379:6379"]

  # Grafana + Prometheus for local observability
  prometheus:
    image: prom/prometheus
    volumes: ["./resources/prometheus:/etc/prometheus"]
    ports: ["9090:9090"]

  grafana:
    image: grafana/grafana
    ports: ["3001:3000"]
    depends_on: [prometheus]
```

### Ticket 1.4 — GitHub Actions CI

```yaml
# .github/workflows/ci.yml
PSEUDO-CODE:
  on: [push, pull_request]
  jobs:
    rust-api:
      - cargo fmt --check
      - cargo clippy -- -D warnings
      - cargo test
      - cargo build --release

    next-web:
      - pnpm install
      - pnpm lint
      - pnpm build

    flutter-mobile:
      - flutter pub get
      - flutter analyze
      - flutter test
```

---

## Epic 2 — Database Migration (Prisma → sqlx)

**Goal:** Translate all Prisma migrations to raw SQL sqlx migrations. No ORM.

```
# Required crates
sqlx = { version = "0.8", features = ["postgres", "uuid", "time", "runtime-tokio", "migrate"] }
uuid = { version = "1", features = ["v4", "serde"] }
time = "0.3"
```

### Ticket 2.1 — Consolidate Prisma Migrations to Baseline SQL

```sql
-- packages/db/migrations/0001_initial_schema.sql
-- (Derived from your 15+ Prisma migration files, consolidated)

CREATE EXTENSION IF NOT EXISTS "pgcrypto";

CREATE TYPE "Role"               AS ENUM ('admin', 'guest');
CREATE TYPE "BroadcastRole"      AS ENUM ('HOST', 'COHOST', 'LISTENER');
CREATE TYPE "Status"             AS ENUM ('active', 'inactive');
CREATE TYPE "Display"            AS ENUM ('light', 'dark');
CREATE TYPE "EmailAccountType"   AS ENUM ('normal', 'google');
CREATE TYPE "NotificationType"   AS ENUM (
  'addedAsCoHost', 'userSubscribed', 'scheduledBroadcast',
  'liveBroadcastStarted', 'verifyEmail', 'resetPassword'
);

CREATE TABLE users (
  id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  full_name           TEXT NOT NULL,
  bio                 TEXT,
  email               TEXT NOT NULL UNIQUE,
  role                "Role" NOT NULL DEFAULT 'guest',
  password            TEXT NOT NULL,
  email_account_type  "EmailAccountType" NOT NULL DEFAULT 'normal',
  verified            BOOLEAN NOT NULL DEFAULT true,
  image_id            TEXT,
  image_url           TEXT,
  created_at          TIMESTAMPTZ DEFAULT now(),
  deleted_at          TIMESTAMPTZ
);

CREATE TABLE broadcasts (
  id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  title            TEXT NOT NULL,
  description      VARCHAR(244) NOT NULL,
  status           "Status" NOT NULL DEFAULT 'inactive',
  broadcast_token  TEXT,
  time_zone        TEXT NOT NULL DEFAULT 'etc/UTC',
  image_id         TEXT,
  image_url        TEXT,
  start_time       TIMESTAMPTZ,
  end_time         TIMESTAMPTZ,
  created_at       TIMESTAMPTZ DEFAULT now(),
  deleted_at       TIMESTAMPTZ,
  creator_id       UUID NOT NULL REFERENCES users(id) ON UPDATE CASCADE ON DELETE CASCADE
);

CREATE TABLE broadcast_cohosts (
  cohost_id    UUID NOT NULL REFERENCES users(id) ON UPDATE CASCADE ON DELETE CASCADE,
  broadcast_id UUID NOT NULL REFERENCES broadcasts(id) ON UPDATE CASCADE ON DELETE CASCADE,
  PRIMARY KEY (cohost_id, broadcast_id)
);

CREATE TABLE broadcast_listeners (
  listener_id  UUID NOT NULL REFERENCES users(id) ON UPDATE CASCADE ON DELETE CASCADE,
  broadcast_id UUID NOT NULL REFERENCES broadcasts(id) ON UPDATE CASCADE ON DELETE CASCADE,
  joined_at    TIMESTAMPTZ NOT NULL,
  role         "BroadcastRole" NOT NULL DEFAULT 'LISTENER',
  PRIMARY KEY (listener_id, broadcast_id)
);

CREATE TABLE chat_messages (
  id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  content      VARCHAR(256) NOT NULL,
  created_at   TIMESTAMPTZ NOT NULL,
  updated_at   TIMESTAMPTZ,
  sender_id    UUID NOT NULL REFERENCES users(id) ON UPDATE CASCADE ON DELETE CASCADE,
  broadcast_id UUID NOT NULL REFERENCES broadcasts(id) ON UPDATE CASCADE ON DELETE CASCADE
);
CREATE INDEX idx_chat_messages_sender_broadcast ON chat_messages(sender_id, broadcast_id);

CREATE TABLE chat_reactions (
  id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  content      VARCHAR(256) NOT NULL,
  created_at   TIMESTAMPTZ NOT NULL,
  sender_id    UUID NOT NULL REFERENCES users(id) ON UPDATE CASCADE ON DELETE CASCADE,
  broadcast_id UUID NOT NULL REFERENCES broadcasts(id) ON UPDATE CASCADE ON DELETE CASCADE
);
CREATE INDEX idx_chat_reactions_sender_broadcast ON chat_reactions(sender_id, broadcast_id);

CREATE TABLE general_settings (
  id                      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  push_notifications      BOOLEAN NOT NULL DEFAULT true,
  app_notifications       BOOLEAN NOT NULL DEFAULT true,
  email_notifications     BOOLEAN NOT NULL DEFAULT false,
  push_notification_token TEXT,
  notification_settings   JSONB NOT NULL DEFAULT '[]',
  display                 "Display" NOT NULL DEFAULT 'light',
  language                TEXT NOT NULL DEFAULT 'en/English',
  user_id                 UUID NOT NULL UNIQUE REFERENCES users(id) ON UPDATE CASCADE ON DELETE CASCADE
);

CREATE TABLE notifications (
  id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  type          "NotificationType" NOT NULL,
  read          BOOLEAN NOT NULL DEFAULT false,
  created_at    TIMESTAMPTZ DEFAULT now(),
  owner_id      UUID NOT NULL REFERENCES users(id) ON UPDATE CASCADE ON DELETE CASCADE,
  broadcast_id  UUID REFERENCES broadcasts(id) ON UPDATE CASCADE ON DELETE CASCADE,
  subscriber_id UUID REFERENCES users(id) ON UPDATE CASCADE ON DELETE CASCADE,
  email         TEXT,
  code          TEXT
);

CREATE TABLE notes (
  id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  title      VARCHAR(100) NOT NULL,
  content    VARCHAR(10000) NOT NULL,
  pinned     BOOLEAN NOT NULL DEFAULT false,
  folder_id  UUID,
  creator_id UUID NOT NULL REFERENCES users(id) ON UPDATE CASCADE ON DELETE CASCADE,
  created_at TIMESTAMPTZ DEFAULT now(),
  updated_at TIMESTAMPTZ DEFAULT now()
);

CREATE TABLE folders (
  id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  title      VARCHAR(100) NOT NULL,
  pinned     BOOLEAN NOT NULL DEFAULT false,
  creator_id UUID NOT NULL REFERENCES users(id) ON UPDATE CASCADE ON DELETE CASCADE,
  created_at TIMESTAMPTZ DEFAULT now()
);

ALTER TABLE notes
  ADD CONSTRAINT notes_folder_fk
  FOREIGN KEY (folder_id) REFERENCES folders(id) ON DELETE SET NULL;

CREATE TABLE user_subscribers (
  subscriber_id    UUID NOT NULL REFERENCES users(id) ON UPDATE CASCADE ON DELETE CASCADE,
  subscription_id  UUID NOT NULL REFERENCES users(id) ON UPDATE CASCADE ON DELETE CASCADE,
  PRIMARY KEY (subscriber_id, subscription_id)
);

CREATE TABLE otp (
  id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  email      TEXT NOT NULL,
  code       TEXT NOT NULL UNIQUE,
  used       BOOLEAN NOT NULL DEFAULT false,
  expires_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE refresh_tokens (
  id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  token_hash TEXT NOT NULL UNIQUE,
  expires_at TIMESTAMPTZ NOT NULL,
  created_at TIMESTAMPTZ DEFAULT now()
);
```

> Note: Column names are converted to snake_case (Rust convention). `deleted` → `deleted_at` for clarity. `broadcastToken` (the Agora token field) → `broadcast_token`.

### Ticket 2.2 — sqlx AppState Setup

```rust
// apps/api/src/state.rs
PSEUDO-CODE:
  pub struct AppState {
    pub db:       sqlx::PgPool,
    pub redis:    fred::clients::RedisClient,
    pub livekit:  LiveKitClient,
    pub config:   Arc<Config>,
    pub ws_hub:   Arc<WsHub>,          // broadcast WebSocket hub
  }

  pub async fn build_state(config: Config) -> AppState {
    let db = PgPoolOptions::new()
      .max_connections(20)
      .connect(&config.database_url)
      .await?;

    sqlx::migrate!("../../packages/db/migrations")
      .run(&db)
      .await?;

    let redis = RedisClient::new(config.redis_url)?;

    AppState { db, redis, livekit, config: Arc::new(config), ws_hub }
  }
```

### Ticket 2.3 — Generic Repository Helpers

```rust
// apps/api/src/shared/pagination.rs
PSEUDO-CODE:
  pub struct PaginationParams {
    pub page:  i64,   // 1-indexed
    pub limit: i64,
  }

  impl PaginationParams {
    pub fn offset(&self) -> i64 { (self.page - 1) * self.limit }
  }

  pub struct PaginatedResponse<T> {
    pub data:        Vec<T>,
    pub total:       i64,
    pub page:        i64,
    pub total_pages: i64,
  }
```

---

## Epic 3 — Core Infrastructure (Axum App Skeleton)

**Goal:** Get a running Axum server with middleware, error handling, and observability wired up.

```
# Required crates
axum = { version = "0.7", features = ["ws", "multipart"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["cors", "trace", "compression"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
axum-prometheus = "0.6"       # Prometheus metrics middleware
opentelemetry = "0.22"
sentry = "0.32"               # optional: error tracking (free tier)
```

### Ticket 3.1 — main.rs & Router

```rust
// apps/api/src/main.rs
PSEUDO-CODE:
  #[tokio::main]
  async fn main() {
    init_tracing();          // JSON logs in prod, pretty in dev
    let config = Config::from_env();
    let state  = build_state(config).await;

    let app = Router::new()
      .merge(auth_router())
      .merge(user_router())
      .merge(broadcast_router())
      .merge(chat_router())
      .merge(notification_router())
      .merge(subscriber_router())
      .merge(note_router())
      .merge(folder_router())
      .merge(settings_router())
      .route("/ws",     get(ws_handler))
      .route("/health", get(health_handler))
      .route("/metrics", get(metrics_handler))   // Prometheus scrape
      .layer(CorsLayer::permissive())            // tighten in prod
      .layer(TraceLayer::new_for_http())
      .layer(CompressionLayer::new())
      .with_state(Arc::new(state));

    let listener = TcpListener::bind("0.0.0.0:8080").await?;
    axum::serve(listener, app).await?;
  }
```

### Ticket 3.2 — Centralised Error Handling

```rust
// apps/api/src/shared/errors.rs
PSEUDO-CODE:
  // Mirrors NestJS exception filter
  pub enum AppError {
    Unauthorized(String),        // 401
    BadRequest(String),          // 400
    NotFound(String),            // 404
    Conflict(String),            // 409
    Internal(anyhow::Error),     // 500
  }

  // impl IntoResponse for AppError
  //   -> JSON: { "statusCode": N, "message": "...", "error": "..." }
  //   -> tracing::error! on Internal variants

  // Use thiserror for clean error derivation in sub-modules
```

### Ticket 3.3 — Auth Middleware

```rust
// apps/api/src/shared/middleware/auth.rs
PSEUDO-CODE:
  // Replaces AuthMiddleware from NestJS exactly
  pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    mut request:  Request,
    next:         Next,
  ) -> Result<Response, AppError> {

    let token = extract_bearer(&request.headers)?;
    let claims = state.jwt.decode(token)?;    // AppError::Unauthorized if expired

    // Check expiry (mirrors existing tokenExpired check)
    if claims.exp < now_unix() {
      return Err(AppError::Unauthorized("Token has expired"));
    }

    let user = state.user_repo.find_by_id(claims.sub).await?
      .ok_or(AppError::BadRequest("User does not exist"))?;

    request.extensions_mut().insert(AuthUser(user));
    Ok(next.run(request).await)
  }

  // Apply selectively:
  // Router::new()
  //   .route("/protected", get(handler))
  //   .route_layer(middleware::from_fn_with_state(state, auth_middleware))
```

### Ticket 3.4 — Observability (Replaces Grafana/Prometheus Setup)

```rust
PSEUDO-CODE:
  // tracing-subscriber in JSON mode for production
  // axum-prometheus exposes /metrics endpoint
  // Grafana Cloud scrapes it (same dashboards as before, just different source)

  // Resources:
  // - resources/prometheus/prometheus.yml  (scrape interval: 15s)
  // - resources/grafana/dashboards/*.json  (import existing dashboards)

  // Key metrics automatically tracked:
  //   - http_requests_total (by route, method, status)
  //   - http_request_duration_seconds
  //   - active_websocket_connections (custom counter in WsHub)
  //   - livekit_active_rooms (custom gauge)
```

---

## Epic 4 — Authentication & User Management

**Goal:** Migrate all auth flows. Email/password + JWT refresh tokens + Google OAuth.

```
# Required crates
argon2      = "0.5"
jsonwebtoken = "9"
oauth2      = "4"
reqwest     = { version = "0.12", features = ["json"] }
lettre      = { version = "0.11", features = ["tokio1-rustls-tls", "builder"] }
rand        = "0.8"
```

### Ticket 4.1 — JWT Service

```rust
// modules/auth/jwt_service.rs
PSEUDO-CODE:
  pub struct JwtService { secret: String, refresh_secret: String }

  pub struct AccessClaims {
    pub sub: Uuid,     // user id
    pub exp: i64,
    pub iat: i64,
    pub role: Role,
  }

  pub struct RefreshClaims {
    pub sub:      Uuid,
    pub token_id: Uuid,   // for revocation
    pub exp:      i64,
  }

  impl JwtService {
    pub fn sign_access(&self, user: &User)    -> Result<String>
    pub fn sign_refresh(&self, user_id: Uuid) -> Result<(String, Uuid)>  // token, jti
    pub fn decode_access(&self, token: &str)  -> Result<AccessClaims>
    pub fn decode_refresh(&self, token: &str) -> Result<RefreshClaims>
  }

  // Refresh token flow:
  //   1. On login: create refresh_token row in DB (hashed), return both tokens
  //   2. On refresh: verify token, check DB row exists + not expired, rotate
  //   3. On logout: delete refresh_token row (revokes immediately)
```

### Ticket 4.2 — Auth Routes

```
POST /auth/register
  BODY: { fullName, email, password }
  - hash password with argon2 (Argon2id, time_cost=2, mem_cost=65536)
  - insert user row
  - create general_settings row (defaults)
  - send verification OTP email
  - return { accessToken, refreshToken, user }

POST /auth/login
  BODY: { email, password }
  - fetch user by email
  - argon2::verify(password, user.password_hash)
  - if !user.verified -> 403 with message
  - sign access + refresh tokens
  - store refresh token hash in refresh_tokens table
  - return { accessToken, refreshToken, user }

POST /auth/refresh
  BODY: { refreshToken }
  - decode refresh JWT
  - look up refresh_tokens row by jti (UUID in claims)
  - verify hash matches
  - if expired -> 401
  - rotate: delete old row, insert new row
  - return { accessToken, refreshToken }

POST /auth/logout
  AUTH REQUIRED
  - delete refresh_tokens row for current user
  - return 200

POST /auth/google
  BODY: { idToken }
  - verify Google ID token via Google tokeninfo endpoint
  - upsert user (email_account_type = google, verified = true)
  - return { accessToken, refreshToken, user }

POST /auth/forgot-password
  BODY: { email }
  - generate 6-digit OTP
  - store in otp table (expires_at = now + 15min)
  - send email via lettre
  - return 200 (never reveal if email exists)

POST /auth/reset-password
  BODY: { email, code, newPassword }
  - find otp row: email match, not used, not expired
  - argon2::hash(newPassword)
  - update user.password
  - mark otp.used = true
  - revoke all refresh tokens for user
  - return 200

POST /auth/verify-email
  BODY: { email, code }
  - same OTP lookup pattern
  - set user.verified = true
  - return 200
```

### Ticket 4.3 — User Module Routes

```
GET  /users/me           -> return authenticated user + settings
PATCH /users/me          -> update fullName, bio, imageUrl
DELETE /users/me         -> soft delete (set deleted_at = now())

GET  /users/:id          -> public profile
GET  /users/search?q=    -> full-text search (Postgres tsvector on full_name)

# Profile image upload
POST /users/me/image
  BODY: multipart/form-data { file }
  - validate: image/jpeg or image/png, max 5MB
  - upload to S3/object_store
  - update user.image_url
```

```
# Additional crates for file uploads
aws-sdk-s3 = "1"      # or object_store = "0.10" (provider-agnostic)
mime = "0.3"
```

---

## Epic 5 — Broadcasts Module

**Goal:** Full broadcast lifecycle — CRUD, LiveKit room management, scheduling.

```
# Required crates
livekit-api = "0.3"    # Official LiveKit Rust SDK
apalis      = { version = "0.6", features = ["postgres"] }
apalis-sql  = "0.6"
chrono      = { version = "0.4", features = ["serde"] }
```

### Ticket 5.1 — Broadcast CRUD

```
POST /broadcasts
  AUTH REQUIRED
  BODY: { title, description, timeZone, startTime?, imageUrl? }
  - insert broadcast (status = inactive, creator_id = auth user)
  - if startTime set -> schedule BroadcastStartJob via apalis
  - return broadcast

GET /broadcasts
  QUERY: { status?, creatorId?, onlySubscriptions?, keywords?, page, limit }
  - if onlySubscriptions=true -> JOIN user_subscribers filter
  - full-text search via Postgres tsvector if keywords present
  - soft-delete filter: WHERE deleted_at IS NULL
  - return PaginatedResponse<Broadcast>

GET /broadcasts/:id
  QUERY: { include? }   # "totalListeners" etc, mirrors existing DTO
  - fetch broadcast + optional aggregates
  - return Broadcast

PATCH /broadcasts/:id
  AUTH REQUIRED, must be creator
  - partial update
  - if startTime changed -> reschedule apalis job
  - return updated broadcast

DELETE /broadcasts/:id
  AUTH REQUIRED, must be creator
  - soft delete: set deleted_at = now()
  - if broadcast is active: end LiveKit room
  - return 200
```

### Ticket 5.2 — LiveKit Integration

```rust
PSEUDO-CODE:
  // livekit_service.rs
  pub struct LiveKitService {
    api_key:    String,
    api_secret: String,
    host:       String,
  }

  impl LiveKitService {
    pub fn create_room_token(
      &self,
      room_name: &str,
      participant_identity: &str,
      grants: VideoGrants,     // livekit_api::VideoGrants
    ) -> Result<String>
    // Grants differ by role:
    //   HOST:    can_publish=true, can_subscribe=true, room_admin=true
    //   COHOST:  can_publish=true, can_subscribe=true
    //   LISTENER: can_publish=false, can_subscribe=true

    pub async fn delete_room(&self, room_name: &str) -> Result<()>
    pub async fn list_participants(&self, room_name: &str) -> Result<Vec<Participant>>
  }

POST /broadcasts/:id/go-live
  AUTH REQUIRED, must be creator
  - set status = active
  - generate LiveKit token for HOST
  - store token in broadcasts.broadcast_token
  - emit WS event "broadcast:started" to all subscribers (see Epic 7)
  - create liveBroadcastStarted notifications for all subscribers
  - return { token, wsUrl }

POST /broadcasts/:id/end
  AUTH REQUIRED, must be creator or admin
  - set status = inactive, end_time = now()
  - delete LiveKit room
  - emit WS event "broadcast:ended"
  - return 200

POST /broadcasts/:id/join
  AUTH REQUIRED
  - upsert broadcast_listeners (listener_id, broadcast_id, role=LISTENER, joined_at)
  - generate LiveKit token for LISTENER
  - emit WS event "broadcast:listener_joined" to room
  - return { token, wsUrl }

POST /broadcasts/:id/leave
  AUTH REQUIRED
  - delete broadcast_listeners row
  - emit WS event "broadcast:listener_left"
  - return 200
```

### Ticket 5.3 — Cohosts

```
POST /broadcasts/:id/cohosts
  AUTH: creator only
  BODY: { userId }
  - insert broadcast_cohosts row
  - create addedAsCoHost notification for the user
  - generate LiveKit COHOST token
  - emit WS event "broadcast:cohost_added" to that user
  - return 201

DELETE /broadcasts/:id/cohosts/:userId
  AUTH: creator only
  - delete broadcast_cohosts row
  - if broadcast active: revoke their LiveKit token (delete room participant)
  - return 200

GET /broadcasts/:id/cohosts
  - return list of cohosts with user profiles
```

### Ticket 5.4 — Background Jobs (apalis)

```rust
PSEUDO-CODE:
  // jobs/broadcast_start_job.rs
  pub struct BroadcastStartJob { pub broadcast_id: Uuid }

  impl Job for BroadcastStartJob {
    // Runs at scheduled startTime
    async fn execute(&self, ctx: JobContext) -> Result<(), JobError> {
      let state = ctx.data::<Arc<AppState>>()?;
      let broadcast = state.broadcast_repo.find(self.broadcast_id).await?;
      if broadcast.status == Status::Inactive {
        // Notify subscribers: "broadcast starting soon"
        // Create scheduledBroadcast notifications
        state.notification_service.notify_subscribers(broadcast).await?;
      }
      Ok(())
    }
  }

  // Register in main:
  // Monitor::new()
  //   .register(WorkerBuilder::new("broadcast-start")
  //     .data(Arc::clone(&state))
  //     .build_fn(BroadcastStartJob::execute))
  //   .run().await?;
```

---

## Epic 6 — Subscribers & Notifications

### Ticket 6.1 — Subscriber Routes

```
POST /subscribers/:creatorId
  AUTH REQUIRED
  - insert user_subscribers (subscriber_id=me, subscription_id=creator)
  - create userSubscribed notification for creator
  - emit WS event "user:new_subscriber" to creator
  - return 201

DELETE /subscribers/:creatorId
  AUTH REQUIRED
  - delete user_subscribers row
  - return 200

GET /subscribers/me/subscribers    -> people who follow me (paginated)
GET /subscribers/me/subscriptions  -> people I follow (paginated)

GET /subscribers/:userId/subscribers    -> public follower count + list
GET /subscribers/:userId/subscriptions  -> public following count + list
```

### Ticket 6.2 — Notifications Routes

```
GET /notifications
  AUTH REQUIRED
  QUERY: { read?, type?, page, limit }
  - fetch notifications WHERE owner_id = me
  - return PaginatedResponse<Notification>

PATCH /notifications/:id/read
  AUTH REQUIRED
  - set read = true
  - return updated notification

PATCH /notifications/read-all
  AUTH REQUIRED
  - UPDATE notifications SET read = true WHERE owner_id = me AND read = false
  - return { updated: count }

DELETE /notifications/:id
  AUTH REQUIRED, must be owner
  - hard delete (notifications don't need soft delete)
  - return 200
```

### Ticket 6.3 — Push Notifications

```
# Required crates
fcm = "0.9"         # Firebase Cloud Messaging (for Flutter + Web push)
# or: use direct HTTP to FCM v1 API via reqwest

PSEUDO-CODE:
  pub struct PushService { fcm_key: String }

  impl PushService {
    pub async fn send(
      &self,
      token: &str,
      title: &str,
      body: &str,
      data: Option<serde_json::Value>,
    ) -> Result<()> {
      // POST to FCM API
      // Called from notification_service after DB insert
    }

    pub async fn send_to_user(&self, state, user_id, payload) -> Result<()> {
      let settings = state.settings_repo.find_by_user(user_id).await?;
      if settings.push_notifications {
        if let Some(token) = settings.push_notification_token {
          self.send(&token, ...).await?;
        }
      }
    }
  }
```

---

## Epic 7 — Real-Time (WebSocket Hub)

**Goal:** Replace Socket.IO with native Axum WebSockets. Same event model, better performance.

```
# Required crates
axum           = { features = ["ws"] }
tokio          = { features = ["sync"] }  # for broadcast channel
serde_json     = "1"
dashmap        = "5"   # concurrent HashMap for connected clients
```

### Ticket 7.1 — WebSocket Hub

```rust
// apps/api/src/shared/ws/hub.rs
PSEUDO-CODE:
  // Each connected client has a Sender half of a tokio::sync::mpsc channel
  // Hub stores: user_id -> Vec<Sender<WsMessage>>
  // (one user can have multiple connections: web + mobile)

  pub struct WsHub {
    clients: DashMap<Uuid, Vec<mpsc::Sender<WsMessage>>>,
  }

  pub enum WsMessage {
    BroadcastStarted    { broadcast: BroadcastDto },
    BroadcastEnded      { broadcast_id: Uuid },
    ChatMessage         { message: ChatMessageDto },
    ChatReaction        { reaction: ChatReactionDto },
    ListenerJoined      { user: UserDto, broadcast_id: Uuid },
    ListenerLeft        { user_id: Uuid, broadcast_id: Uuid },
    NewSubscriber       { subscriber: UserDto },
    Notification        { notification: NotificationDto },
    CohostAdded         { broadcast_id: Uuid, token: String },
    PresenceUpdate      { user_id: Uuid, online: bool },
  }

  impl WsHub {
    pub fn register(&self, user_id: Uuid, sender: mpsc::Sender<WsMessage>)
    pub fn unregister(&self, user_id: Uuid, sender_id: usize)
    pub async fn send_to_user(&self, user_id: Uuid, msg: WsMessage)
    pub async fn send_to_users(&self, user_ids: &[Uuid], msg: WsMessage)
    pub async fn broadcast_to_room(&self, listener_ids: &[Uuid], msg: WsMessage)
  }
```

### Ticket 7.2 — WebSocket Handler

```rust
// apps/api/src/shared/ws/handler.rs
PSEUDO-CODE:
  // GET /ws?token=<access_token>
  pub async fn ws_handler(
    ws:    WebSocketUpgrade,
    Query(params): Query<WsParams>,   // { token }
    State(state):  State<Arc<AppState>>,
  ) -> Response {
    // Authenticate via query param token (same JWT)
    let claims = state.jwt.decode(&params.token)?;

    ws.on_upgrade(move |socket| handle_socket(socket, claims.sub, state))
  }

  async fn handle_socket(socket: WebSocket, user_id: Uuid, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = mpsc::channel::<WsMessage>(64);

    // Register with hub
    state.ws_hub.register(user_id, tx.clone());

    // Mark user online -> emit PresenceUpdate to subscribers
    state.ws_hub.send_to_subscribers(user_id, WsMessage::PresenceUpdate {
      user_id, online: true
    }).await;

    // Spawn task: hub messages -> WebSocket
    let send_task = tokio::spawn(async move {
      while let Some(msg) = rx.recv().await {
        let json = serde_json::to_string(&msg)?;
        sender.send(Message::Text(json)).await?;
      }
    });

    // Receive loop: handle ping/pong + client messages (if any)
    while let Some(Ok(msg)) = receiver.next().await {
      match msg {
        Message::Close(_) => break,
        Message::Ping(p)  => { /* pong handled automatically by axum */ }
        _                 => {}
      }
    }

    // Cleanup on disconnect
    send_task.abort();
    state.ws_hub.unregister(user_id);
    state.ws_hub.send_to_subscribers(user_id, WsMessage::PresenceUpdate {
      user_id, online: false
    }).await;
  }
```

### Ticket 7.3 — Redis Pub/Sub for Horizontal Scaling

```rust
// When you have multiple API instances (Railway scales horizontally)
// WsHub alone won't work — a WS connection on instance A can't
// reach a client on instance B.
// Solution: publish WS events to Redis, all instances subscribe.

PSEUDO-CODE:
  pub struct RedisWsBridge {
    pub_client: fred::clients::RedisClient,
    sub_client: fred::clients::SubscriberClient,
    local_hub:  Arc<WsHub>,
  }

  impl RedisWsBridge {
    pub async fn publish(&self, channel: &str, msg: WsMessage) {
      let json = serde_json::to_string(&msg)?;
      self.pub_client.publish(channel, json).await?;
    }

    pub async fn start_subscriber_loop(&self) {
      self.sub_client.subscribe("ws:events").await?;
      while let Ok(msg) = self.sub_client.next_message().await {
        let ws_msg: WsMessage = serde_json::from_str(&msg.value)?;
        // Deliver to local clients only
        self.local_hub.deliver_if_local(ws_msg).await;
      }
    }
  }

  // All services call bridge.publish() instead of hub.send_to_user() directly
```

---

## Epic 8 — Chat Module

### Ticket 8.1 — Chat Routes

```
POST /broadcasts/:id/chat/messages
  AUTH REQUIRED, must be listener or host
  BODY: { content }
  - validate content length (max 256 chars)
  - insert chat_messages row
  - emit WS event ChatMessage to all listeners of this broadcast room
  - return ChatMessageDto

GET /broadcasts/:id/chat/messages
  QUERY: { page, limit, before? }  # cursor-based for live chat
  - fetch messages ORDER BY created_at DESC
  - return PaginatedResponse<ChatMessage>

PATCH /broadcasts/:id/chat/messages/:msgId
  AUTH REQUIRED, must be sender
  - update content + updated_at
  - emit WS event ChatMessageUpdated
  - return updated message

DELETE /broadcasts/:id/chat/messages/:msgId
  AUTH REQUIRED, must be sender or host
  - hard delete
  - emit WS event ChatMessageDeleted
  - return 200

POST /broadcasts/:id/chat/reactions
  AUTH REQUIRED, must be listener or host
  BODY: { content }
  - insert chat_reactions row
  - emit WS event ChatReaction (fire and forget, don't persist read)
  - return 201
```

---

## Epic 9 — Notes & Folders Module

```
# No new crates needed beyond base stack

GET    /notes                  -> paginated, creator_id = me
POST   /notes                  -> create note
GET    /notes/:id              -> get single note (must be owner)
PATCH  /notes/:id              -> update content/title/pinned/folder
DELETE /notes/:id              -> hard delete

GET    /folders                -> all folders for user
POST   /folders                -> create folder
GET    /folders/:id            -> folder + its notes
PATCH  /folders/:id            -> update title/pinned
DELETE /folders/:id            -> delete folder (notes.folder_id SET NULL)

# Full-text search across notes
GET /notes/search?q=
  - SELECT ... WHERE creator_id = me
      AND to_tsvector('english', title || ' ' || content) @@ plainto_tsquery($1)
```

---

## Epic 10 — Settings Module

```
GET   /settings
  AUTH REQUIRED
  - fetch general_settings WHERE user_id = me
  - return settings

PATCH /settings
  AUTH REQUIRED
  BODY: { pushNotifications?, appNotifications?, emailNotifications?,
          pushNotificationToken?, display?, language?, notificationSettings? }
  - partial update
  - return updated settings

# notificationSettings is JSONB — validate structure in Rust:
  pub struct NotificationSetting {
    pub notification_type: NotificationType,
    pub push:  bool,
    pub email: bool,
    pub app:   bool,
  }
  // Validate before storing: ensure all NotificationType variants are present
```

---

## Epic 11 — Future: Video Broadcasting & Restreaming

> This epic is planned, not immediate. Architecture decisions made now should not block it.

### How to Prepare the Current Architecture

```
1. BROADCAST TABLE — add columns now (nullable, so no breaking change):
   ALTER TABLE broadcasts ADD COLUMN IF NOT EXISTS video_enabled BOOLEAN DEFAULT false;
   ALTER TABLE broadcasts ADD COLUMN IF NOT EXISTS recording_url TEXT;
   ALTER TABLE broadcasts ADD COLUMN IF NOT EXISTS rtmp_key TEXT;

2. LIVEKIT ROOM CREATION — when creating rooms, always set:
   RoomOptions {
     max_participants: 1000,
     empty_timeout:    300,
     // These don't limit video later
   }
   // Video grants are additive — just update VideoGrants.can_publish_sources
   // to include CameraTrack when the time comes

3. RESTREAMING — LiveKit supports RTMP egress natively:
   // livekit-api: RoomServiceClient::start_rtmp_egress(room_name, rtmp_url)
   // This lets you restream to YouTube Live, Twitch, etc. with one API call
   // No additional infra needed

4. RECORDING / PLAYBACK — LiveKit cloud handles recording egress:
   // Store recording_url in broadcasts table after egress completes
   // Serve via CDN or S3 pre-signed URLs
```

```
# Crates to add when needed (not now)
livekit-api = "0.3"   # already in use; RTMP egress is part of same SDK
object_store = "0.10"  # for recording storage (S3-compatible)
```

---

## Appendix A — Environment Variables

```env
# apps/api/.env
DATABASE_URL=postgresql://meno:secret@localhost:5432/meno_dev
REDIS_URL=redis://localhost:6379
JWT_SECRET=<64-char random string>
JWT_REFRESH_SECRET=<64-char random string>
JWT_ACCESS_EXPIRES_IN=900        # 15 minutes (seconds)
JWT_REFRESH_EXPIRES_IN=2592000   # 30 days (seconds)

LIVEKIT_API_KEY=<from livekit cloud>
LIVEKIT_API_SECRET=<from livekit cloud>
LIVEKIT_HOST=https://your-project.livekit.cloud

GOOGLE_CLIENT_ID=<from google cloud console>

SMTP_HOST=smtp.resend.com    # Resend has a free tier
SMTP_PORT=465
SMTP_USER=resend
SMTP_PASSWORD=<resend api key>
SMTP_FROM=noreply@yourdomain.com

S3_BUCKET=meno-uploads
S3_REGION=us-east-1
S3_ACCESS_KEY=<key>
S3_SECRET_KEY=<secret>
S3_ENDPOINT=<cloudflare R2 or backblaze for free egress>

FCM_SERVER_KEY=<from firebase console>
APP_ENV=development   # or production
PORT=8080
```

---

## Appendix B — Full Crate Dependency Summary

```toml
# apps/api/Cargo.toml
[dependencies]
# Web framework
axum            = { version = "0.7", features = ["ws", "multipart", "macros"] }
tower           = "0.4"
tower-http      = { version = "0.5", features = ["cors", "trace", "compression"] }

# Async runtime
tokio           = { version = "1", features = ["full"] }

# Database
sqlx            = { version = "0.8", features = ["postgres", "uuid", "time", "runtime-tokio", "migrate"] }

# Serialization
serde           = { version = "1", features = ["derive"] }
serde_json      = "1"

# Types
uuid            = { version = "1", features = ["v4", "serde"] }
time            = { version = "0.3", features = ["serde"] }

# Auth
argon2          = "0.5"
jsonwebtoken    = "9"
oauth2          = "4"

# Redis
fred            = { version = "9", features = ["tokio-runtime", "subscriber-client"] }

# Real-time
dashmap         = "5"

# LiveKit
livekit-api     = "0.3"

# Background jobs
apalis          = { version = "0.6", features = ["postgres"] }
apalis-sql      = "0.6"

# HTTP client (Google OAuth, FCM)
reqwest         = { version = "0.12", features = ["json", "rustls-tls"] }

# Email
lettre          = { version = "0.11", features = ["tokio1-rustls-tls", "builder"] }

# File storage
object_store    = { version = "0.10", features = ["aws"] }

# Observability
tracing              = "0.1"
tracing-subscriber   = { version = "0.3", features = ["env-filter", "json"] }
axum-prometheus      = "0.6"

# Error handling
thiserror       = "1"
anyhow          = "1"

# Config
dotenvy         = "0.15"
config          = "0.14"

# Utilities
rand            = "0.8"
chrono          = { version = "0.4", features = ["serde"] }  # for job scheduling
mime            = "0.3"
validator       = { version = "0.18", features = ["derive"] }
```

---

## Appendix C — Migration Sequencing (Recommended Order)

```
Phase 1 (Weeks 1-2):  Epic 1 (Mono-repo) + Epic 2 (DB migrations)
Phase 2 (Weeks 3-4):  Epic 3 (Axum skeleton) + Epic 4 (Auth)
Phase 3 (Weeks 5-6):  Epic 5 (Broadcasts) + Epic 7 (WebSockets)
Phase 4 (Week 7):     Epic 6 (Subscribers + Notifications) + Epic 8 (Chat)
Phase 5 (Week 8):     Epic 9 (Notes/Folders) + Epic 10 (Settings)
Phase 6 (Week 9):     Integration testing, load testing, cutover
Phase 7 (Future):     Epic 11 (Video + Restreaming)
```

> At the end of Phase 2, you can run the Rust API in parallel with the NestJS API (feature flags or reverse proxy split) to validate before full cutover.
