CREATE TABLE public.broadcast_cohosts
(
    broadcast_id UUID        NOT NULL REFERENCES public.broadcasts (id) ON UPDATE CASCADE ON DELETE CASCADE,
    cohost_id    UUID        NOT NULL REFERENCES public.users (id) ON UPDATE CASCADE ON DELETE CASCADE,
    invited_by   UUID        NOT NULL REFERENCES public.users (id) ON UPDATE CASCADE ON DELETE CASCADE,
    invited_at   TIMESTAMPTZ(3) NOT NULL DEFAULT now(),
    removed_at   TIMESTAMPTZ(3),

    PRIMARY KEY (broadcast_id, cohost_id)
);

CREATE INDEX idx_broadcast_cohosts_cohost ON public.broadcast_cohosts (cohost_id) WHERE removed_at IS NULL;
