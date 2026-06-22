CREATE TABLE public.folders
(
    id         UUID PRIMARY KEY,
    title      VARCHAR(100) NOT NULL,
    pinned     BOOLEAN      NOT NULL DEFAULT false,
    creator_id UUID         NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    version    INT          NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ  NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);

CREATE INDEX idx_folders_creator ON folders (creator_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_folders_pinned ON folders (pinned) WHERE deleted_at IS NULL;
CREATE INDEX idx_folders_sync ON folders (creator_id, updated_at, id);

SELECT setup_updated_at_triggers();