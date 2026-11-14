CREATE TABLE public.notes
(
    id         UUID PRIMARY KEY     DEFAULT gen_random_uuid(),
    title      VARCHAR(100)   NOT NULL DEFAULT '',
    content    VARCHAR(10000) NOT NULL DEFAULT '',
    pinned     BOOLEAN        NOT NULL DEFAULT false,
    folder_id  UUID           REFERENCES public.folders (id) ON UPDATE CASCADE ON DELETE SET NULL,
    creator_id UUID           NOT NULL REFERENCES public.users (id) ON UPDATE CASCADE ON DELETE CASCADE,
    version    INT            NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ(3) NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ(3) NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ(3)
);

CREATE INDEX idx_notes_creator ON public.notes (creator_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_notes_pinned ON public.notes (pinned) WHERE deleted_at IS NULL;
CREATE INDEX idx_notes_folder ON public.notes (folder_id, updated_at DESC, id DESC)
    WHERE folder_id IS NOT NULL AND deleted_at IS NULL;
CREATE INDEX idx_notes_sync ON public.notes (creator_id, updated_at, id);
CREATE INDEX idx_notes_cursor_pinned ON public.notes (creator_id, pinned DESC, updated_at DESC, id DESC)
    WHERE deleted_at IS NULL;
CREATE INDEX idx_notes_search ON public.notes
    USING GIN (to_tsvector('english', title || ' ' || content))
    WHERE deleted_at IS NULL;
CREATE INDEX idx_notes_deleted_at ON public.notes (deleted_at) WHERE deleted_at IS NOT NULL;

CREATE TRIGGER notes_set_updated_at
    BEFORE UPDATE ON public.notes
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
