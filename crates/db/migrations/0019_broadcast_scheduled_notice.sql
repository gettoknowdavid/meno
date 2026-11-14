-- The scheduled-start reminder needs a once-only marker.
--
-- `start_time` alone cannot answer "has this broadcast's reminder gone out?", and a
-- worker restart would otherwise remind the same subscribers every minute. The column is
-- nullable: an unscheduled broadcast, or one whose moment has not arrived, has no value.

ALTER TABLE public.broadcasts
    ADD COLUMN IF NOT EXISTS scheduled_notified_at TIMESTAMPTZ(3);

-- The reminder sweep asks exactly this question: inactive, due, not yet told. A partial
-- index keeps the sweep proportional to due rows rather than to every broadcast.
CREATE INDEX IF NOT EXISTS broadcasts_scheduled_due_idx
    ON public.broadcasts (start_time)
    WHERE start_time IS NOT NULL AND scheduled_notified_at IS NULL;
