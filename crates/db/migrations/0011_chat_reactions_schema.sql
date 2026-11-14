CREATE TABLE public.chat_reactions
(
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    content      VARCHAR(32)  NOT NULL,
    sender_id    UUID        NOT NULL REFERENCES public.users (id) ON UPDATE CASCADE ON DELETE CASCADE,
    broadcast_id UUID        NOT NULL REFERENCES public.broadcasts (id) ON UPDATE CASCADE ON DELETE CASCADE,
    created_at   TIMESTAMPTZ(3) NOT NULL DEFAULT now()
);

CREATE INDEX idx_chat_reactions_cursor
    ON public.chat_reactions (broadcast_id, created_at DESC, id DESC);
