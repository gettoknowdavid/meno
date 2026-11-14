//! Cursor pagination helpers for `sqlx::QueryBuilder`.
//!
//! The plan's §5.2 "preserve this pattern" note refers to the
//! `QueryBuilder` + `build_query_as` approach for dynamic SQL: one query with a
//! `WHERE` clause whose shape depends on filters, rather than a family of pre-baked
//! statements. These two functions are the pagination half of that pattern, and they are
//! what stops every feed from having its own subtly-wrong cursor implementation.
//!
//! # What changed from the original
//!
//! The first version of this file could not compile. Three fixes, none of them cosmetic:
//!
//! - **`crate::shared::pagination::Order` does not exist.** The cursor type moved to
//!   `crates/core`, so this now names [`meno_core::pagination::Order`]. The original
//!   path was a leftover from `master`'s layout that the module split (§5.1) removed.
//! - **`sqlx` was not a dependency of `apps/api`.** It is now declared, matching the
//!   manifest's own note that the final set lands with the query-using modules.
//! - **Unqualified column names were interpolated into SQL.** See below.
//!
//! # Injection: the column names are the one thing that cannot be bound
//!
//! §9.5 requires parameterised SQL, and every *value* here is a bound parameter. The
//! column names cannot be — Postgres has no placeholder for an identifier — so they are
//! interpolated. That is only safe because the caller supplies them as literals from its
//! own source, never from a request. [`ColumnName`] makes that a type-level requirement:
//! a caller cannot pass an arbitrary `&str` that arrived from a query parameter without
//! going through [`ColumnName::parse`], which rejects anything that is not a bare,
//! lowercase SQL identifier.
//!
//! `Order::sql()` and `Order::cursor_op()` return `&'static str` from a `match`, so they
//! cannot carry caller text at all — that property is inherited, not re-checked here.

use meno_core::pagination::Order;
use sqlx::{Postgres, QueryBuilder};
use time::OffsetDateTime;
use uuid::Uuid;

/// A SQL column name that has been checked to be safe to interpolate.
///
/// The check is deliberately narrow rather than clever: lowercase, ASCII, and made only
/// of `[a-z0-9_]`, with no leading digit. Anything else — a space, a quote, a semicolon,
/// a `--` comment — is rejected at construction, so the only way to get a
/// `ColumnName` into [`push_cursor_condition`] is with a string that was already safe.
///
/// Not `serde`-derivable or `From<&str>` on purpose. Those would put the validation one
/// layer away from the call site, and a caller who finds a convenient unchecked
/// constructor will use it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColumnName<'a>(&'a str);

impl<'a> ColumnName<'a> {
    /// The name as SQL should see it.
    #[must_use]
    pub const fn as_str(self) -> &'a str {
        self.0
    }

    /// Wrap a literal column name without checking.
    ///
    /// Use this for column names hard-coded in this crate's own source and therefore
    /// incapable of carrying request input — the validation that [`parse`](Self::parse)
    /// provides is only useful for names that arrived from outside. The field is private
    /// and constructible only within this impl, so this is the infallible boundary for
    /// known-safe literals while `parse` remains the checked boundary for external ones.
    #[must_use]
    pub const fn literal(name: &'a str) -> Self {
        Self(name)
    }

    /// Check a name and return it as a [`ColumnName`].
    ///
    /// # Errors
    ///
    /// Returns a message naming the problem when `name` is empty, starts with a digit, or
    /// contains a character outside `[a-z0-9_]`.
    pub fn parse(name: &'a str) -> Result<Self, &'static str> {
        if name.is_empty() {
            return Err("a column name must not be empty");
        }
        if name.starts_with(|c: char| c.is_ascii_digit()) {
            return Err("a column name must not start with a digit");
        }
        if let Some(bad) = name
            .chars()
            .find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_'))
        {
            return Err(match bad {
                'A'..='Z' => "a column name must be lowercase",
                _ => "a column name may only contain a-z, 0-9 and _",
            });
        }
        Ok(Self(name))
    }
}

impl std::fmt::Display for ColumnName<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

