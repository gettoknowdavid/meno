//! Keyset ("cursor") pagination primitives.
//!
//! Copied verbatim from `apps/api/src/shared/pagination.rs` on `master` (`903c3ba`)
//! apart from three deliberate edits, all recorded in guide Step 3.5:
//!
//! 1. The cross-crate `impl From<CursorError> for crate::shared::errors::MenoError` was
//!    deleted. `crates/core` cannot name a type from `apps/api` — the dependency runs
//!    the other way — so the impl could never have compiled here. The replacement
//!    converts into [`Error`] instead, which lives in this same crate.
//! 2. Doc comments were added to the public items that `master` left undocumented, so
//!    the workspace `missing_docs` lint has something to pass.
//! 3. The `#[cfg(test)]` module was appended. On `master` this logic had no tests at
//!    all and was untestable without standing up Postgres, Redis and an Axum router.
//!
//! **Why a cursor rather than `OFFSET`:** `OFFSET n` makes the database walk and discard
//! `n` rows, so page 1,000 costs 1,000 rows of work, and an insert or delete between
//! page requests shifts every subsequent page and silently drops or duplicates rows.
//! A cursor carries the exact sort key of the last row seen, so every page costs the
//! same and pagination stays stable under concurrent writes.

use crate::error::{Error, ErrorCode};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};
use time::OffsetDateTime;
use uuid::Uuid;

/// The universal cursor wire type — an opaque, URL-safe base64 string.
///
/// Always treated as opaque by clients; never constructed manually.
///
/// Five wire shapes exist, one per sort strategy, each distinguished by a prefix so a
/// decoded cursor can never be fed to the wrong decoder.
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

    /// Decode into (`OffsetDateTime`, Uuid).
    ///
    /// # Errors
    ///
    /// Returns [`CursorError::InvalidEncoding`] if the value is not URL-safe base64, and
    /// [`CursorError::InvalidShape`] if it is not a `nanos|uuid` pair.
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

    /// Decode into (`OffsetDateTime`, `OffsetDateTime`, `Uuid`).
    ///
    /// # Errors
    ///
    /// Returns [`CursorError::InvalidShape`] if the value is not a `n1:n2|uuid` triple.
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

    /// Encode (i64 score, uuid) — used for count-sorted queries such as
    /// broadcasts sorted by `total_listeners`.
    #[must_use]
    pub fn from_score_id(score: i64, id: Uuid) -> Self {
        let raw = format!("score:{score}|{id}");
        Self(URL_SAFE_NO_PAD.encode(raw))
    }

    /// Decode into (i64 score, Uuid).
    ///
    /// # Errors
    ///
    /// Returns [`CursorError::InvalidShape`] if the value lacks the `score:` prefix or
    /// is not a `score|uuid` pair.
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

    /// Encode (name, uuid) for name-based sorting.
    #[must_use]
    pub fn from_name_id(name: &str, id: Uuid) -> Self {
        // Names can have special chars, so we encode safely
        let raw = format!("name:{name}|{id}");
        Self(URL_SAFE_NO_PAD.encode(raw))
    }

    /// Decode into (String, Uuid) for name-based cursor.
    ///
    /// # Errors
    ///
    /// Returns [`CursorError::InvalidShape`] if the value lacks the `name:` prefix or
    /// is not a `name|uuid` pair. Names containing `|` are supported — see the
    /// implementation note.
    pub fn to_name_id(&self) -> Result<(String, Uuid), CursorError> {
        let bytes = URL_SAFE_NO_PAD
            .decode(&self.0)
            .map_err(|_| CursorError::InvalidEncoding)?;
        let s = String::from_utf8(bytes).map_err(|_| CursorError::InvalidEncoding)?;
        let s = s.strip_prefix("name:").ok_or(CursorError::InvalidShape)?;
        // rsplitn, not splitn: the uuid is always last, so split from the right.
        // On master this used splitn, which breaks for any name containing `|` — a
        // folder titled `a|b` encoded to `name:a|b|<uuid>` and decoding fed `b|<uuid>`
        // to Uuid::parse_str, so the next page request returned 400. Every other shape
        // puts only numeric fields ahead of the uuid, so this is the only decoder
        // affected. rsplitn yields [uuid, name], hence the swapped indices.
        let parts: Vec<&str> = s.rsplitn(2, '|').collect();
        if parts.len() != 2 {
            return Err(CursorError::InvalidShape);
        }
        let id = Uuid::parse_str(parts[0]).map_err(|_| CursorError::InvalidShape)?;
        let name = parts[1].to_string();
        Ok((name, id))
    }

    /// Encode (`rank_score`, `timestamp`, `uuid`) for search result pagination.
    ///
    /// Rank is a float (`ts_rank` result), but we store it as a string to preserve
    /// precision.
    #[must_use]
    pub fn from_rank_timestamp_id(rank: f32, ts: OffsetDateTime, id: Uuid) -> Self {
        // Store rank with 6 decimal places for consistency
        let raw = format!("rank:{rank}|{}|{id}", ts.unix_timestamp_nanos());
        Self(URL_SAFE_NO_PAD.encode(raw))
    }

    /// Decode into (f32, OffsetDateTime, Uuid) for search cursor.
    ///
    /// # Errors
    ///
    /// Returns [`CursorError::InvalidShape`] if the value lacks the `rank:` prefix or is
    /// not a `rank|nanos|uuid` triple.
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

