CREATE TABLE public.broadcasts
(
    id                 UUID PRIMARY KEY     DEFAULT gen_random_uuid(),
    title              TEXT        NOT NULL,
    description        VARCHAR(244),
    image_url          TEXT,
    image_id           TEXT,
    broadcast_token    TEXT,
    status             TEXT        NOT NULL DEFAULT 'inactive',
    is_draft           BOOLEAN     NOT NULL DEFAULT false,
    start_time         TIMESTAMPTZ,
    end_time           TIMESTAMPTZ,
    time_zone          TEXT                 DEFAULT 'Africa/Lagos',

    creator_id         UUID        NOT NULL REFERENCES public.users (id) ON UPDATE CASCADE ON DELETE CASCADE,
    total_participants BIGINT      NOT NULL DEFAULT 0,

    recording_enabled  BOOLEAN     NOT NULL DEFAULT false,
    recording_key      TEXT,
    recording_url      TEXT,
    published_at       TIMESTAMPTZ,
    end_reason         TEXT,

    created_at         TIMESTAMPTZ(3) NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ(3) NOT NULL DEFAULT now(),
    deleted_at         TIMESTAMPTZ(3),

    CONSTRAINT broadcasts_status_check CHECK (status IN ('inactive', 'active', 'ended')),
    CONSTRAINT broadcasts_end_reason_check CHECK (end_reason IS NULL OR
                                                   end_reason IN ('normal', 'host_disconnected', 'admin_forced', 'quota_exceeded')),
    CONSTRAINT broadcasts_total_participants_non_negative CHECK (total_participants >= 0)
);

CREATE INDEX idx_broadcasts_creator_id ON public.broadcasts (creator_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_broadcasts_start_time ON public.broadcasts (start_time) WHERE start_time IS NOT NULL;
CREATE INDEX idx_broadcasts_end_time ON public.broadcasts (end_time) WHERE end_time IS NOT NULL;

CREATE INDEX idx_broadcasts_cursor ON public.broadcasts (created_at DESC, id DESC) WHERE deleted_at IS NULL;
CREATE INDEX idx_broadcasts_creator_cursor ON public.broadcasts (creator_id, created_at DESC, id DESC)
    WHERE deleted_at IS NULL;
CREATE INDEX idx_broadcasts_status_cursor ON public.broadcasts (status, created_at DESC, id DESC)
    WHERE deleted_at IS NULL;
CREATE INDEX idx_broadcasts_creator_status_cursor ON public.broadcasts (creator_id, status, created_at DESC, id DESC)
    WHERE deleted_at IS NULL;
CREATE INDEX idx_broadcasts_ended_cursor ON public.broadcasts (end_time DESC NULLS LAST, id DESC)
    WHERE status = 'ended' AND deleted_at IS NULL;
CREATE INDEX idx_broadcasts_scheduled_cursor ON public.broadcasts (start_time ASC NULLS LAST, id ASC)
    WHERE status = 'inactive' AND deleted_at IS NULL;
CREATE INDEX idx_broadcasts_search ON public.broadcasts
    USING GIN (to_tsvector('english', title || ' ' || coalesce(description, '')))
    WHERE deleted_at IS NULL;

CREATE TRIGGER broadcasts_set_updated_at
    BEFORE UPDATE ON public.broadcasts
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE FUNCTION update_user_broadcast_count() RETURNS TRIGGER AS
$$
BEGIN
    IF TG_OP = 'INSERT' THEN
        IF NEW.deleted_at IS NULL THEN
            UPDATE public.users SET broadcasts = broadcasts + 1 WHERE id = NEW.creator_id;
        END IF;

    ELSIF TG_OP = 'UPDATE' THEN
        IF OLD.deleted_at IS NULL AND NEW.deleted_at IS NOT NULL THEN
            UPDATE public.users SET broadcasts = broadcasts - 1 WHERE id = OLD.creator_id;
        ELSIF OLD.deleted_at IS NOT NULL AND NEW.deleted_at IS NULL THEN
            UPDATE public.users SET broadcasts = broadcasts + 1 WHERE id = NEW.creator_id;
        ELSIF OLD.creator_id IS DISTINCT FROM NEW.creator_id THEN
            UPDATE public.users SET broadcasts = broadcasts - 1 WHERE id = OLD.creator_id;
            UPDATE public.users SET broadcasts = broadcasts + 1 WHERE id = NEW.creator_id;
        END IF;

    ELSIF TG_OP = 'DELETE' THEN
        IF OLD.deleted_at IS NULL THEN
            UPDATE public.users SET broadcasts = broadcasts - 1 WHERE id = OLD.creator_id;
        END IF;
    END IF;

    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER broadcasts_count
    AFTER INSERT OR UPDATE OF deleted_at, creator_id OR DELETE ON public.broadcasts
    FOR EACH ROW EXECUTE FUNCTION update_user_broadcast_count();
