CREATE TABLE public.notes
(
    id         UUID PRIMARY KEY        DEFAULT gen_random_uuid(),
    title      VARCHAR(100)   NOT NULL,
    content    VARCHAR(10000) NOT NULL,

    pinned     BOOLEAN        NOT NULL DEFAULT FALSE,

    folder_id  UUID           REFERENCES folders (id) ON DELETE SET NULL,
    creator_id UUID REFERENCES users (id) ON UPDATE CASCADE ON DELETE CASCADE,

    created_at TIMESTAMPTZ    NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ,
    deleted_at TIMESTAMPTZ
);

CREATE INDEX idx_notes_creator ON notes (creator_id);
CREATE INDEX idx_notes_folder ON notes (folder_id) WHERE folder_id IS NOT NULL;
CREATE INDEX idx_notes_pinned ON notes (pinned);
CREATE INDEX idx_notes_fts ON notes USING GIN (to_tsvector('english', title || ' ' || content));

SELECT setup_updated_at_triggers();