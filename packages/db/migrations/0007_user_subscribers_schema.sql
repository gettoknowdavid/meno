CREATE TABLE public.user_subscribers
(
    subscriber_id   UUID        NOT NULL REFERENCES users (id) ON UPDATE CASCADE ON DELETE CASCADE,
    subscription_id UUID        NOT NULL REFERENCES users (id) ON UPDATE CASCADE ON DELETE CASCADE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (subscriber_id, subscription_id),
    CHECK ( subscriber_id != subscription_id )
);

CREATE INDEX idx_user_subscribers_subscription ON user_subscribers (subscription_id);
