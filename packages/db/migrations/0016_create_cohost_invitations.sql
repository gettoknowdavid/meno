CREATE TABLE cohost_invitations
(
    id           UUID PRIMARY KEY     DEFAULT gen_random_uuid(),
    broadcast_id UUID        NOT NULL REFERENCES broadcasts (id) ON DELETE CASCADE,
    inviter_id   UUID        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    invitee_id   UUID        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    status       TEXT        NOT NULL DEFAULT 'pending',
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    responded_at TIMESTAMPTZ,
    UNIQUE (broadcast_id, invitee_id)
);