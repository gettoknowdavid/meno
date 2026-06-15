CREATE TABLE public.chat_messages
(
    id           UUID PRIMARY KEY      DEFAULT gen_random_uuid(),
    content      VARCHAR(256) NOT NULL,
    sender_id    UUID         NOT NULL REFERENCES users (id) ON UPDATE CASCADE ON DELETE CASCADE,
    broadcast_id UUID         NOT NULL REFERENCES broadcasts (id) ON UPDATE CASCADE ON DELETE CASCADE,

    created_at   TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ,
    deleted_at   TIMESTAMPTZ
);

CREATE INDEX idx_chat_messages_broadcast ON chat_messages (broadcast_id, created_at DESC) WHERE deleted_at IS NULL;
CREATE INDEX idx_users_deleted ON users(id) WHERE deleted_at IS NULL;

SELECT setup_updated_at_triggers();

CREATE TABLE public.chat_reactions
(
    id           UUID PRIMARY KEY     DEFAULT gen_random_uuid(),
    content      VARCHAR(32) NOT NULL,
    sender_id    UUID        NOT NULL REFERENCES users (id) ON UPDATE CASCADE ON DELETE CASCADE,
    broadcast_id UUID        NOT NULL REFERENCES broadcasts (id) ON UPDATE CASCADE ON DELETE CASCADE,

    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_chat_reactions_broadcast ON chat_reactions (broadcast_id, created_at DESC);
