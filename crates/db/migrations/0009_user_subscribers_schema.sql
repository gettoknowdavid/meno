CREATE TABLE public.user_subscribers
(
    subscriber_id   UUID           NOT NULL REFERENCES public.users (id) ON UPDATE CASCADE ON DELETE CASCADE,
    subscription_id UUID           NOT NULL REFERENCES public.users (id) ON UPDATE CASCADE ON DELETE CASCADE,
    created_at      TIMESTAMPTZ(3) NOT NULL DEFAULT now(),

    PRIMARY KEY (subscriber_id, subscription_id),
    CONSTRAINT user_subscribers_no_self_follow CHECK (subscriber_id <> subscription_id)
);

CREATE INDEX idx_user_subscribers_followers_cursor
    ON public.user_subscribers (subscription_id, created_at DESC, subscriber_id DESC);
CREATE INDEX idx_user_subscribers_following_cursor
    ON public.user_subscribers (subscriber_id, created_at DESC, subscription_id DESC);

CREATE FUNCTION update_user_follow_counts() RETURNS TRIGGER AS
$$
BEGIN
    IF TG_OP = 'INSERT' THEN
        UPDATE public.users SET followers = followers + 1 WHERE id = NEW.subscription_id;
        UPDATE public.users SET following = following + 1 WHERE id = NEW.subscriber_id;
    ELSIF TG_OP = 'DELETE' THEN
        UPDATE public.users SET followers = followers - 1 WHERE id = OLD.subscription_id;
        UPDATE public.users SET following = following - 1 WHERE id = OLD.subscriber_id;
    END IF;

    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER user_subscribers_follow_counts
    AFTER INSERT OR DELETE ON public.user_subscribers
    FOR EACH ROW EXECUTE FUNCTION update_user_follow_counts();
