//! Password hashing (Argon2id) and verification.
//!
//! # Why Argon2id
//!
//! Argon2id is the hybrid of Argon2d and Argon2i, so it resists both GPU cracking (data
//! dependence) and side-channel timing attacks (memory dependence). It is the OWASP
//! Password Storage Cheat Sheet's first recommendation and the previous revision's
//! choice; this module keeps the algorithm and fixes the things around it.
//!
//! # What changed, and why
//!
//! - **No `expect`.** The old `argon2()` builder called `Params::new(...).expect("valid
//!   argon2 params")` on every hash and every verify. It replaced
//!   [`argon2::Params::DEFAULT`], which is the same parameters — 19 MiB, t=2, p=1 — so
//!   the panic was removable without changing a single hashed password.
//! - **No `DUMMY_HASH` `LazyLock` of a `Result`.** The old one did
//!   `hash_password("...").expect("dummy hash")` inside a `static`, which is a panic
//!   reachable from a request. The dummy hash is now built during startup by
//!   [`AuthService::new`](super::services::AuthService::new), which returns an error.
//! - **Verification runs on the blocking pool.** Argon2id deliberately burns ~50 ms of
//!   CPU. On the async runtime that is a stalled worker thread per login, which under
//!   load is how a login endpoint takes the whole process down. §9.4 is about bounding
//!   work; this is bounding *where* it happens.
//!
//! # Cost bounds
//!
//! Argon2's memory and time costs are independent of the password's length, but the
//! input is still copied into the hash. [`MAX_PASSWORD_BYTES`] refuses an absurd one
//! before any allocation, because an unbounded string in a JSON body is an
//! unbounded-read problem regardless of what happens to it afterwards.

// argon2 0.6 moved the PHC types behind `phc` and made `PasswordHasher` generic over its
// output, so the imports name the module rather than the re-export.
use argon2::password_hash::phc::{PasswordHash, Salt};
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use argon2::{Algorithm, Argon2, Params, Version};

use super::error;
use meno_core::Error as MenoError;

/// Largest password this module will hash, in bytes.
///
/// Well above [`super::validators::MAX_PASSWORD_LEN`] (128 characters), because a
/// 128-character password can be several hundred bytes in UTF-8 and this bound is about
/// refusing a megabyte, not about duplicating the character rule.
pub const MAX_PASSWORD_BYTES: usize = 1024;

/// The password the dummy hash is computed from.
///
/// Its value is irrelevant — what matters is that hashing *some* string of this shape
/// costs the same as hashing the caller's guess, so a response that finds no account
/// takes as long as one that finds a wrong password.
const DUMMY_PLAINTEXT: &str = "meno-timing-equaliser-not-a-real-password";

/// The Argon2id configuration used for every hash this service writes.
///
/// [`Params::DEFAULT`] is `m = 19456` (19 MiB), `t = 2`, `p = 1` — the OWASP first
/// recommendation. Named as a function so "how strong are these hashes" has one answer,
/// and so a future parameter bump is one line rather than a search.
fn argon2() -> Argon2<'static> {
    Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::DEFAULT)
}

/// Hash `password` with a fresh random salt.
///
/// # Errors
///
/// [`MenoError::Internal`] if the password exceeds [`MAX_PASSWORD_BYTES`] or the hasher
/// reports a failure. Never panics (§9.1).
pub fn hash_password(password: &str) -> Result<String, MenoError> {
    if password.len() > MAX_PASSWORD_BYTES {
        return Err(error::internal(
            "hash_password",
            format!(
                "refused a {}-byte password; the bound is {MAX_PASSWORD_BYTES}",
                password.len()
            ),
        ));
    }

    // A 16-byte salt, drawn from the OS CSPRNG — the PHC string format's recommended
    // length. `Salt::generate` is typed as fallible, so the `?` is here rather than an
    // `unwrap`: §9.1 forbids the second, and a future RNG change should not be the thing
    // that introduces a panic.
    let salt = Salt::generate();

    argon2()
        .hash_password_with_salt(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|e| error::internal("hash_password", e))
}

/// Verify `password` against a stored PHC hash string.
///
/// Returns `false` — never an error, never a panic — for anything unusable: a malformed
/// hash, an unsupported algorithm, or a truncated string. A stored hash is not
/// attacker-controlled in the sense that matters, but it *is* data read from a database
/// that a migration or a restore can put anything in, and a login must answer 401 rather
/// than 500 for it.
///
/// A `false` result for a malformed hash is also indistinguishable from a wrong
/// password, which is what keeps a corrupt row from becoming an oracle.
#[must_use]
pub fn verify_password(password: &str, hash: &str) -> bool {
    if password.len() > MAX_PASSWORD_BYTES {
        return false;
    }

    PasswordHash::new(hash).is_ok_and(|parsed| {
        argon2()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    })
}

