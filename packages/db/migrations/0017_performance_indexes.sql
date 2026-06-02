-- Broadcasts: status-filtered queries (the most common list query)
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_broadcasts_status_created
    ON broadcasts (status, created_at DESC)
    WHERE deleted_at IS NULL;

-- Users: full-text search (already used in profile search)
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_users_search_vector
    ON users USING GIN (search_vector);

-- Notifications: owner + read filter (most common notification list query)
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_notifications_owner_read
    ON notifications (owner_id, read, created_at DESC);

-- Refresh tokens: expiry cleanup (the batch-delete query needs this)
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_refresh_tokens_expires_at
    ON refresh_tokens (expires_at)
    WHERE expires_at IS NOT NULL;

-- Fast lookup for valid refresh tokens (user_id + expires_at)
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_refresh_tokens_user_expires
    ON refresh_tokens (user_id, expires_at);

-- Broadcast participants: participant lookup (join/leave path)
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_broadcast_participants_active
    ON broadcast_participants (broadcast_id)
    WHERE left_at IS NULL;

-- User subscribers: subscription feed lookups
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_user_subscribers_subscription
    ON user_subscribers (subscription_id, subscriber_id);

-- Composite index for common broadcast list queries
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_broadcasts_creator_status
    ON broadcasts (creator_id, status)
    WHERE deleted_at IS NULL;