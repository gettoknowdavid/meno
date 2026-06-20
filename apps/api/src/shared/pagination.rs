use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};
use time::OffsetDateTime;
use uuid::Uuid;

// ─────────────────────────────────────────────────────────────────────────────
// CURSOR
// ─────────────────────────────────────────────────────────────────────────────

/// The universal cursor wire type — an opaque, URL-safe base64 string.
/// Always treated as opaque by clients; never constructed manually.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Cursor(pub String);

impl Cursor {
    /// Encode a (timestamp, uuid) pair — used by the vast majority of feeds.
    #[must_use]
    pub fn from_timestamp_id(ts: OffsetDateTime, id: Uuid) -> Self {
        let raw = format!("{}|{}", ts.unix_timestamp_nanos(), id);
        Self(URL_SAFE_NO_PAD.encode(raw))
    }

    /// Encode (`primary_ts`, `secondary_ts`, `uuid`) — used for composite sorts
    /// such as notes (`pinned` + `updated_at`) and folders (`pinned` + `created_at`).
    #[must_use]
    pub fn from_two_timestamps_id(ts1: OffsetDateTime, ts2: OffsetDateTime, id: Uuid) -> Self {
        let raw = format!(
            "{}:{}|{}",
            ts1.unix_timestamp_nanos(),
            ts2.unix_timestamp_nanos(),
            id
        );
        Self(URL_SAFE_NO_PAD.encode(raw))
    }

    /// Encode (i64 score, uuid) — used for count-sorted queries such as
    /// broadcasts sorted by `total_listeners`.
    #[must_use]
    pub fn from_score_id(score: i64, id: Uuid) -> Self {
        let raw = format!("score:{score}|{id}");
        Self(URL_SAFE_NO_PAD.encode(raw))
    }

    /// Decode into (`OffsetDateTime`, Uuid).
    pub fn to_timestamp_id(&self) -> Result<(OffsetDateTime, Uuid), CursorError> {
        let bytes = URL_SAFE_NO_PAD
            .decode(&self.0)
            .map_err(|_| CursorError::InvalidEncoding)?;
        let s = String::from_utf8(bytes).map_err(|_| CursorError::InvalidEncoding)?;
        let parts: Vec<&str> = s.splitn(2, '|').collect();
        if parts.len() != 2 {
            return Err(CursorError::InvalidShape);
        }
        let nanos: i128 = parts[0].parse().map_err(|_| CursorError::InvalidShape)?;
        let id = Uuid::parse_str(parts[1]).map_err(|_| CursorError::InvalidShape)?;
        let ts = OffsetDateTime::from_unix_timestamp_nanos(nanos)
            .map_err(|_| CursorError::InvalidTimestamp)?;
        Ok((ts, id))
    }

    /// Decode into (`OffsetDateTime`, `OffsetDateTime`, `Uuid`).
    pub fn to_two_timestamps_id(
        &self,
    ) -> Result<(OffsetDateTime, OffsetDateTime, Uuid), CursorError> {
        let bytes = URL_SAFE_NO_PAD
            .decode(&self.0)
            .map_err(|_| CursorError::InvalidEncoding)?;
        let s = String::from_utf8(bytes).map_err(|_| CursorError::InvalidEncoding)?;
        // Format: "{n1}:{n2}|{uuid}"
        let parts: Vec<&str> = s.splitn(2, '|').collect();
        if parts.len() != 2 {
            return Err(CursorError::InvalidShape);
        }
        let ts_parts: Vec<&str> = parts[0].splitn(2, ':').collect();
        if ts_parts.len() != 2 {
            return Err(CursorError::InvalidShape);
        }
        let n1: i128 = ts_parts[0].parse().map_err(|_| CursorError::InvalidShape)?;
        let n2: i128 = ts_parts[1].parse().map_err(|_| CursorError::InvalidShape)?;
        let id = Uuid::parse_str(parts[1]).map_err(|_| CursorError::InvalidShape)?;
        let ts1 = OffsetDateTime::from_unix_timestamp_nanos(n1)
            .map_err(|_| CursorError::InvalidTimestamp)?;
        let ts2 = OffsetDateTime::from_unix_timestamp_nanos(n2)
            .map_err(|_| CursorError::InvalidTimestamp)?;
        Ok((ts1, ts2, id))
    }

