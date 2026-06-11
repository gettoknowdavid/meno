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
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Notification Templates
CREATE TABLE public.notification_templates
(
    id         UUID PRIMARY KEY     DEFAULT gen_random_uuid(),
    type       TEXT        NOT NULL REFERENCES notification_types (code) ON DELETE RESTRICT ON UPDATE CASCADE,

    title      TEXT        NOT NULL,
    body       TEXT        NOT NULL,
    image_url  TEXT,

    metadata   JSONB       NOT NULL DEFAULT '{}',
    is_active  BOOLEAN     NOT NULL DEFAULT TRUE,

    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_notification_templates_type
    ON notification_templates (type)
    WHERE is_active = true AND deleted_at IS NULL;

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

-- Index for the primary list query (owner + unread filter + cursor)
CREATE INDEX IF NOT EXISTS idx_notifications_owner_created
    ON notifications (owner_id, created_at DESC, id DESC)
    WHERE archived_at IS NULL;

-- Index for unread-count query
CREATE INDEX IF NOT EXISTS idx_notifications_owner_unread
    ON notifications (owner_id)
    WHERE read = false AND archived_at IS NULL;

CREATE INDEX idx_notifications_template ON notifications (template_id) WHERE archived_at IS NULL;
CREATE INDEX idx_notifications_actor ON notifications (actor_id) WHERE archived_at IS NULL;
CREATE INDEX idx_notifications_broadcast ON notifications (broadcast_id) WHERE archived_at IS NULL;

CREATE INDEX idx_notifications_owner_type ON notifications (owner_id, template_id, created_at DESC) WHERE archived_at IS NULL;

SELECT setup_updated_at_triggers();

INSERT INTO notification_types (code, label, description)
VALUES ('added_as_cohost', 'Added as Co-host', 'You were invited to co-host a broadcast'),
       ('user_subscribed', 'New Follower', 'Someone subscribed to you'),
       ('scheduled_broadcast', 'Upcoming Broadcast', 'A creator you follow scheduled a broadcast'),
       ('live_broadcast_started', 'Live Now', 'A creator you follow went live'),
       ('broadcast_ended', 'Broadcast Ended', 'A broadcast you were in has ended')
ON CONFLICT (code) DO NOTHING;

INSERT INTO notification_templates (type, title, body)
VALUES ('added_as_cohost', 'Co-host Invite', '{actor} invited you to co-host {broadcast}'),
       ('user_subscribed', 'New Follower', '{actor} started following you'),
       ('scheduled_broadcast', 'Upcoming Broadcast', '{actor} scheduled a broadcast: {title}'),
       ('live_broadcast_started', 'Live Now', '{actor} is live: {title}'),
       ('broadcast_ended', 'Broadcast Ended', '{title} has ended')
ON CONFLICT DO NOTHING;