/// Why a cursor could not be decoded.
///
/// This stays a crate-internal detail enum rather than collapsing into [`Error`]
/// directly: the three causes are genuinely distinct for diagnosis, and every one of them
/// maps to the same client-visible outcome — a 400 with code `INVALID_CURSOR`. Keeping
/// the precision internally costs nothing and makes a bad cursor debuggable from logs.
#[derive(Debug, thiserror::Error)]
pub enum CursorError {
    /// The value is not valid URL-safe base64.
    #[error("Invalid cursor encoding")]
    InvalidEncoding,
    /// The value decoded, but does not match the expected `prefix|a|b|c` shape.
    #[error("Invalid cursor shape")]
    InvalidShape,
    /// A timestamp field parsed as an integer but is not a representable instant.
    #[error("Invalid timestamp in cursor")]
    InvalidTimestamp,
}

impl From<CursorError> for Error {
    /// A client fully controls this value via `?cursor=`, so every decode failure is a
    /// bad request — never a server fault, and never a panic.
    fn from(err: CursorError) -> Self {
        Self::BadRequest {
            code: ErrorCode::InvalidCursor,
            message: format!("Invalid pagination cursor: {err}"),
        }
    }
}

/// Pagination parameters that are embedded in every query struct.
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

    /// `limit + 1` — used for the "fetch one extra to detect next page" trick.
    #[must_use]
    pub fn limit_plus_one(&self) -> i64 {
        self.limit() + 1
    }
}

/// Standard paginated response returned by every list endpoint.
#[derive(Debug, Serialize, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorPage<T: Serialize> {
    /// The page of items. Never longer than the requested limit.
    pub data: Vec<T>,
    /// Opaque cursor to pass as `?cursor=` for the next page. `None` on the last page.
    pub next_cursor: Option<Cursor>,
    /// Whether another page exists. Cheaper than testing `next_cursor.is_some()`, and
    /// survives a future change where a valid cursor may exist without a next page.
    pub has_next_page: bool,
}

impl<T: Serialize> CursorPage<T> {
    /// Build a page from a `Vec` that was fetched with `limit + 1`.
    ///
    /// `encode_cursor` is called on the last *included* item to produce
    /// the cursor for the next page. It is not called when there is no
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

/// Sort direction used by repository helpers. Embedded in domain query
/// structs wherever the caller may choose a direction.
#[derive(Debug, Deserialize, Default, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Order {
    /// Newest/largest first. The default: almost every feed wants this.
    #[default]
    Desc,
    /// Oldest/smallest first.
    Asc,
}

impl Order {
    /// SQL direction fragment, including NULLS handling.
    ///
    /// A fixed fragment, never caller text — see the injection guard in the tests.
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
    /// so we use `<`. For ASC the next page has rows *after*, so we use `>`.
    #[must_use]
    pub fn cursor_op(self) -> &'static str {
        match self {
            Order::Desc => "<",
            Order::Asc => ">",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(nanos: i128) -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp_nanos(nanos).expect("valid timestamp")
    }

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn timestamp_id_round_trips() {
        let (t, i) = (ts(1_700_000_000_123_456_789), id(7));
        let (t2, i2) = Cursor::from_timestamp_id(t, i)
            .to_timestamp_id()
            .expect("decodes");
        assert_eq!(t.unix_timestamp_nanos(), t2.unix_timestamp_nanos());
        assert_eq!(i, i2);
    }

    #[test]
    fn two_timestamp_id_round_trips() {
        let (a, b, i) = (ts(100), ts(200), id(9));
        let (a2, b2, i2) = Cursor::from_two_timestamps_id(a, b, i)
            .to_two_timestamps_id()
            .expect("decodes");
        assert_eq!(a.unix_timestamp_nanos(), a2.unix_timestamp_nanos());
        assert_eq!(b.unix_timestamp_nanos(), b2.unix_timestamp_nanos());
        assert_eq!(i, i2);
    }

    #[test]
    fn score_id_round_trips_across_the_i64_range() {
        for score in [i64::MIN + 1, -1, 0, 1, i64::MAX] {
            let (s, i) = Cursor::from_score_id(score, id(3))
                .to_score_id()
                .expect("decodes");
            assert_eq!(s, score, "score {score} must survive the round trip");
            assert_eq!(i, id(3));
        }
    }