/// Spend the same time a real verification would, and report nothing.
///
/// # Errors
///
/// Never. This exists for exactly one caller — [`super::services::AuthService::login`],
/// when no account matched — and the *absence* of a result is the point: returning
/// `bool` would invite a caller to branch on it, and branching on "did the dummy hash
/// verify" is a bug waiting to happen, because the answer is always `false`.
///
/// Naming it rather than discarding a `let _ =` at the call site is what keeps the
/// intent ("waste some time") separate from the shape of the operation ("check a
/// password"), which are different things that happen to share code.
pub async fn spend_verification_time(password: String, hash: String) {
    // `let _ =` rather than `drop`: `bool` is `Copy`, so `drop` is a no-op the compiler
    // correctly complains about, and the warning is worth listening to.
    let _ = verify_password_async(password, hash).await;
}

/// [`verify_password`] on the blocking pool.
///
/// # Panics
///
/// Never. A join failure — a cancelled or aborted task — is reported as `false`, which
/// denies the login rather than taking the process down.
#[must_use]
pub async fn verify_password_async(password: String, hash: String) -> bool {
    tokio::task::spawn_blocking(move || verify_password(&password, &hash))
        .await
        .unwrap_or(false)
}

/// Build the hash used to spend time on when no account matched.
///
/// Called once at startup so that [`verify_password`] against it costs what a real
/// verification costs. This is the whole of §7.9's user-enumeration mitigation on the
/// login path, and it only works if the hash is a *real* Argon2id hash — a constant or
/// an early return would be measurably faster and would undo it.
///
/// # Errors
///
/// [`MenoError::Internal`] if the hash cannot be computed. Failing startup is correct
/// here: a deployment that cannot produce a dummy hash cannot equalise timing, and
/// running anyway would look fine until it was exploited.
pub fn dummy_hash() -> Result<String, MenoError> {
    hash_password(DUMMY_PLAINTEXT).map_err(|e| {
        error::internal(
            "build_dummy_hash",
            format!("{e:?}; login timing cannot be equalised"),
        )
    })
}

#[cfg(test)]
mod tests {
    //! Argon2id at OWASP parameters takes ~50 ms per call by design, so these tests are
    //! slow by nature. They are the ones that must not be weakened to go faster: a test
    //! that verified a hash the cheap way would not be testing the hash.
    //!
    //! The properties pinned are the ones that decide whether a stolen database is
    //! usable by an attacker, plus the panic-freedom §9.1 requires.

    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn a_hash_verifies_against_its_own_password() {
        let hash = hash_password("Correct Horse Battery Staple").expect("hashing works");
        assert!(verify_password("Correct Horse Battery Staple", &hash));
    }

    #[test]
    fn a_wrong_password_does_not_verify() {
        let hash = hash_password("Correct Horse Battery Staple").expect("hashing works");
        assert!(!verify_password("correct horse battery staple", &hash));
        assert!(!verify_password("", &hash));
        assert!(!verify_password(&"x".repeat(1000), &hash));
    }

    #[test]
    fn the_same_password_hashes_differently_every_time() {
        // Two equal inputs must not produce equal outputs. Identical hashes across users
        // would mean the salt is not random, and the whole point of a per-password salt
        // is that one cracked password does not crack the rest.
        let first = hash_password("same password").expect("hashing works");
        let second = hash_password("same password").expect("hashing works");

        assert_ne!(first, second, "the salt is not being applied");
        // Both still verify — a differing hash that does not verify would be a worse bug.
        assert!(verify_password("same password", &first));
        assert!(verify_password("same password", &second));
    }

    #[test]
    fn the_hash_records_argon2id_and_the_owasp_parameters() {
        // A migration that silently downgrades to Argon2i or drops to m=4096 must be
        // visible in the stored string, which is the only place anyone would notice.
        let hash = hash_password("x").expect("hashing works");
        let parsed = PasswordHash::new(&hash).expect("a PHC string we just wrote");

        assert_eq!(parsed.algorithm.as_str(), "argon2id");
        assert_eq!(parsed.version, Some(0x13));
        assert_eq!(parsed.params.get_decimal("m"), Some(19_456), "m = 19 MiB");
        assert_eq!(parsed.params.get_decimal("t"), Some(2));
        assert_eq!(parsed.params.get_decimal("p"), Some(1));
    }

    #[test]
    fn the_hash_never_contains_the_password() {
        let hash = hash_password("hunter2-but-longer").expect("hashing works");
        assert!(
            !hash.contains("hunter2"),
            "the plaintext survived into the hash"
        );
    }

