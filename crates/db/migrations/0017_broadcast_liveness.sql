-- Two things a broadcast needs and the schema could not express before:
--
-- 1. **One account, one broadcast on air.** Nothing stopped a creator from having
--    several rows in `active` at once — two editor sessions, a retry, or a client
--    that lost the answer to its own go-live all produced a second live broadcast.
--    A partial unique index is the only place that can be true of every writer,
--    including two replicas racing the same request.
--
-- 2. **When a broadcast first met an end condition.** The reaper ends a broadcast
--    nobody is in, or whose host has not come back, only after a grace period; the
--    clock has to survive a restart of the process that started it, so it lives here
--    rather than in a job's memory.

-- Repair first: a single creator with more than one live row makes the unique index
-- impossible to build, and leaving the extra rows live is what the index exists to
-- stop. The newest row (by publish time) per creator stays on air; the rest are
-- closed as forced ends, which is an honest description of what happened to them.
UPDATE public.broadcasts AS b
SET status     = 'inactive',
    end_time   = COALESCE(b.end_time, now()),
    end_reason = COALESCE(b.end_reason, 'admin_forced'),
    updated_at = now()
WHERE b.status = 'active'
  AND b.deleted_at IS NULL
  AND b.id NOT IN (
      SELECT DISTINCT ON (live.creator_id) live.id
      FROM public.broadcasts AS live
      WHERE live.status = 'active'
        AND live.deleted_at IS NULL
      ORDER BY live.creator_id, live.published_at DESC NULLS LAST, live.created_at DESC, live.id
  );

CREATE UNIQUE INDEX idx_broadcasts_one_live_per_creator
    ON public.broadcasts (creator_id)
    WHERE status = 'active' AND deleted_at IS NULL;

-- Null while the broadcast is in a healthy state; set to the moment it first became
-- abandonable (nobody in the room, or no host and no cohost) and cleared when somebody
-- comes back. The reaper compares it against now() rather than counting down in memory.
ALTER TABLE public.broadcasts
    ADD COLUMN abandoned_since TIMESTAMPTZ(3);

COMMENT ON COLUMN public.broadcasts.abandoned_since IS
    'When the broadcast first had nobody in its room, or no host and no cohost. Cleared when that stops being true.';
