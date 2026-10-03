CREATE TABLE public.broadcast_participants
(
    broadcast_id                 UUID        NOT NULL REFERENCES public.broadcasts (id) ON UPDATE CASCADE ON DELETE CASCADE,
    participant_id               UUID        NOT NULL REFERENCES public.users (id) ON UPDATE CASCADE ON DELETE CASCADE,
    role                         TEXT        NOT NULL DEFAULT 'participant',
    joined_at                    TIMESTAMPTZ(3) NOT NULL DEFAULT now(),
    left_at                      TIMESTAMPTZ(3),
    last_listen_position_seconds INT         NOT NULL DEFAULT 0,
    last_listened_at             TIMESTAMPTZ(3),

    PRIMARY KEY (broadcast_id, participant_id),
    CONSTRAINT broadcast_participants_role_check CHECK (role IN ('host', 'cohost', 'participant', 'none'))
);

CREATE INDEX idx_broadcast_participants_cursor
    ON public.broadcast_participants (broadcast_id, joined_at DESC, participant_id DESC);
CREATE INDEX idx_broadcast_participants_active
    ON public.broadcast_participants (broadcast_id) WHERE left_at IS NULL;

CREATE FUNCTION update_broadcast_participant_count() RETURNS TRIGGER AS
$$
BEGIN
    IF TG_OP = 'INSERT' THEN
        UPDATE public.broadcasts SET total_participants = total_participants + 1 WHERE id = NEW.broadcast_id;
    ELSIF TG_OP = 'DELETE' THEN
        UPDATE public.broadcasts SET total_participants = total_participants - 1 WHERE id = OLD.broadcast_id;
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER broadcast_participants_count
    AFTER INSERT OR DELETE ON public.broadcast_participants
    FOR EACH ROW EXECUTE FUNCTION update_broadcast_participant_count();
