CREATE TABLE public.chat_messages
(
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    content      VARCHAR(256) NOT NULL,
    sender_id    UUID        NOT NULL REFERENCES public.users (id) ON UPDATE CASCADE ON DELETE CASCADE,
    broadcast_id UUID        NOT NULL REFERENCES public.broadcasts (id) ON UPDATE CASCADE ON DELETE CASCADE,
    created_at   TIMESTAMPTZ(3) NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ(3),
    deleted_at   TIMESTAMPTZ(3)
);

CREATE INDEX idx_chat_messages_cursor_desc
    ON public.chat_messages (broadcast_id, created_at DESC, id DESC)
    WHERE deleted_at IS NULL;
CREATE INDEX idx_chat_messages_cursor_asc
    ON public.chat_messages (broadcast_id, created_at ASC, id ASC)
    WHERE deleted_at IS NULL;
CREATE INDEX idx_chat_messages_deleted_at
    ON public.chat_messages (deleted_at) WHERE deleted_at IS NOT NULL;

CREATE TRIGGER chat_messages_set_updated_at
    BEFORE UPDATE ON public.chat_messages
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