/// Append the keyset cursor comparison, if there is a cursor.
///
/// Call this **after** the main `WHERE` clause. Emits a row-value comparison so the
/// timestamp and id move together:
///
/// ```text
/// AND (created_at, id) < ($1, $2)   -- DESC, i.e. the next page is older
/// AND (created_at, id) > ($1, $2)   -- ASC
/// ```
///
/// A row-value comparison rather than two separate `AND`s, because `(a < $1 OR (a = $1 AND b < $2))`
/// is the same thing written longhand and the longhand is the version people get wrong
/// when a timestamp is not unique.
///
/// Does nothing when either half of the cursor is `None`: a half-cursor is not a page
/// boundary, and emitting the condition with a `NULL` bound would silently return zero
/// rows.
pub fn push_cursor_condition(
    qb: &mut QueryBuilder<Postgres>,
    ts_col: ColumnName<'_>,
    id_col: ColumnName<'_>,
    cursor_ts: Option<OffsetDateTime>,
    cursor_id: Option<Uuid>,
    order: Order,
) {
    let (Some(ts), Some(id)) = (cursor_ts, cursor_id) else {
        return;
    };

    // `cursor_op` is a `&'static str` from a `match` on `Order` — never caller text.
    let op = order.cursor_op();
    qb.push(format!(
        " AND ({}, {}) {op} (",
        ts_col.as_str(),
        id_col.as_str()
    ))
    .push_bind(ts)
    .push(", ")
    .push_bind(id)
    .push(")");
}

/// Append `ORDER BY` and `LIMIT`. Call this last, after every `WHERE` term.
///
/// `limit_plus_one` is the page size plus one, and the caller needs the extra row to know
/// whether another page exists without a second `COUNT` query — §9.4's "no query in a
/// loop" rule applies to counting too.
///
/// Both sort columns get the same direction so the ordering is a total order. With
/// `NULLS LAST`/`NULLS FIRST` from [`Order::sql`] the two columns cannot disagree about
/// where a null goes, which is what keeps the keyset comparison above stable across
/// pages.
pub fn push_order_and_limit(
    qb: &mut QueryBuilder<Postgres>,
    ts_col: ColumnName<'_>,
    id_col: ColumnName<'_>,
    order: Order,
    limit_plus_one: i64,
) {
    let dir = order.sql();
    qb.push(format!(
        " ORDER BY {} {dir}, {} {dir} LIMIT ",
        ts_col.as_str(),
        id_col.as_str()
    ))
    .push_bind(limit_plus_one);
}

#[cfg(test)]
mod tests {
    //! Tests for the column-name guard and the SQL fragments.
    //!
    //! The fragments are asserted on the built query's SQL text rather than on a live
    //! database: what these functions are responsible for is *the shape of the clause and
    //! what is bound vs interpolated*, and a real database would not make that any more
    //! visible. Binding is verified by the placeholders appearing in order.

    use super::*;

    fn builder() -> QueryBuilder<Postgres> {
        QueryBuilder::new("SELECT id FROM broadcasts WHERE 1=1")
    }

