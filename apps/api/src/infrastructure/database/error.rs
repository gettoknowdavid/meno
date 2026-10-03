//! Database failures, and the one place a `sqlx::Error` becomes a `meno_core::Error`.
//!
//! # Why this is the boundary (§5.5)
//!
//! §5.5's named leak is that `AuthError` and `BroadcastError` wrapped `sqlx::Error`
//! directly, so domain code was coupled to the database driver. The fix the plan gives is
//! to move the conversion here, where driver types are legitimate, and hand domain code a
//! driver-free error.
//!
//! [`db_err`] is that function. Everything else in the codebase reaches a database fault
//! through it, which is why this file is the *only* place in `apps/api` that mentions
//! `sqlx::Error` by name — a grep for it should find this file and the adapters that
//! genuinely need the driver.
//!
//! # Why `Internal` and not a `Database` variant
//!
//! §4.2 sketches an `Error::Database(sqlx::Error)` variant. It was not carried into
//! `crates/core::Error`, and for a good reason: that enum lives in `crates/core`, which
//! has no `sqlx` dependency, and adding one to hold a driver type in the type that reaches
//! the client is the §5.5 leak wearing a different hat.
//!
//! [`Internal`] carries a `&'static str` context and a `String` detail. The context is
//! logged and correlated by request id; the detail is the driver's own text, which
//! `to_body` replaces with "An internal error occurred" — §9.1's "no leaked internals"
//! and "no SQL strings in responses" in one mechanism.

use meno_core::{Error as MenoError, ErrorCode};

/// The service name used in log fields for database faults.
pub const SERVICE_NAME: &str = "postgres";

/// A `sqlx::Error` at the layer boundary.
///
/// Not something a caller is expected to handle: by the time an error reaches the layer
/// above the repository, it is either a driver fault worth logging and nothing more, or a
/// caller mistake the repository should have caught. This type exists so the *logging*
/// has somewhere to live before it is erased.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DatabaseError {
    /// The pool could not be created or reached the database.
    #[error("could not connect to {SERVICE_NAME}: {0}")]
    Connect(String),

    /// Migrations did not complete.
    ///
    /// Distinct from [`Self::Connect`] because the remedy is different: a connection
    /// failure is usually credentials or network, while a migration failure means the
    /// schema is behind the code and the process must not serve traffic (§4.3).
    #[error("migrations failed: {0}")]
    Migration(String),

    /// A statement failed.
    #[error("database query failed: {0}")]
    Query(String),
}

impl DatabaseError {
    /// The stable wire code.
    ///
    /// `Internal` for all three. A caller cannot act on "Postgres is unhappy", and a 500
    /// with `INTERNAL_ERROR` is the honest answer; the plan reserves 503 for *optional
    /// upstreams* going away (§4.2), which is what `Upstream` means, and the database is
    /// not optional.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        ErrorCode::Internal
    }
}

impl From<sqlx::Error> for DatabaseError {
    /// Erase the driver error, keeping the layer it happened in.
    fn from(error: sqlx::Error) -> Self {
        Self::Query(error.to_string())
    }
}

/// Turn a driver error into the single application error type.
///
/// The §5.5 boundary. `context` must be a `&'static str` naming the operation — `"load_broadcast"`,
/// not `"SELECT ... WHERE ..."` — because it becomes a log field and, through
/// [`MenoError::Internal::context`], part of the operator-visible failure name.
///
/// Passing a query string here would defeat the point: the detail is logged, but a SQL
/// string in a `context` that reaches a log aggregator is a schema-disclosure risk and a
/// card table for anyone reading Grafana.
#[must_use]
pub fn db_err(context: &'static str, error: impl std::fmt::Display) -> MenoError {
    MenoError::Internal {
        context,
        detail: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    //! Tests for the boundary translation.

    use super::*;

    #[test]
    fn a_driver_error_becomes_an_internal_error_carrying_its_context() {
        let meno = db_err("load_broadcast", "relation \"broadcasts\" does not exist");

        assert_eq!(meno.code(), ErrorCode::Internal);
        match &meno {
            MenoError::Internal { context, detail } => {
                assert_eq!(*context, "load_broadcast");
                assert!(detail.contains("does not exist"));
            }
            other => panic!("expected an Internal error, got {other:?}"),
        }
    }

    #[test]
    fn a_database_fault_is_never_client_safe() {
        // §9.1: infrastructure errors are logged with full context and returned as a
        // generic message.
        assert!(!db_err("load_broadcast", "boom").is_client_safe());
    }

    #[test]
    fn the_rendered_body_carries_no_schema_detail() {
        // The guarantee that matters. `Debug` legitimately holds the detail for the log;
        // `to_body` is what the client receives.
        let meno = db_err("load_broadcast", r#"relation "users" does not exist"#);

        let body = meno_core::to_body(&meno);

        assert_eq!(body.http_status, 500);
        assert_eq!(body.code, ErrorCode::Internal.as_str());
        assert_eq!(body.message, "An internal error occurred");
        assert!(
            !body.message.contains("users"),
            "a table name must never reach the client: {}",
            body.message
        );
        assert!(!format!("{body:?}").contains("relation"));
    }

    #[test]
    fn every_database_error_maps_to_one_wire_code() {
        // Consistency matters more than precision here: a client that branches on the code
        // must not have to handle three shapes of "the database is unhappy".
        let all = [
            DatabaseError::Connect("refused".to_owned()),
            DatabaseError::Migration("0003 failed".to_owned()),
            DatabaseError::Query("deadlock".to_owned()),
        ];

        for error in all {
            assert_eq!(error.code(), ErrorCode::Internal, "{error}");
        }
    }

    #[test]
    fn a_connection_failure_mentions_the_service() {
        let error = DatabaseError::Connect("connection refused".to_owned());

        assert!(
            error.to_string().contains("postgres"),
            "the message must name the dependency: {error}"
        );
    }

    #[test]
    fn a_migration_failure_is_distinguishable_from_a_query_failure() {
        // Different remedy: a migration failure means the schema is behind the code.
        let migration = DatabaseError::Migration("no such table".to_owned());
        let query = DatabaseError::Query("no such table".to_owned());

        assert_ne!(migration.to_string(), query.to_string());
    }
}
