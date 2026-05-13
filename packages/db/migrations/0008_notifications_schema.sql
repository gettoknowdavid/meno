-- Small lookup table for the notification types:
-- 1. added_as_cohost
-- 2. user_subscribed
-- 3. scheduled_broadcast
-- 4. live_broadcast_started
-- 5. broadcast_ended
CREATE TABLE public.notification_types
(
    code        TEXT PRIMARY KEY,     -- e.g. "added_as_cohost"
    label       TEXT        NOT NULL, -- e.g. "Added as co-host"
    description TEXT,                 -- e.g. "You have been added as a co-host to a broadcast"
    icon        TEXT,                 -- e.g. "user-plus"
    color       TEXT,                 -- e.g. "#22C55E"
    createdAt   TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Notification Templates
CREATE TABLE public.notification_templates
(
    id        UUID PRIMARY KEY     DEFAULT gen_random_uuid(),
    type      TEXT        NOT NULL REFERENCES notification_types (code) ON DELETE RESTRICT ON UPDATE CASCADE,

    title     TEXT        NOT NULL,
    body      TEXT        NOT NULL,
    image_url TEXT,

    metadata  JSONB       NOT NULL DEFAULT '{}',
    is_active BOOLEAN     NOT NULL DEFAULT TRUE,

    createdAt TIMESTAMPTZ NOT NULL DEFAULT now(),
    updatedAt TIMESTAMPTZ NOT NULL DEFAULT now(),
    deletedAt TIMESTAMPTZ
);
CREATE INDEX idx_notification_templates_type ON notification_templates (type);

-- User Notifications (Per user instance - lightweight)
CREATE TABLE public.notifications
(
    id              UUID PRIMARY KEY     DEFAULT gen_random_uuid(),
    owner_id        UUID        NOT NULL REFERENCES users (id) ON UPDATE CASCADE ON DELETE CASCADE,
    template_id     UUID        NOT NULL REFERENCES notification_templates (id) ON UPDATE CASCADE ON DELETE CASCADE,

    actor_id        UUID        REFERENCES users (id) ON DELETE SET NULL, -- User who triggered the notification
    broadcast_id    UUID REFERENCES broadcasts (id) ON DELETE CASCADE,
    entity_type     TEXT,
    entity_id       UUID,

    read            BOOLEAN     NOT NULL DEFAULT false,
    read_at         TIMESTAMPTZ,
    archived_at     TIMESTAMPTZ,

    custom_metadata JSONB,

    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_notifications_owner ON notifications (owner_id, created_at DESC);
CREATE INDEX idx_notifications_unread ON notifications (owner_id) WHERE read = false AND archived_at IS NULL;

CREATE INDEX idx_notifications_template ON notifications (template_id);
CREATE INDEX idx_notifications_actor ON notifications (actor_id);
CREATE INDEX idx_notifications_broadcast ON notifications (broadcast_id);

CREATE INDEX idx_notifications_owner_type ON notifications (owner_id, template_id, created_at DESC);

SELECT setup_updated_at_triggers();