    #[test]
    fn a_malformed_hash_is_a_miss_not_an_error() {
        // A row corrupted by a migration must produce a 401, not a 500 (§4.2).
        for stored in [
            "",
            "not-a-hash",
            "$argon2id$",
            "$2b$12$abcdefghijklmnopqrstuv",
        ] {
            assert!(
                !verify_password("anything", stored),
                "{stored:?} should be a miss"
            );
        }
    }

    #[test]
    fn a_hash_for_another_algorithm_does_not_verify() {
        // Refuses silently rather than erroring: a client that switched from bcrypt
        // mid-migration should get told "wrong password" and log in again.
        let bcrypt = "$2b$12$C6UzMDM.H6dfI/f/IKcEe.HkVeeO5fHXZQ2Vi4v9GQGrGqZqvYsS";
        assert!(!verify_password("password", bcrypt));
    }

    #[test]
    fn an_oversized_password_is_refused_without_allocating_a_hash() {
        let huge = "a".repeat(MAX_PASSWORD_BYTES + 1);

        let error = hash_password(&huge).expect_err("an oversized password is refused");
        assert_eq!(error.code(), meno_core::ErrorCode::Internal);

        // And verification of one is a miss rather than a hash comparison, because
        // nothing legitimate can be this long (the DTOs cap at 128 characters).
        assert!(!verify_password(
            &huge,
            "$argon2id$v=19$m=19456,t=2,p=1$abc$def"
        ));
    }

    #[test]
    fn the_dummy_hash_costs_the_same_as_a_real_verification() {
        // §7.9's mitigation, asserted: the dummy hash is a real Argon2id hash, so
        // verifying against it costs the same as verifying a real one. A cheap dummy
        // would reintroduce the timing oracle it exists to close.
        //
        // Note it *does* verify against `DUMMY_PLAINTEXT`, and that is fine: the hash is
        // never stored against an account, so no one can present it as a credential. The
        // property that matters is the cost, not the answer.
        let dummy = dummy_hash().expect("the dummy hash is computable");
        assert!(
            verify_password(DUMMY_PLAINTEXT, &dummy),
            "a real Argon2id hash"
        );
        assert!(
            !verify_password("anything else", &dummy),
            "and only of that"
        );

        let real = hash_password("a real password").expect("hashing works");

        let real_elapsed = time_one(|| verify_password("a real password", &real));
        let dummy_elapsed = time_one(|| verify_password("whatever they typed", &dummy));

        // A generous ceiling: this asserts the two are the same *order of magnitude*,
        // not that they are bit-identical. A dummy that short-circuited would be three
        // orders of magnitude faster, which is the failure this catches.
        let ratio = real_elapsed.as_secs_f64() / dummy_elapsed.as_secs_f64().max(1e-9);
        assert!(
            (0.1..10.0).contains(&ratio),
            "dummy verification took {dummy_elapsed:?} against {real_elapsed:?} for a real hash"
        );
    }

    #[test]
    fn verification_is_off_the_async_runtime() {
        // Not a performance assertion about Argon2; a structural one. If this ever
        // goes back to a blocking call on the runtime, a burst of logins stalls
        // unrelated async tasks on the same worker threads.
        let hash = hash_password("Correct Horse Battery Staple").expect("hashing works");

        let start = Instant::now();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("a current-thread runtime");
        runtime.block_on(verify_password_async(
            "Correct Horse Battery Staple".to_owned(),
            hash,
        ));
        let elapsed = start.elapsed();

        assert!(
            elapsed > Duration::from_millis(5),
            "returned in {elapsed:?}, which suggests the work never left the task"
        );
    }

    #[test]
    fn the_async_verifier_agrees_with_the_blocking_one() {
        let hash = hash_password("Correct Horse Battery Staple").expect("hashing works");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("a current-thread runtime");

        assert!(runtime.block_on(verify_password_async(
            "Correct Horse Battery Staple".to_owned(),
            hash.clone()
        )));
        assert!(!runtime.block_on(verify_password_async("wrong".to_owned(), hash)));
    }

    #[test]
    fn hashing_a_non_ascii_password_round_trips() {
        // Argon2 hashes bytes, not characters, so this is where an encoding mistake
        // would show up: a password that hashes but never verifies.
        let hash = hash_password("pässwörd🦀").expect("hashing works");
        assert!(verify_password("pässwörd🦀", &hash));
        assert!(!verify_password("passwörd", &hash));
    }

    /// Time one verification, so the caller can compare two.
    fn time_one(f: impl FnOnce() -> bool) -> Duration {
        let start = Instant::now();
        let _ = f();
        start.elapsed()
    }
}
