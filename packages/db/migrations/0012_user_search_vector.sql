CREATE EXTENSION IF NOT EXISTS pg_trgm;

ALTER TABLE public.users
    ADD COLUMN search_vector tsvector
        GENERATED ALWAYS AS (
            to_tsvector('english', COALESCE(full_name, '') || '' || COALESCE(bio, ''))
            ) STORED;

CREATE INDEX idx_users_search_vector ON public.users USING GIN (search_vector);
CREATE INDEX idx_users_full_name_trgm ON public.users USING GIN (full_name gin_trgm_ops);
