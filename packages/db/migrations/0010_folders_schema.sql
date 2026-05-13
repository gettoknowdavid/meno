CREATE TABLE public.folders
(
    id         UUID PRIMARY KEY      DEFAULT gen_random_uuid(),
    title      VARCHAR(100) NOT NULL,

    pinned     BOOLEAN      NOT NULL DEFAULT FALSE,

    creator_id UUID REFERENCES users (id) ON UPDATE CASCADE ON DELETE CASCADE,

    created_at TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ,
    deleted_at TIMESTAMPTZ
);

CREATE INDEX idx_folders_creator ON folders (creator_id);
CREATE INDEX idx_folders_pinned ON folders (pinned);

SELECT setup_updated_at_triggers();