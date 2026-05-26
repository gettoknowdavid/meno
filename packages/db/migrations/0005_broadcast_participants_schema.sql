CREATE TABLE public.broadcast_participants
(
    broadcast_id                 UUID        NOT NULL REFERENCES broadcasts (id) ON UPDATE CASCADE ON DELETE CASCADE,
    participant_id               UUID        NOT NULL REFERENCES users (id) ON UPDATE CASCADE ON DELETE CASCADE,
    role                         TEXT        NOT NULL DEFAULT 'participant' CHECK ( role IN ('host', 'cohost', 'participant', 'none') ),
    joined_at                    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    left_at                      TIMESTAMPTZ,
    last_listen_position_seconds INT         NOT NULL DEFAULT 0,
    last_listened_at             TIMESTAMPTZ,
    PRIMARY KEY (broadcast_id, participant_id)
);