CREATE TABLE public.broadcast_bookmarks
(
    user_id      UUID           NOT NULL REFERENCES public.users (id) ON UPDATE CASCADE ON DELETE CASCADE,
    broadcast_id UUID           NOT NULL REFERENCES public.broadcasts (id) ON UPDATE CASCADE ON DELETE CASCADE,
    saved_at     TIMESTAMPTZ(3) NOT NULL DEFAULT now(),

    PRIMARY KEY (user_id, broadcast_id)
);

CREATE INDEX idx_broadcast_bookmarks_cursor
    ON public.broadcast_bookmarks (user_id, saved_at DESC, broadcast_id DESC);
