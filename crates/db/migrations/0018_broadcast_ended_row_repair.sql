-- A broadcast that has ended is not startable again, and a live row always has an end
-- to come.
--
-- Until now `mark_live` set `status = 'active'` without looking at `end_time`, so
-- re-opening an ended row produced a row that was live to the catalogue (`status`) and
-- ended to everything that reads `end_time` — `state`, `end`, `resume`, `token` — and
-- that `find_live_by_creator` treated as the creator's one on-air slot. The service now
-- refuses to start such a row; the update below closes the ones already written, and the
-- constraint keeps the shape from being written again by any writer, including a future
-- query that forgets the guard.
--
-- The status is set to `inactive` rather than `ended` on purpose: `inactive` + `end_time`
-- is the pair every read path in this API already understands, and `ended` exists only so
-- a row written by something else still decodes.

UPDATE public.broadcasts
SET status     = 'inactive',
    updated_at = now()
WHERE status = 'active'
  AND end_time IS NOT NULL
  AND deleted_at IS NULL;

-- A live row cannot carry an ending. Written as a named constraint so a violation names
-- itself, instead of surfacing as whichever query happened to read the row first.
ALTER TABLE public.broadcasts
    ADD CONSTRAINT broadcasts_live_has_no_end_check
        CHECK (NOT (status = 'active' AND end_time IS NOT NULL));

COMMENT ON CONSTRAINT broadcasts_live_has_no_end_check ON public.broadcasts IS
    'A broadcast is one event: once it has an end_time it is not live, and starting it again is refused.';