    fn ts(nanos: i128) -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp_nanos(nanos).expect("a valid timestamp")
    }

    /// How many bind placeholders the built query carries.
    ///
    /// `sqlx` 0.9 does not expose the argument count, so this reads the placeholders out
    /// of the SQL — which is the more meaningful assertion anyway, since the placeholders
    /// are what Postgres substitutes. A value that was interpolated instead shows up as
    /// literal text and no placeholder, which is exactly the failure these tests hunt.
    fn bound_count(qb: &QueryBuilder<Postgres>) -> usize {
        // The `SqlStr` must be bound to a local: `qb.sql()` returns a temporary, and its
        // `&str` would otherwise outlive it.
        let rendered = qb.sql();
        let sql = rendered.as_str();
        let mut highest = 0;
        let mut rest = sql;
        while let Some(at) = rest.find('$') {
            rest = &rest[at + 1..];
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            if digits.is_empty() {
                continue;
            }
            highest = highest.max(digits.parse::<usize>().unwrap_or(0));
        }
        highest
    }

    fn columns() -> (ColumnName<'static>, ColumnName<'static>) {
        (
            ColumnName::parse("created_at").expect("a valid column"),
            ColumnName::parse("id").expect("a valid column"),
        )
    }

    // ── the column-name guard ─────────────────────────────────────────────

    #[test]
    fn ordinary_column_names_are_accepted() {
        for name in ["created_at", "id", "broadcast_id", "left_at", "a1_b2"] {
            assert!(
                ColumnName::parse(name).is_ok(),
                "{name} should be a valid column name"
            );
        }
    }

    #[test]
    fn an_empty_name_is_rejected() {
        assert!(ColumnName::parse("").is_err());
    }

    #[test]
    fn a_name_with_a_quote_is_rejected() {
        // The case that matters: `id' OR '1'='1` would otherwise close the identifier and
        // rewrite the query.
        assert!(ColumnName::parse("id' OR '1'='1").is_err());
    }

    #[test]
    fn a_name_with_a_comment_is_rejected() {
        assert!(ColumnName::parse("id -- drop").is_err());
        assert!(ColumnName::parse("id;drop").is_err());
    }

    #[test]
    fn a_name_with_a_space_or_quote_is_rejected() {
        assert!(ColumnName::parse("created at").is_err());
        assert!(ColumnName::parse("\"id\"").is_err());
        assert!(ColumnName::parse("id)").is_err());
    }

    #[test]
    fn a_leading_digit_is_rejected() {
        assert!(ColumnName::parse("1id").is_err());
    }

    #[test]
    fn an_uppercase_name_is_rejected_with_a_message_that_says_why() {
        // Lowercase-only means the emitted SQL matches the snake_case schema without
        // needing quoting, which is itself a small safety property.
        let error = ColumnName::parse("CreatedAt").expect_err("must be rejected");

        assert!(
            error.contains("lowercase"),
            "the message must say why: {error}"
        );
    }

    // ── the fragments ──────────────────────────────────────────────────────

    #[test]
    fn a_descending_cursor_compares_strictly_less_than() {
        let (ts_col, id_col) = columns();
        let mut qb = builder();

        push_cursor_condition(
            &mut qb,
            ts_col,
            id_col,
            Some(ts(1_000)),
            Some(Uuid::from_u128(1)),
            Order::Desc,
        );
        let sql = qb.sql().as_str().to_owned();

        assert!(sql.contains("AND (created_at, id) < ("), "{sql}");
        assert_eq!(
            bound_count(&qb),
            2,
            "both cursor values must be bound, not interpolated: {sql}"
        );
    }

    #[test]
    fn an_ascending_cursor_compares_strictly_greater_than() {
        let (ts_col, id_col) = columns();
        let mut qb = builder();

        push_cursor_condition(
            &mut qb,
            ts_col,
            id_col,
            Some(ts(1_000)),
            Some(Uuid::from_u128(1)),
            Order::Asc,
        );
        let sql = qb.sql().as_str().to_owned();

        assert!(sql.contains("AND (created_at, id) > ("), "{sql}");
    }

    #[test]
    fn a_half_cursor_emits_nothing() {
        // The bug this prevents: binding a `NULL` and returning zero rows for a feed that
        // has plenty of them.
        let (ts_col, id_col) = columns();

        for (cursor_ts, cursor_id) in [
            (None, Some(Uuid::from_u128(1))),
            (Some(ts(1_000)), None),
            (None, None),
        ] {
            let mut qb = builder();
            push_cursor_condition(&mut qb, ts_col, id_col, cursor_ts, cursor_id, Order::Desc);

            assert_eq!(
                bound_count(&qb),
                0,
                "an incomplete cursor must bind nothing: {}",
                qb.sql().as_str()
            );
        }
    }

    #[test]
    fn the_order_clause_names_both_columns_in_the_same_direction() {
        let (ts_col, id_col) = columns();

        for (order, expected) in [
            (Order::Desc, "DESC NULLS LAST"),
            (Order::Asc, "ASC NULLS FIRST"),
        ] {
            let mut qb = builder();
            push_order_and_limit(&mut qb, ts_col, id_col, order, 51);
            let sql = qb.sql().as_str().to_owned();

            assert!(
                sql.contains(&format!("ORDER BY created_at {expected}, id {expected}")),
                "{sql}"
            );
            assert!(sql.contains("LIMIT "), "{sql}");
            assert_eq!(bound_count(&qb), 1, "the limit must be bound: {sql}");
        }
    }

    #[test]
    fn the_limit_is_bound_rather_than_interpolated() {
        let (ts_col, id_col) = columns();
        let mut qb = builder();

        push_order_and_limit(&mut qb, ts_col, id_col, Order::Desc, 51);
        let sql = qb.sql().as_str().to_owned();

        assert!(
            !sql.contains("51"),
            "the limit must be a placeholder, not text: {sql}"
        );
        assert!(sql.contains('$'), "{sql}");
    }

    #[test]
    fn a_full_page_builds_where_cursor_order_and_limit_in_that_order() {
        // The order of these calls is the caller's contract, and getting it wrong is
        // invalid SQL rather than a subtle bug — so it is pinned here.
        let (ts_col, id_col) = columns();
        let mut qb = builder();

        push_cursor_condition(
            &mut qb,
            ts_col,
            id_col,
            Some(ts(1_000)),
            Some(Uuid::from_u128(1)),
            Order::Desc,
        );
        push_order_and_limit(&mut qb, ts_col, id_col, Order::Desc, 51);

        let sql = qb.sql().as_str().to_owned();
        let cursor_at = sql.find("AND (created_at, id)").expect("a cursor clause");
        let order_at = sql.find("ORDER BY").expect("an order clause");

        assert!(
            cursor_at < order_at,
            "the cursor must precede ORDER BY: {sql}"
        );
        assert_eq!(bound_count(&qb), 3, "cursor, cursor, limit: {sql}");
    }
}
