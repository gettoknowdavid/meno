CREATE TABLE public.broadcasts
(
    id          UUID PRIMARY KEY      DEFAULT gen_random_uuid(),
    title       TEXT         NOT NULL,
    description VARCHAR(244) NOT NULL,
    image_url   TEXT,
    image_id    TEXT,
    status      TEXT         NOT NULL DEFAULT 'inactive' CHECK ( status IN ('inactive', 'active', 'ended') ),

    start_time  TIMESTAMPTZ,
    end_time    TIMESTAMPTZ,
    time_zone   TEXT         NOT NULL DEFAULT 'Africa/Lagos',

    creator_id  UUID         NOT NULL REFERENCES users (id) ON UPDATE CASCADE ON DELETE CASCADE,

    created_at  TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ  NOT NULL DEFAULT now(),
    deleted_at  TIMESTAMPTZ
);

CREATE INDEX idx_broadcasts_creator_id ON public.broadcasts (creator_id);
CREATE INDEX idx_broadcasts_status ON public.broadcasts (status);
CREATE INDEX idx_broadcasts_start_time ON public.broadcasts (start_time) WHERE start_time IS NOT NULL;
CREATE INDEX idx_broadcasts_end_time ON public.broadcasts (end_time) WHERE end_time IS NOT NULL;

-- Full-text search on title + description
CREATE INDEX idx_broadcasts_fts_title_description ON broadcasTS
    USING GIN (to_tsvector('english', title || '' || description));

SELECT setup_updated_at_triggers();