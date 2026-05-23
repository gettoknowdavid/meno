CREATE TABLE broadcast_bookmarks
(
    user_id      UUID        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    broadcast_id UUID        NOT NULL REFERENCES broadcasts (id) ON DELETE CASCADE,
    saved_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, broadcast_id)
);

CREATE INDEX idx_bookmarks_user ON broadcast_bookmarks (user_id, saved_at DESC);
