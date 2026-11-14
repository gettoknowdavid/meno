CREATE TABLE public.users
(
    id          UUID PRIMARY KEY        DEFAULT gen_random_uuid(),
    full_name   TEXT           NOT NULL,
    bio         VARCHAR(244),
    email       TEXT           NOT NULL,
    avatar_id   TEXT,
    avatar_url  TEXT,
    verified    BOOLEAN        NOT NULL DEFAULT false,
    role        TEXT           NOT NULL DEFAULT 'user',

    followers   BIGINT         NOT NULL DEFAULT 0,
    following   BIGINT         NOT NULL DEFAULT 0,
    broadcasts  BIGINT         NOT NULL DEFAULT 0,

    created_at  TIMESTAMPTZ(3) NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ(3) NOT NULL DEFAULT now(),
    deleted_at  TIMESTAMPTZ(3),

    search_vector tsvector GENERATED ALWAYS AS (
        to_tsvector('english', full_name || ' ' || coalesce(bio, ''))
        ) STORED,

    CONSTRAINT users_role_check CHECK (role IN ('user', 'admin')),
    CONSTRAINT users_followers_non_negative CHECK (followers >= 0),
    CONSTRAINT users_following_non_negative CHECK (following >= 0),
    CONSTRAINT users_broadcasts_non_negative CHECK (broadcasts >= 0)
);

-- Uniqueness is scoped to live rows so a soft-deleted account releases its email.
CREATE UNIQUE INDEX idx_users_email ON public.users (email) WHERE deleted_at IS NULL;
CREATE INDEX idx_users_deleted_at ON public.users (deleted_at) WHERE deleted_at IS NOT NULL;
CREATE INDEX idx_users_followers ON public.users (followers DESC) WHERE deleted_at IS NULL;
CREATE INDEX idx_users_broadcasts ON public.users (broadcasts DESC) WHERE deleted_at IS NULL;
CREATE INDEX idx_users_created_cursor ON public.users (created_at DESC, id DESC) WHERE deleted_at IS NULL;
CREATE INDEX idx_users_search ON public.users USING GIN (search_vector);
CREATE INDEX idx_users_full_name_trgm ON public.users USING GIN (full_name gin_trgm_ops);

CREATE TRIGGER users_set_updated_at
    BEFORE UPDATE ON public.users
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TABLE public.user_identities
(
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id          UUID        NOT NULL REFERENCES public.users (id) ON UPDATE CASCADE ON DELETE CASCADE,
    provider_type    TEXT        NOT NULL,
    provider_user_id TEXT        NOT NULL,
    password_hash    TEXT,
    created_at       TIMESTAMPTZ(3) NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ(3),

    CONSTRAINT user_identities_user_id_provider_type_key UNIQUE (user_id, provider_type),
    CONSTRAINT user_identities_provider_user_id_key UNIQUE (provider_type, provider_user_id),
    CONSTRAINT user_identities_provider_type_check CHECK (provider_type IN ('email', 'google', 'apple', 'facebook'))
);

CREATE TRIGGER user_identities_set_updated_at
    BEFORE UPDATE ON public.user_identities
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TABLE public.refresh_tokens
(
    id         UUID PRIMARY KEY        DEFAULT gen_random_uuid(),
    user_id    UUID           NOT NULL REFERENCES public.users (id) ON UPDATE CASCADE ON DELETE CASCADE,
    token_hash TEXT           NOT NULL UNIQUE,
    created_at TIMESTAMPTZ(3) NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ(3) NOT NULL
);

CREATE INDEX idx_refresh_tokens_user_id ON public.refresh_tokens (user_id);
CREATE INDEX idx_refresh_tokens_user_expires ON public.refresh_tokens (user_id, expires_at);
CREATE INDEX idx_refresh_tokens_expires_at ON public.refresh_tokens (expires_at) WHERE expires_at IS NOT NULL;

CREATE TABLE public.otps
(
    id         UUID PRIMARY KEY        DEFAULT gen_random_uuid(),
    email      TEXT           NOT NULL,
    code       TEXT           NOT NULL UNIQUE,
    type       TEXT           NOT NULL,
    used       BOOLEAN        NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ(3) NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ(3) NOT NULL,

    CONSTRAINT otps_type_check CHECK (type IN ('verify_email', 'reset_password'))
);

CREATE UNIQUE INDEX idx_otps_email_type ON public.otps (email, type);
