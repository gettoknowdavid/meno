CREATE TABLE public.broadcast_cohosts
(
    broadcast_id UUID        NOT NULL REFERENCES broadcasts (id) ON UPDATE CASCADE ON DELETE CASCADE,
    cohost_id    UUID        NOT NULL REFERENCES users (id) ON UPDATE CASCADE ON DELETE CASCADE,
    invited_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    invited_by   UUID        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    removed_at   TIMESTAMPTZ,

    PRIMARY KEY (broadcast_id, cohost_id)
);