    /// Decode into (i64 score, Uuid).
    pub fn to_score_id(&self) -> Result<(i64, Uuid), CursorError> {
        let bytes = URL_SAFE_NO_PAD
            .decode(&self.0)
            .map_err(|_| CursorError::InvalidEncoding)?;
        let s = String::from_utf8(bytes).map_err(|_| CursorError::InvalidEncoding)?;
        let s = s.strip_prefix("score:").ok_or(CursorError::InvalidShape)?;
        let parts: Vec<&str> = s.splitn(2, '|').collect();
        if parts.len() != 2 {
            return Err(CursorError::InvalidShape);
        }
        let score: i64 = parts[0].parse().map_err(|_| CursorError::InvalidShape)?;
        let id = Uuid::parse_str(parts[1]).map_err(|_| CursorError::InvalidShape)?;
        Ok((score, id))
    }

    /// Encode (name, uuid) for name-based sorting
    #[must_use]
    pub fn from_name_id(name: &str, id: Uuid) -> Self {
        // Names can have special chars, so we encode safely
        let raw = format!("name:{name}|{id}");
        Self(URL_SAFE_NO_PAD.encode(raw))
    }

    /// Decode into (String, Uuid) for name-based cursor
    pub fn to_name_id(&self) -> Result<(String, Uuid), CursorError> {
        let bytes = URL_SAFE_NO_PAD
            .decode(&self.0)
            .map_err(|_| CursorError::InvalidEncoding)?;
        let s = String::from_utf8(bytes).map_err(|_| CursorError::InvalidEncoding)?;
        let s = s.strip_prefix("name:").ok_or(CursorError::InvalidShape)?;
        let parts: Vec<&str> = s.splitn(2, '|').collect();
        if parts.len() != 2 {
            return Err(CursorError::InvalidShape);
        }
        let name = parts[0].to_string();
        let id = Uuid::parse_str(parts[1]).map_err(|_| CursorError::InvalidShape)?;
        Ok((name, id))
    }

    /// Encode (`rank_score`, `timestamp`, `uuid`) for search result pagination
    /// Rank is a float (ts_rank result), but we store as string to preserve precision
    #[must_use]
    pub fn from_rank_timestamp_id(rank: f32, ts: OffsetDateTime, id: Uuid) -> Self {
        // Store rank with 6 decimal places for consistency
        let raw = format!("rank:{rank}|{}|{id}", ts.unix_timestamp_nanos());
        Self(URL_SAFE_NO_PAD.encode(raw))
    }

