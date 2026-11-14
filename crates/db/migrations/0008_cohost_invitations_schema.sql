CREATE TABLE public.cohost_invitations
(
    id           UUID PRIMARY KEY     DEFAULT gen_random_uuid(),
    broadcast_id UUID           NOT NULL REFERENCES public.broadcasts (id) ON UPDATE CASCADE ON DELETE CASCADE,
    inviter_id   UUID           NOT NULL REFERENCES public.users (id) ON UPDATE CASCADE ON DELETE CASCADE,
    invitee_id   UUID           NOT NULL REFERENCES public.users (id) ON UPDATE CASCADE ON DELETE CASCADE,
    status       TEXT           NOT NULL DEFAULT 'pending',
    created_at   TIMESTAMPTZ(3) NOT NULL DEFAULT now(),
    responded_at TIMESTAMPTZ(3),

    CONSTRAINT cohost_invitations_broadcast_invitee_key UNIQUE (broadcast_id, invitee_id),
    CONSTRAINT cohost_invitations_status_check CHECK (status IN ('pending', 'accepted', 'declined', 'expired')),
    CONSTRAINT cohost_invitations_no_self_invite CHECK (inviter_id <> invitee_id)
);

CREATE INDEX idx_cohost_invitations_invitee
    ON public.cohost_invitations (invitee_id, created_at DESC)
    WHERE status = 'pending';