    #[test]
    fn name_id_round_trips_including_awkward_characters() {
        for name in [
            "simple",
            "with space",
            "emoji 🎙️",
            "quote'and\"dq",
            "pipe|inside",
        ] {
            let (n, i) = Cursor::from_name_id(name, id(4))
                .to_name_id()
                .expect("decodes");
            assert_eq!(n, name, "name {name:?} must survive the round trip");
            assert_eq!(i, id(4));
        }
    }

    #[test]
    fn rank_timestamp_id_round_trips() {
        let (rank, t, i) = (0.75_f32, ts(1_700_000_000_000_000_000), id(5));
        let (r2, t2, i2) = Cursor::from_rank_timestamp_id(rank, t, i)
            .to_rank_timestamp_id()
            .expect("decodes");
        assert!((r2 - rank).abs() < f32::EPSILON, "rank must round-trip");
        assert_eq!(t.unix_timestamp_nanos(), t2.unix_timestamp_nanos());
        assert_eq!(i, i2);
    }

    #[test]
    fn encoding_is_url_safe_and_unpadded() {
        // These go straight into a ?cursor= query parameter, so '+', '/' and '=' would
        // each need escaping, and any client that forgets is silently broken.
        let c = Cursor::from_timestamp_id(ts(1), id(1));
        assert!(
            !c.0.contains('='),
            "padding needs escaping in a query string"
        );
        assert!(!c.0.contains('+'), "base64 '+' must not appear in a URL");
        assert!(!c.0.contains('/'), "base64 '/' must not appear in a URL");
    }

    #[test]
    fn client_supplied_garbage_is_rejected_and_never_panics() {
        // A client fully controls this via ?cursor=. Every one of these is reachable,
        // so every one must be an Err — never a panic, because the release profile
        // uses panic = "abort".
        for bad in [
            "",
            "!!!",
            "YWJj",
            "MTIzfGVrZXM",
            "999999999999999999999|fake",
        ] {
            let c = Cursor(bad.to_string());
            assert!(
                c.to_timestamp_id().is_err(),
                "cursor {bad:?} must not decode"
            );
        }
        // A score cursor fed to the timestamp decoder must fail rather than
        // silently mis-parse, and vice versa.
        assert!(Cursor::from_score_id(5, id(1)).to_timestamp_id().is_err());
    }

    #[test]
    fn a_cursor_error_converts_to_the_stable_wire_code() {
        // The consolidation's whole point: a crate-internal failure reaches the client
        // as one predictable code, whatever its internal cause.
        let err: Error = CursorError::InvalidShape.into();
        let body = crate::error::to_body(&err);
        assert_eq!(body.status_code, 400);
        assert_eq!(body.code, "INVALID_CURSOR");
        assert!(body.data.is_none());
    }

    #[test]
    fn from_rows_truncates_the_extra_row_and_reports_more() {
        let rows: Vec<u32> = (0..=10).collect(); // 11 rows
        let page = CursorPage::from_rows(rows, 10, |v| Cursor::from_score_id(i64::from(*v), id(1)));
        assert!(
            page.has_next_page,
            "11 rows at limit 10 means a next page exists"
        );
        assert_eq!(page.data.len(), 10, "the limit+1 probe row must be dropped");
        assert!(page.next_cursor.is_some());
    }

    #[test]
    fn from_rows_emits_no_cursor_on_the_last_page() {
        let rows: Vec<u32> = (0..10).collect(); // exactly limit
        let page = CursorPage::from_rows(rows, 10, |v| Cursor::from_score_id(i64::from(*v), id(1)));
        assert!(!page.has_next_page);
        assert!(
            page.next_cursor.is_none(),
            "a final page must not advertise a cursor"
        );
    }

    #[test]
    fn order_emits_only_fixed_sql_fragments() {
        // These strings are interpolated straight into SQL by push_order_and_limit.
        // This test is a SQL-injection guard: it proves a deserialized Order can never
        // carry client text into a query.
        assert_eq!(Order::Desc.sql(), "DESC NULLS LAST");
        assert_eq!(Order::Asc.sql(), "ASC NULLS FIRST");
        assert_eq!(Order::Desc.cursor_op(), "<");
        assert_eq!(Order::Asc.cursor_op(), ">");
    }

    #[test]
    fn limit_is_clamped_to_a_sane_range() {
        let mut p = CursorParams::default();
        assert_eq!(p.limit(), 20, "default page size");
        p.limit = Some(0);
        assert_eq!(p.limit(), 1, "zero must clamp up to 1");
        p.limit = Some(10_000);
        assert_eq!(p.limit(), 100, "absurd limits must clamp to 100");
        p.limit = Some(-5);
        assert_eq!(p.limit(), 1, "negative must clamp to 1");
        assert_eq!(p.limit_plus_one(), 2);
    }
}
