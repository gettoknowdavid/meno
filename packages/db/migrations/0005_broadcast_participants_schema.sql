CREATE TABLE public.broadcast_participants
(
    broadcast_id   UUID        NOT NULL REFERENCES broadcasts (id) ON UPDATE CASCADE ON DELETE CASCADE,
    participant_id UUID        NOT NULL REFERENCES users (id) ON UPDATE CASCADE ON DELETE CASCADE,
    role           TEXT        NOT NULL DEFAULT 'participant' CHECK ( role IN ('host', 'cohost', 'participant') ),
    joined_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (broadcast_id, participant_id)
);