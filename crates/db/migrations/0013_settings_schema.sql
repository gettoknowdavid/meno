CREATE TABLE public.settings
(
    id                       UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id                  UUID    NOT NULL UNIQUE REFERENCES public.users (id) ON UPDATE CASCADE ON DELETE CASCADE,
    push_notifications       BOOLEAN NOT NULL DEFAULT false,
    app_notifications        BOOLEAN NOT NULL DEFAULT false,
    email_notifications      BOOLEAN NOT NULL DEFAULT false,
    push_notification_token  TEXT,
    notification_preferences JSONB   NOT NULL DEFAULT '{}',
    display                  TEXT    NOT NULL DEFAULT 'system',
    language                 TEXT    NOT NULL DEFAULT 'en',

    CONSTRAINT settings_display_check CHECK (display IN ('system', 'light', 'dark'))
);
