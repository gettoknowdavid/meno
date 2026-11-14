CREATE TABLE public.notification_types
(
    code        TEXT PRIMARY KEY,
    label       TEXT        NOT NULL,
    description TEXT,
    icon        TEXT,
    color       TEXT,
    created_at  TIMESTAMPTZ(3) NOT NULL DEFAULT now()
);

CREATE TABLE public.notification_templates
(
    id         UUID PRIMARY KEY     DEFAULT gen_random_uuid(),
    type       TEXT        NOT NULL REFERENCES public.notification_types (code) ON UPDATE CASCADE ON DELETE RESTRICT,
    title      TEXT        NOT NULL,
    body       TEXT        NOT NULL,
    image_url  TEXT,
    metadata   JSONB       NOT NULL DEFAULT '{}',
    is_active  BOOLEAN     NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ(3) NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ(3) NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ(3)
);

CREATE INDEX idx_notification_templates_type
    ON public.notification_templates (type)
    WHERE is_active AND deleted_at IS NULL;

CREATE TRIGGER notification_templates_set_updated_at
    BEFORE UPDATE ON public.notification_templates
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TABLE public.notifications
(
    id              UUID PRIMARY KEY     DEFAULT gen_random_uuid(),
    owner_id        UUID        NOT NULL REFERENCES public.users (id) ON UPDATE CASCADE ON DELETE CASCADE,
    template_id     UUID        NOT NULL REFERENCES public.notification_templates (id) ON UPDATE CASCADE ON DELETE CASCADE,
    actor_id        UUID                 REFERENCES public.users (id) ON UPDATE CASCADE ON DELETE SET NULL,
    broadcast_id    UUID                 REFERENCES public.broadcasts (id) ON UPDATE CASCADE ON DELETE CASCADE,
    entity_type     TEXT,
    entity_id       UUID,
    read            BOOLEAN     NOT NULL DEFAULT false,
    read_at         TIMESTAMPTZ(3),
    archived_at     TIMESTAMPTZ(3),
    custom_metadata JSONB,
    created_at      TIMESTAMPTZ(3) NOT NULL DEFAULT now()
);

CREATE INDEX idx_notifications_cursor
    ON public.notifications (owner_id, created_at DESC, id DESC)
    WHERE archived_at IS NULL;
CREATE INDEX idx_notifications_unread_cursor
    ON public.notifications (owner_id, created_at DESC, id DESC)
    WHERE read = false AND archived_at IS NULL;
CREATE INDEX idx_notifications_owner_template
    ON public.notifications (owner_id, template_id, created_at DESC)
    WHERE archived_at IS NULL;
CREATE INDEX idx_notifications_template ON public.notifications (template_id) WHERE archived_at IS NULL;
CREATE INDEX idx_notifications_actor ON public.notifications (actor_id) WHERE archived_at IS NULL;
CREATE INDEX idx_notifications_broadcast ON public.notifications (broadcast_id) WHERE archived_at IS NULL;

INSERT INTO public.notification_types (code, label, description)
VALUES ('added_as_cohost', 'Added as Co-host', 'You were invited to co-host a broadcast'),
       ('user_subscribed', 'New Follower', 'Someone subscribed to you'),
       ('scheduled_broadcast', 'Upcoming Broadcast', 'A creator you follow scheduled a broadcast'),
       ('live_broadcast_started', 'Live Now', 'A creator you follow went live'),
       ('broadcast_ended', 'Broadcast Ended', 'A broadcast you were in has ended')
ON CONFLICT (code) DO NOTHING;

INSERT INTO public.notification_templates (type, title, body)
VALUES ('added_as_cohost', 'Co-host Invite', '{actor} invited you to co-host {broadcast}'),
       ('user_subscribed', 'New Follower', '{actor} started following you'),
       ('scheduled_broadcast', 'Upcoming Broadcast', '{actor} scheduled a broadcast: {title}'),
       ('live_broadcast_started', 'Live Now', '{actor} is live: {title}'),
       ('broadcast_ended', 'Broadcast Ended', '{title} has ended');
