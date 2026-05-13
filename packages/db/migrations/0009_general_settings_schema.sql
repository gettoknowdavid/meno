CREATE TABLE public.general_settings
(
    id                       UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id                  UUID    NOT NULL UNIQUE REFERENCES users (id) ON UPDATE CASCADE ON DELETE CASCADE,
    push_notifications       BOOLEAN NOT NULL DEFAULT FALSE,
    app_notifications        BOOLEAN NOT NULL DEFAULT FALSE,
    email_notifications      BOOLEAN NOT NULL DEFAULT FALSE,
    push_notification_token  TEXT,
    notification_preferences JSONB   NOT NULL DEFAULT '{}',
    display                  TEXT    NOT NULL DEFAULT 'system' CHECK ( display IN ('system', 'light', 'dark') ),
    language                 TEXT    NOT NULL DEFAULT 'en'
);