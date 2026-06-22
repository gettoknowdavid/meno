CREATE TABLE public.broadcasts
(
    id                 UUID PRIMARY KEY     DEFAULT gen_random_uuid(),
    title              TEXT        NOT NULL,
    description        VARCHAR(244),
    image_url          TEXT,
    image_id           TEXT,
    broadcast_token    TEXT,
    status             TEXT        NOT NULL DEFAULT 'inactive' CHECK ( status IN ('inactive', 'active', 'ended') ),
    is_draft           BOOLEAN     NOT NULL DEFAULT FALSE,
    start_time         TIMESTAMPTZ,
    end_time           TIMESTAMPTZ,
    time_zone          TEXT                 DEFAULT 'Africa/Lagos',

    creator_id         UUID        NOT NULL REFERENCES users (id) ON UPDATE CASCADE ON DELETE CASCADE,

    total_participants BIGINT      NOT NULL DEFAULT 0,

    recording_enabled  BOOLEAN     NOT NULL DEFAULT false,
    recording_key      TEXT,
    recording_url      TEXT,
    published_at       TIMESTAMPTZ,
    end_reason         TEXT CHECK ( end_reason IN ('normal', 'host_disconnected', 'admin_forced', 'quota_exceeded') ),

    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at         TIMESTAMPTZ
);

CREATE INDEX idx_broadcasts_creator_id ON public.broadcasts (creator_id);
CREATE INDEX idx_broadcasts_status ON public.broadcasts (status);
CREATE INDEX idx_broadcasts_start_time ON public.broadcasts (start_time) WHERE start_time IS NOT NULL;
CREATE INDEX idx_broadcasts_end_time ON public.broadcasts (end_time) WHERE end_time IS NOT NULL;

-- Full-text search on title + description
CREATE INDEX idx_broadcasts_fts_title_description ON broadcasTS
    USING GIN (to_tsvector('english', title || '' || description));

SELECT setup_updated_at_triggers();

CREATE OR REPLACE FUNCTION update_broadcast_participant_count() RETURNS TRIGGER AS
$$
BEGIN
    IF TG_OP = 'INSERT' THEN
        UPDATE broadcasts SET total_participants = total_participants + 1 WHERE id = NEW.broadcast_id;
    ELSIF TG_OP = 'DELETE' THEN
        UPDATE broadcasts SET total_participants = total_participants - 1 WHERE id = OLD.broadcast_id;
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_broadcast_participant_count
    AFTER INSERT OR DELETE
    ON broadcast_participants
    FOR EACH ROW
EXECUTE FUNCTION update_broadcast_participant_count();