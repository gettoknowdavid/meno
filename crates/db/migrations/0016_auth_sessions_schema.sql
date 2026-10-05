-- Device-bound refresh sessions (plan §4.7 items 1-3).
--
-- `refresh_tokens` (0003) records *that* a refresh token was issued. It does not record
-- what it was issued *to*, so there is nothing to revoke per device and nothing to
-- compare a replayed token against. This table supplies both.
--
-- The security property §4.7 item 2 buys is reuse detection, and it rests on one column:
-- `rotated_from`. When a refresh succeeds the row is *replaced*, with the new row
-- pointing back at the one it supersedes. A token whose `jti` is no longer the live
-- `refresh_jti` of any session was therefore already rotated once — which is exactly
-- what happens when an attacker steals a token and races the legitimate client. The
-- service responds by revoking every session for that user.

CREATE TABLE public.auth_sessions
(
    id            UUID PRIMARY KEY        DEFAULT gen_random_uuid(),
    user_id       UUID           NOT NULL REFERENCES public.users (id) ON UPDATE CASCADE ON DELETE CASCADE,
    refresh_jti   UUID           NOT NULL,
    device_label  VARCHAR(120),
    user_agent    TEXT,
    ip            INET,
    created_at    TIMESTAMPTZ(3) NOT NULL DEFAULT now(),
    last_used_at  TIMESTAMPTZ(3) NOT NULL DEFAULT now(),
    revoked_at    TIMESTAMPTZ(3),
    -- NULL for the first token of a session; the superseded session's jti thereafter.
    rotated_from  UUID,

    CONSTRAINT auth_sessions_refresh_jti_key UNIQUE (refresh_jti),
    -- `revoked_at IS NULL` is what "live" means, so the partial unique index is what
    -- makes "one live session per jti" a database invariant rather than a convention
    -- the service has to remember to check inside every transaction.
    CONSTRAINT auth_sessions_rotated_from_key UNIQUE (rotated_from)
);

-- Rotation looks up by jti; revocation looks up by user. Both are hot paths.
CREATE INDEX idx_auth_sessions_user_live ON public.auth_sessions (user_id) WHERE revoked_at IS NULL;
CREATE INDEX idx_auth_sessions_user_rotated ON public.auth_sessions (user_id, rotated_from);
-- "Log out all devices" reads every session for a user newest-first.
CREATE INDEX idx_auth_sessions_user_created ON public.auth_sessions (user_id, created_at DESC);

-- Reuse detection is "is this jti anywhere in the table, live or rotated". Without the
-- index that degrades into a scan per refresh, which is the single most frequent
-- authenticated write in the system.
CREATE INDEX idx_auth_sessions_jti ON public.auth_sessions (refresh_jti);

-- `refresh_tokens` (0003) stores the SHA-256 of the signed token but never the `jti`
-- from inside it, so every lookup had to re-hash the whole token to find its own row.
-- Carrying the `jti` alongside the hash makes rotation and reuse detection a keyed
-- lookup on an indexed UUID instead of a full scan of the token text.
--
-- Nullable because `0003` is already applied on deployed databases: existing rows have
-- no `jti` to backfill. The service treats a row with a NULL `jti` as not found, which
-- means a session created before this migration must re-authenticate once. Adding the
-- column NOT NULL would have required dropping the constraint table-wide in one step.
ALTER TABLE public.refresh_tokens ADD COLUMN jti UUID;

-- `revoke_refresh_token` marks a token dead rather than deleting it, so the
-- row survives as an audit trail (a deleted row cannot answer "was this token
-- real?", which is the question reuse detection asks). `pg.rs` already wrote
-- `revoked_at` here; nothing created it, so every logout against Postgres
-- failed with a missing-column error.
--
-- Nullable so `0003`'s existing rows stay valid without a backfill: a token with
-- a NULL `revoked_at` is live, which is the same reading the partial index uses.
ALTER TABLE public.refresh_tokens ADD COLUMN revoked_at TIMESTAMPTZ(3);

-- "Is this jti live?" is checked on every refresh and every logout.
CREATE INDEX idx_refresh_tokens_jti_live ON public.refresh_tokens (jti) WHERE revoked_at IS NULL;
CREATE UNIQUE INDEX idx_refresh_tokens_jti ON public.refresh_tokens (jti) WHERE jti IS NOT NULL;

-- `revoke_all_sessions` is what reuse detection fires. It must not be able to miss a row
-- it has not revoked, so the sweeper that prunes expired sessions is a separate concern
-- (§4.5's background sweeper) and not part of this table's contract.