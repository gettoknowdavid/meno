CREATE TABLE public.user_blocks
(
    blocker_id UUID           NOT NULL REFERENCES public.users (id) ON UPDATE CASCADE ON DELETE CASCADE,
    blocked_id UUID           NOT NULL REFERENCES public.users (id) ON UPDATE CASCADE ON DELETE CASCADE,
    created_at TIMESTAMPTZ(3) NOT NULL DEFAULT now(),

    PRIMARY KEY (blocker_id, blocked_id),
    CONSTRAINT user_blocks_no_self_block CHECK (blocker_id <> blocked_id)
);

-- The direction a read asks about far less often than "who did this person block": the
-- follow and chat checks ask "has the owner blocked me", which is a lookup by
-- `blocked_id`.
CREATE INDEX idx_user_blocks_blocked ON public.user_blocks (blocked_id);

CREATE TABLE public.moderation_reports
(
    id           UUID           NOT NULL DEFAULT gen_random_uuid(),
    reporter_id  UUID           NOT NULL REFERENCES public.users (id) ON UPDATE CASCADE ON DELETE CASCADE,
    subject_type TEXT           NOT NULL,
    subject_id   UUID           NOT NULL,
    reason       TEXT           NOT NULL,
    details      TEXT,
    status       TEXT           NOT NULL DEFAULT 'open',
    resolved_at  TIMESTAMPTZ(3),
    created_at   TIMESTAMPTZ(3) NOT NULL DEFAULT now(),

    PRIMARY KEY (id),
    -- One table for both kinds of subject, so an operator reads one queue. The CHECK is
    -- what keeps that honest: a typo in `subject_type` is refused by the database rather
    -- than becoming a row no query of either kind will find.
    CONSTRAINT moderation_reports_subject_type CHECK (subject_type IN ('user', 'broadcast')),
    CONSTRAINT moderation_reports_status CHECK (status IN ('open', 'reviewed', 'dismissed')),
    CONSTRAINT moderation_reports_resolution CHECK (
        (status = 'open' AND resolved_at IS NULL)
            OR (status <> 'open' AND resolved_at IS NOT NULL)
        )
);

CREATE INDEX idx_moderation_reports_queue
    ON public.moderation_reports (status, created_at DESC, id DESC);
