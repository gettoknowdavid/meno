CREATE TABLE public.folders
(
    id         UUID PRIMARY KEY     DEFAULT gen_random_uuid(),
    title      VARCHAR(100) NOT NULL,
    pinned     BOOLEAN      NOT NULL DEFAULT false,
    creator_id UUID         NOT NULL REFERENCES public.users (id) ON UPDATE CASCADE ON DELETE CASCADE,
    version    INT          NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ(3) NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ(3) NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ(3)
);

CREATE INDEX idx_folders_creator ON public.folders (creator_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_folders_pinned ON public.folders (pinned) WHERE deleted_at IS NULL;
CREATE INDEX idx_folders_sync ON public.folders (creator_id, updated_at, id);
CREATE INDEX idx_folders_cursor ON public.folders (creator_id, pinned DESC, created_at DESC, id DESC)
    WHERE deleted_at IS NULL;
CREATE INDEX idx_folders_deleted_at ON public.folders (deleted_at) WHERE deleted_at IS NOT NULL;

CREATE TRIGGER folders_set_updated_at
    BEFORE UPDATE ON public.folders
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
