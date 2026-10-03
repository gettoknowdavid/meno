//! Canonical id generation.
//!
//! Every id in Meno is a v4 UUID. Centralised so the one place that would ever need to
//! change — to v7, for index locality — is findable by grep.
//!
//! §4.7 keeps JWT + refresh rotation, and every table's primary key is a `uuid` column
//! defaulted with `gen_random_uuid()`. That database default is what protects rows
//! inserted outside the application (a migration backfill, a psql session, a seeding
//! script); this function exists so the *application* never rolls its own.

use uuid::Uuid;

/// Generate a new random v4 identifier.
#[must_use]
pub fn new_id() -> Uuid {
    Uuid::new_v4()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_unique_and_are_v4() {
        let mut seen = HashSet::new();
        for _ in 0..10_000 {
            let id = new_id();
            assert_eq!(id.get_version_num(), 4, "ids must be UUID v4");
            assert!(seen.insert(id), "ids must not collide");
        }
    }

    #[test]
    fn id_is_usable_as_a_cursor_component() {
        // Pagination encodes (timestamp, uuid) pairs, so an id that fails to
        // parse-string would only surface as a cursor failure at runtime.
        let id = new_id();
        assert_eq!(Uuid::parse_str(&id.to_string()).expect("parses"), id);
    }
}
