CREATE TABLE public.notes
(
    id         UUID PRIMARY KEY,
    title      VARCHAR(100)   NOT NULL DEFAULT '',
    content    VARCHAR(10000) NOT NULL DEFAULT '',
    pinned     BOOLEAN        NOT NULL DEFAULT false,
    folder_id  UUID           REFERENCES folders (id) ON DELETE SET NULL,
    creator_id UUID           NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    version    INT            NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ    NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ    NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);

CREATE INDEX idx_notes_creator ON notes (creator_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_notes_folder ON notes (folder_id) WHERE folder_id IS NOT NULL;
CREATE INDEX idx_notes_pinned ON notes (pinned) WHERE deleted_at IS NULL;
CREATE INDEX idx_notes_sync ON notes (creator_id, updated_at, id);
CREATE INDEX idx_notes_fts ON notes USING GIN (to_tsvector('english', title || ' ' || content))
    WHERE deleted_at IS NULL;

SELECT setup_updated_at_triggers();