    /// Decode into (f32, OffsetDateTime, Uuid) for search cursor
    pub fn to_rank_timestamp_id(&self) -> Result<(f32, OffsetDateTime, Uuid), CursorError> {
        let bytes = URL_SAFE_NO_PAD
            .decode(&self.0)
            .map_err(|_| CursorError::InvalidEncoding)?;
        let s = String::from_utf8(bytes).map_err(|_| CursorError::InvalidEncoding)?;
        let s = s.strip_prefix("rank:").ok_or(CursorError::InvalidShape)?;
        let parts: Vec<&str> = s.splitn(3, '|').collect();
        if parts.len() != 3 {
            return Err(CursorError::InvalidShape);
        }

        let rank: f32 = parts[0].parse().map_err(|_| CursorError::InvalidShape)?;
        let nanos: i128 = parts[1].parse().map_err(|_| CursorError::InvalidShape)?;
        let id = Uuid::parse_str(parts[2]).map_err(|_| CursorError::InvalidShape)?;
        let ts = OffsetDateTime::from_unix_timestamp_nanos(nanos)
            .map_err(|_| CursorError::InvalidTimestamp)?;

        Ok((rank, ts, id))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CursorError {
    #[error("Invalid cursor encoding")]
    InvalidEncoding,
    #[error("Invalid cursor shape")]
    InvalidShape,
    #[error("Invalid timestamp in cursor")]
    InvalidTimestamp,
}

impl From<CursorError> for crate::shared::errors::MenoError {
    fn from(e: CursorError) -> Self {
        Self::BadRequest(format!("Invalid pagination cursor: {e}"))
    }
}

/// Pagination parameters that are embedded in every query struct via
/// `#[serde(flatten)]`.  Never extracted separately; always part of the
/// domain query struct.
#[serde_as]
#[derive(Debug, Deserialize, Serialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CursorParams {
    /// Opaque cursor from the previous response's `nextCursor` field.
    /// Absent on the first page.
    pub cursor: Option<Cursor>,

    /// Items per page. Clamped server-side to [1, 100]. Default: 20.
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub limit: Option<i64>,
}

impl CursorParams {
    /// Validated limit, clamped to [1, 100].
    #[must_use]
    pub fn limit(&self) -> i64 {
        self.limit.unwrap_or(20).clamp(1, 100)
    }

    /// limit + 1 — used for the "fetch one extra to detect next page" trick.
    #[must_use]
    pub fn limit_plus_one(&self) -> i64 {
        self.limit() + 1
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// RESPONSE
// ─────────────────────────────────────────────────────────────────────────────

/// Standard paginated response returned by every list endpoint.
///
/// `total_count` is populated only when the count is available at zero
/// extra cost (e.g., from a Redis counter). For large filtered queries we
/// skip it — a COUNT(*) on millions of rows under arbitrary filters is
/// too expensive to run on every page.
#[derive(Debug, Serialize, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorPage<T: Serialize> {
    pub data: Vec<T>,
    pub next_cursor: Option<Cursor>,
    pub has_next_page: bool,
}

impl<T: Serialize> CursorPage<T> {
    /// Build a page from a `Vec` that was fetched with `limit + 1`.
    ///
    /// `encode_cursor` is called on the last *included* item to produce
    /// the cursor for the next page.  It is not called when there is no
    /// next page, so callers need not handle the `None` case.
    pub fn from_rows<F>(mut rows: Vec<T>, limit: i64, encode_cursor: F) -> Self
    where
        F: Fn(&T) -> Cursor,
    {
        let has_next = rows.len() > limit as usize;
        if has_next {
            rows.truncate(limit as usize);
        }
        let next_cursor = if has_next {
            rows.last().map(encode_cursor)
        } else {
            None
        };
        Self {
            data: rows,
            next_cursor,
            has_next_page: has_next,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SORT ORDER
// ─────────────────────────────────────────────────────────────────────────────

/// Sort direction used by repository helpers.  Embedded in domain query
/// structs wherever the caller may choose a direction.
#[derive(Debug, Deserialize, Default, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Order {
    #[default]
    Desc, // newest-first (default for all feeds)
    Asc, // oldest-first (chronological chat history)
}

impl Order {
    /// SQL direction fragment, including NULLS handling.
    #[must_use]
    pub fn sql(self) -> &'static str {
        match self {
            Order::Desc => "DESC NULLS LAST",
            Order::Asc => "ASC NULLS FIRST",
        }
    }

    /// Cursor comparison operator.
    ///
    /// For DESC (newest-first) the next page has rows *before* the cursor row,
    /// so we use `<`.  For ASC the next page has rows *after*, so we use `>`.
    #[must_use]
    pub fn cursor_op(self) -> &'static str {
        match self {
            Order::Desc => "<",
            Order::Asc => ">",
        }
    }
}
