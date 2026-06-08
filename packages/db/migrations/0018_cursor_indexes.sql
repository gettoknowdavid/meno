-- ─────────────────────────────────────────────────────────────────
-- BROADCASTS
-- Primary feed: newest first, filtered by status / deleted_at
-- ─────────────────────────────────────────────────────────────────
CREATE INDEX IF NOT EXISTS idx_broadcasts_cursor_main
    ON broadcasts (created_at DESC, id DESC)
    WHERE deleted_at IS NULL;

-- Creator feed (my broadcasts / drafts)
CREATE INDEX IF NOT EXISTS idx_broadcasts_cursor_creator
    ON broadcasts (creator_id, created_at DESC, id DESC)
    WHERE deleted_at IS NULL;

-- Active broadcasts sorted by listener count (now-live section)
-- NOTE: total_listeners is a computed column we add below
CREATE INDEX IF NOT EXISTS idx_broadcasts_cursor_active
    ON broadcasts (status, created_at DESC, id DESC)
    WHERE deleted_at IS NULL;

-- Recently-ended feed
CREATE INDEX IF NOT EXISTS idx_broadcasts_cursor_ended
    ON broadcasts (end_time DESC NULLS LAST, id DESC)
    WHERE status = 'inactive' AND deleted_at IS NULL;

-- Scheduled broadcasts
CREATE INDEX IF NOT EXISTS idx_broadcasts_cursor_scheduled
    ON broadcasts (start_time ASC NULLS LAST, id ASC)
    WHERE status = 'inactive' AND deleted_at IS NULL;

-- Full-text search (GIN — already in 0002, kept here for completeness)
CREATE INDEX IF NOT EXISTS idx_broadcasts_fts
    ON broadcasts USING GIN (to_tsvector('english', title || ' ' || description))
    WHERE deleted_at IS NULL;

-- ─────────────────────────────────────────────────────────────────
-- BROADCAST PARTICIPANTS (participants inside a live room)
-- ─────────────────────────────────────────────────────────────────
CREATE INDEX IF NOT EXISTS idx_broadcast_participants_cursor
    ON broadcast_participants (broadcast_id, joined_at DESC, participant_id DESC);

-- ─────────────────────────────────────────────────────────────────
-- USERS
-- ─────────────────────────────────────────────────────────────────
CREATE INDEX IF NOT EXISTS idx_users_cursor_created
    ON users (created_at DESC, id DESC)
    WHERE deleted_at IS NULL;

-- Full-text search on full_name
CREATE INDEX IF NOT EXISTS idx_users_fts
    ON users USING GIN (to_tsvector('english', full_name))
    WHERE deleted_at IS NULL;

-- ─────────────────────────────────────────────────────────────────
-- USER SUBSCRIBERS (follow relationships)
-- ─────────────────────────────────────────────────────────────────

-- "Who follows creator X" — creator's followers list
CREATE INDEX IF NOT EXISTS idx_user_subscribers_followers_cursor
    ON user_subscribers (subscription_id, created_at DESC, subscriber_id DESC);

-- "Who does user X follow" — user's following list
CREATE INDEX IF NOT EXISTS idx_user_subscribers_following_cursor
    ON user_subscribers (subscriber_id, created_at DESC, subscription_id DESC);

-- ─────────────────────────────────────────────────────────────────
-- NOTIFICATIONS
-- ─────────────────────────────────────────────────────────────────
CREATE INDEX IF NOT EXISTS idx_notifications_cursor
    ON notifications (owner_id, created_at DESC, id DESC)
    WHERE archived_at IS NULL;

-- Unread-only filter (common query)
CREATE INDEX IF NOT EXISTS idx_notifications_unread_cursor
    ON notifications (owner_id, created_at DESC, id DESC)
    WHERE archived_at IS NULL AND read = false;

-- ─────────────────────────────────────────────────────────────────
-- CHAT MESSAGES
-- ─────────────────────────────────────────────────────────────────
-- Ascending (oldest-first) — initial load of chat history
CREATE INDEX IF NOT EXISTS idx_chat_messages_cursor_asc
    ON chat_messages (broadcast_id, created_at ASC, id ASC);

-- Descending — "load earlier" (infinite scroll upward)
CREATE INDEX IF NOT EXISTS idx_chat_messages_cursor_desc
    ON chat_messages (broadcast_id, created_at DESC, id DESC);

-- ─────────────────────────────────────────────────────────────────
-- CHAT REACTIONS (ephemeral, but still paginated in history view)
-- ─────────────────────────────────────────────────────────────────
CREATE INDEX IF NOT EXISTS idx_chat_reactions_cursor
    ON chat_reactions (broadcast_id, created_at DESC, id DESC);

-- ─────────────────────────────────────────────────────────────────
-- NOTES
-- ─────────────────────────────────────────────────────────────────
-- Pinned notes first, then by updated_at
CREATE INDEX IF NOT EXISTS idx_notes_cursor_pinned
    ON notes (creator_id, pinned DESC, updated_at DESC, id DESC);

-- Notes within a folder
CREATE INDEX IF NOT EXISTS idx_notes_cursor_folder
    ON notes (folder_id, updated_at DESC, id DESC)
    WHERE folder_id IS NOT NULL;

-- Notes full-text search
CREATE INDEX IF NOT EXISTS idx_notes_fts
    ON notes USING GIN (to_tsvector('english', title || ' ' || content));

-- ─────────────────────────────────────────────────────────────────
-- FOLDERS
-- ─────────────────────────────────────────────────────────────────
CREATE INDEX IF NOT EXISTS idx_folders_cursor
    ON folders (creator_id, pinned DESC, created_at DESC, id DESC);

-- ─────────────────────────────────────────────────────────────────
-- BROADCAST BOOKMARKS (listen later)
-- ─────────────────────────────────────────────────────────────────
CREATE INDEX IF NOT EXISTS idx_broadcast_bookmarks_cursor
    ON broadcast_bookmarks (user_id, saved_at DESC, broadcast_id DESC);

-- ─────────────────────────────────────────────────────────────────
-- OTP (no pagination needed — point lookups only)
-- REFRESH TOKENS (no pagination needed)
-- ─────────────────────────────────────────────────────────────────
-- (no new indexes needed for these tables)