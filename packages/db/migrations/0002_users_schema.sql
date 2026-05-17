-- Users
CREATE TABLE public.users
(
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    full_name        TEXT    NOT NULL,
    bio              TEXT,
    email            TEXT    NOT NULL UNIQUE,
    password         TEXT    NOT NULL,
    avatar_id        TEXT,
    avatar_url       TEXT,
    verified         BOOLEAN NOT NULL DEFAULT FALSE,
    role             TEXT    NOT NULL DEFAULT 'user' CHECK ( role IN ('user', 'admin') ),
    account_provider TEXT    NOT NULL CHECK ( account_provider IN ('email', 'google', 'apple', 'facebook') ),
    created_at       TIMESTAMPTZ(3) NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ(3) NOT NULL DEFAULT now(),
    deleted_at       TIMESTAMPTZ(3)
);

CREATE INDEX idx_users_email ON users (email);
CREATE INDEX idx_users_deleted_at ON users (deleted_at) WHERE deleted_at IS NULL;

-- Refresh tokens
CREATE TABLE public.refresh_tokens
(
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ(3) NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ(3) NOT NULL
);

CREATE INDEX idx_refresh_tokens_jti ON refresh_tokens (id);
CREATE INDEX idx_refresh_tokens_user_id ON refresh_tokens (user_id);
CREATE INDEX idx_refresh_tokens_expires_at ON refresh_tokens (expires_at);

-- OTP
CREATE TABLE public.otps
(
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email      TEXT    NOT NULL,
    code       TEXT    NOT NULL UNIQUE,
    type       TEXT    NOT NULL CHECK ( type IN ('verify_email', 'reset_password') ),
    used       BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ(3) NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ(3) NOT NULL,

    CONSTRAINT otps_email_type_unique UNIQUE (email, type)
);

CREATE INDEX idx_otps_email ON otps (email);

SELECT setup_updated_at_triggers();
