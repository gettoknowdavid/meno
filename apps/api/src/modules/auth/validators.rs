//! Argument validation at the request boundary (plan §9.5).
//!
//! Every function here answers one question — "is this field acceptable?" — and answers
//! it with a list of *messages keyed by field*, because §4.2 renders `Error::Validation`'s
//! `fields` map straight to the client. A validator that returned one flat string would
//! force the envelope to flatten it, and then a Flutter form could not highlight the
//! offending input.
//!
//! # Why not `validator`'s derive attributes
//!
//! The previous revision used `#[validate(custom(function = "validate_password"))]` plus
//! `ValidationError::new("email")`. Two problems, both of which this module removes:
//!
//! - the field name was a string in an attribute, so renaming the struct field left the
//!   error key pointing at nothing, with no compiler involved;
//! - `ValidationErrors` cannot express two rules failing on one field in a way the §4.2
//!   envelope can carry, so the code joined them into one comma-separated message and
//!   lost the structure.
//!
//! Hand-written rules are longer and are checked by the compiler when a field is renamed.
//!
//! # Untrusted input
//!
//! All of it. These functions run on strings a stranger sent, so nothing here allocates
//! proportionally to the input beyond the message it returns, and nothing panics.

/// The field name used in [`crate::modules::auth::dto`] for the email address.
///
/// A constant because it appears in four request types, and a typo in one of them would
/// put the message under a key no client looks at.
pub const EMAIL: &str = "email";

/// The field name used for a password or new password.
pub const PASSWORD: &str = "password";

/// The field name used for a one-time code.
pub const CODE: &str = "code";

/// The field name used for a full name.
pub const FULL_NAME: &str = "full_name";

/// The field name used for a device label.
pub const DEVICE_LABEL: &str = "device_label";

/// Longest address that can exist, per RFC 5321 §4.5.3.1.
const MAX_EMAIL_LEN: usize = 254;

/// Longest local part, per RFC 5321 §4.5.3.1.
const MAX_LOCAL_LEN: usize = 64;

/// Longest DNS label, per RFC 1035 §2.3.4.
const MAX_LABEL_LEN: usize = 63;

/// Shortest password this service accepts.
pub const MIN_PASSWORD_LEN: usize = 8;

/// Longest password this service accepts.
///
/// Not a policy statement but a cost one: Argon2's memory and time are independent of
/// input length, but its *allocation* is not, and an unbounded password field is a free
/// way for a client to make the server allocate. §9.4's point about unbounded reads
/// applies to a 10 MB string in a JSON body as much as to a 10 MB upload.
pub const MAX_PASSWORD_LEN: usize = 128;

/// Longest display name, matching the `users` table's intent.
pub const MAX_FULL_NAME_LEN: usize = 100;

/// Shortest display name.
///
/// Two, not one: a single character is not a name, and rejecting it now is cheaper than
/// rendering an empty-looking byline forever.
pub const MIN_FULL_NAME_LEN: usize = 2;

/// Longest device label. Matches `auth_sessions.device_label`.
pub const MAX_DEVICE_LABEL_LEN: usize = 120;

/// Why an email address was rejected.
///
/// Returned rather than `Result<(), String>` so the caller cannot accidentally treat
/// "malformed" and "missing" as the same thing — the service tells the client to fix a
/// typo in one case and to sign in with the address they already have in the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmailProblem {
    /// The field was empty or only whitespace.
    Missing,
    /// The address has no `@`, or more than one.
    NoAtSign,
    /// The part before the `@` is unusable.
    BadLocalPart,
    /// The part after the `@` is unusable.
    BadDomain,
    /// The address exceeds 254 characters.
    TooLong,
}

impl EmailProblem {
    /// The message sent to the client, under the `email` key.
    ///
    /// Every message names what is wrong without naming what a correct address looks
    /// like in full. Echoing the submitted address back would put attacker-controlled
    /// text into a UI, and it tells the user nothing they cannot see in their own input.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::Missing => "An email address is required",
            Self::NoAtSign => "An email address must contain exactly one '@'",
            Self::BadLocalPart => {
                "The part before '@' may not be empty, start or end with '.', or contain '..'"
            }
            Self::BadDomain => {
                "The part after '@' must be a domain such as 'example.com', without a \
                 leading or trailing '-'"
            }
            Self::TooLong => "An email address may be at most 254 characters",
        }
    }
}

/// Check an email address, returning the first problem found.
///
/// # Why the domain is split on `.`
///
/// `a@b` is a syntactically valid address that no public mail provider accepts, and
/// accepting it means accepting an address nothing can be delivered to. Requiring at
/// least one dot keeps a typo like `ada@localhost` out of the table, which is also what
/// stops it being registered as an unclaimable account.
#[must_use]
pub fn email_problem(address: &str) -> Option<EmailProblem> {
    let trimmed = address.trim();

    if trimmed.is_empty() {
        return Some(EmailProblem::Missing);
    }
    if trimmed.len() > MAX_EMAIL_LEN {
        return Some(EmailProblem::TooLong);
    }
    // `split_once` handles "exactly one @"; a second one leaves an `@` in the domain,
    // which `bad_domain` rejects below.
    let Some((local, domain)) = trimmed.split_once('@') else {
        return Some(EmailProblem::NoAtSign);
    };
    if domain.contains('@') {
        return Some(EmailProblem::NoAtSign);
    }

    // Whitespace in the *local* part is a rejection rather than a trim: an address
    // containing a space is a paste error, and silently repairing it registers an
    // account at an address the user did not type.
    //
    // Checked after the split rather than before, so a space in the *domain* is reported
    // as [`EmailProblem::BadDomain`] — the message then names the part that is actually
    // wrong, which is what the user needs to see.
    //
    // A *quoted* local part is the exception, and the exception is the RFC's: the quotes
    // are part of the syntax, so `"Fred Bloggs"@example.com` is a well-formed addr-spec.
    // The bare `Fred Bloggs@example.com` is not, and no provider delivers to it. What is
    // checked is that the quotes are balanced and actually enclose the whitespace, so a
    // stray `"` cannot smuggle a space past this.
    let unquoted = local.strip_prefix('"').and_then(|r| r.strip_suffix('"'));
    match unquoted {
        Some(inner) if inner.chars().all(|c| !c.is_control()) => {}
        _ if local.chars().any(char::is_whitespace) => {
            return Some(EmailProblem::BadLocalPart);
        }
        _ => {}
    }

    if local.is_empty() || local.len() > MAX_LOCAL_LEN {
        return Some(EmailProblem::BadLocalPart);
    }
    if local.starts_with('.') || local.ends_with('.') || local.contains("..") {
        return Some(EmailProblem::BadLocalPart);
    }

    if bad_domain(domain) {
        return Some(EmailProblem::BadDomain);
    }

    None
}

/// Whether `domain` is unusable as a mail domain.
///
/// The character set is the intersection of what RFC 5321 permits in a domain and what
/// DNS permits in a label, which is exactly the set a mail server will accept. A
/// non-ASCII domain (IDN) is rejected rather than punycoded: punycoding needs a table,
/// and a registration form that silently rewrites a user's address is worse than one
/// that tells them to type the ASCII form their provider already uses.
fn bad_domain(domain: &str) -> bool {
    if domain.is_empty() || !domain.contains('.') {
        return true;
    }
    // A trailing dot is legal in DNS (the root) and never what a user typed.
    if domain.ends_with('.') {
        return true;
    }

    domain.split('.').any(|label| {
        label.is_empty()
            || label.len() > MAX_LABEL_LEN
            || label.starts_with('-')
            || label.ends_with('-')
            || !label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
}

/// Check a password, returning one message per unmet rule.
///
/// # The rule set
///
/// Length 8-128, at least one lowercase and one uppercase letter.
///
/// This is deliberately *not* changed from the previous revision's policy — tightening it
/// would silently reject passwords that were valid at registration, and the plan says
/// nothing about composition rules. It is worth revisiting on its own terms: NIST
/// SP 800-63B advises length and a breached-password check *instead of* composition
/// rules, and this function currently does the weaker half of the pair. What it does
/// keep is the maximum, which is a cost bound rather than a policy (§9.4).
#[must_use]
pub fn password_problems(password: &str) -> Vec<&'static str> {
    let mut problems = Vec::new();

    // `.chars().count()` rather than `.len()`: "8 characters" has to mean the same
    // thing for a user whose password contains an emoji.
    match password.chars().count() {
        n if n < MIN_PASSWORD_LEN => problems.push("A password must be at least 8 characters"),
        n if n > MAX_PASSWORD_LEN => problems.push("A password may be at most 128 characters"),
        _ => {}
    }

    if !password.chars().any(char::is_lowercase) {
        problems.push("A password must contain at least one lowercase letter");
    }
    if !password.chars().any(char::is_uppercase) {
        problems.push("A password must contain at least one uppercase letter");
    }

    problems
}

/// Check a one-time code.
///
/// Six digits, because that is what [`crate::modules::auth::mailer`] sends; accepting
/// other lengths would let a caller guess that a four-character code is also valid and
/// brute-force a much smaller space.
#[must_use]
pub fn code_problem(code: &str) -> Option<&'static str> {
    if code.len() != 6 || !code.bytes().all(|byte| byte.is_ascii_digit()) {
        return Some("A verification code is exactly 6 digits");
    }
    None
}

/// Check a display name, returning the messages for every rule it breaks.
#[must_use]
pub fn full_name_problems(name: &str) -> Vec<&'static str> {
    let mut problems = Vec::new();
    let trimmed = name.trim();

    match trimmed.chars().count() {
        n if n < MIN_FULL_NAME_LEN => problems.push("A full name is required"),
        n if n > MAX_FULL_NAME_LEN => {
            problems.push("A full name may be at most 100 characters");
        }
        _ => {}
    }
    if trimmed.chars().any(char::is_control) {
        problems.push("A full name may not contain control characters");
    }

    problems
}

/// Check a device label, returning why it is unacceptable or `None`.
///
/// A label is optional, so emptiness is not a problem — only a label that is present and
/// unusable is. Rejecting it at the boundary keeps an unprintable string out of the
/// "your devices" list a user reads to decide what to revoke.
#[must_use]
pub fn device_label_problem(label: &str) -> Option<&'static str> {
    let trimmed = label.trim();

    if trimmed.is_empty() {
        return None;
    }
    if trimmed.chars().count() > MAX_DEVICE_LABEL_LEN {
        return Some("A device label may be at most 120 characters");
    }
    if trimmed.chars().any(char::is_control) {
        return Some("A device label may not contain control characters");
    }

    None
}

#[cfg(test)]
mod tests {
    //! The rules, and the shapes they exist to reject.
    //!
    //! Each rejection gets a test rather than a table row: the reason a rule exists is
    //! the interesting part, and a shared table would record only that the function
    //! returned something.

    use super::*;

    fn accepted(address: &str) -> bool {
        email_problem(address).is_none()
    }

    // ── email ──────────────────────────────────────────────────────────────

    #[test]
    fn ordinary_addresses_are_accepted() {
        for address in [
            "ada@example.com",
            "ada.lovelace@example.co.uk",
            "ada+menopause@example.com",
            "a@b.co",
            "ada_1@example-site.com",
            // The RFC's own example, which is deliberately unusual — note the *quotes*,
            // which are part of the syntax. The bare `Fred Bloggs@example.com` below is
            // refused, and that is correct rather than pedantic.
            "\"quoted string\"@example.com",
        ] {
            assert!(
                accepted(address),
                "{address:?} should be accepted, got {:?}",
                email_problem(address)
            );
        }
    }

    #[test]
    fn an_absent_address_is_distinguished_from_a_malformed_one() {
        // The service reacts differently: a missing address is a form that was not
        // filled in, a malformed one is a typo worth naming.
        assert_eq!(email_problem("   "), Some(EmailProblem::Missing));
        assert_eq!(email_problem("ada"), Some(EmailProblem::NoAtSign));
        assert_eq!(
            email_problem("ada@@example.com"),
            Some(EmailProblem::NoAtSign)
        );
        assert_eq!(email_problem("ada@"), Some(EmailProblem::BadDomain));
    }

    #[test]
    fn a_local_part_with_dot_rules_is_rejected() {
        for address in [".ada@example.com", "ada.@example.com", "ad..a@example.com"] {
            assert_eq!(
                email_problem(address),
                Some(EmailProblem::BadLocalPart),
                "{address:?}"
            );
        }
    }

    #[test]
    fn a_domain_that_no_mail_provider_would_accept_is_rejected() {
        // `a@b` is syntactically fine and practically undeliverable; registering it
        // would create an account nobody can receive mail for.
        for address in [
            "ada@localhost",
            "ada@example",
            "ada@.com",
            "ada@example.",
            "ada@exa mple.com",
            "ada@-example.com",
            "ada@example-.com",
            "ada@example..com",
        ] {
            assert_eq!(
                email_problem(address),
                Some(EmailProblem::BadDomain),
                "{address:?}"
            );
        }
    }

    #[test]
    fn an_over_long_address_is_rejected_before_it_is_parsed() {
        // A 300-character address is refused as too long rather than falling out of
        // local-part checking, so the message names the actual problem.
        let long = format!("{}@example.com", "a".repeat(MAX_EMAIL_LEN));
        assert_eq!(email_problem(&long), Some(EmailProblem::TooLong));
    }

    #[test]
    fn whitespace_inside_an_address_is_rejected_rather_than_trimmed() {
        // Trimming would register an account at an address the user did not type.
        assert_eq!(
            email_problem("ada lovelace@example.com"),
            Some(EmailProblem::BadLocalPart)
        );
        // A space in the domain is a *domain* problem, and the message says so — the
        // user is looking at the part after the `@` and needs to be told about that part.
        assert_eq!(
            email_problem("ada@exa mple.com"),
            Some(EmailProblem::BadDomain)
        );
        // Whitespace anywhere in the domain, not just a space.
        assert_eq!(
            email_problem("ada@exa\nmple.com"),
            Some(EmailProblem::BadDomain)
        );
    }

    #[test]
    fn surrounding_whitespace_is_tolerated_because_phones_add_it() {
        // A phone keyboard autocorrects a trailing space onto the whole form. Refusing
        // the login over a character the user cannot see would be a support ticket.
        assert!(accepted("  ada@example.com  "));
    }

    #[test]
    fn every_email_problem_has_a_distinct_message() {
        // Two variants sharing a message means the client cannot tell them apart.
        let problems = [
            EmailProblem::Missing,
            EmailProblem::NoAtSign,
            EmailProblem::BadLocalPart,
            EmailProblem::BadDomain,
            EmailProblem::TooLong,
        ];
        for (i, a) in problems.iter().enumerate() {
            for b in &problems[i + 1..] {
                assert_ne!(a.message(), b.message(), "{a:?} and {b:?} share a message");
            }
        }
    }

    // ── password ───────────────────────────────────────────────────────────

    #[test]
    fn a_conforming_password_has_no_problems() {
        assert!(password_problems("Correct Horse9").is_empty());
    }

    #[test]
    fn a_short_password_reports_only_its_length() {
        // "Abc1" fails both length and nothing else; reporting a composition problem it
        // does not have would be noise.
        assert_eq!(
            password_problems("Abc1"),
            vec!["A password must be at least 8 characters"]
        );
    }

    #[test]
    fn length_counts_characters_rather_than_bytes() {
        // Four emoji are eight bytes and one character. Byte length would let a
        // four-emoji password through as "8 characters".
        let four_emoji: String = std::iter::repeat_n('🦀', 4).collect();
        assert_eq!(four_emoji.chars().count(), 4);
        // Emoji have no case, so all three rules fire — and the length message is the
        // one byte-length would have got wrong, which is the point of the assertion.
        assert_eq!(
            password_problems(&four_emoji),
            vec![
                "A password must be at least 8 characters",
                "A password must contain at least one lowercase letter",
                "A password must contain at least one uppercase letter",
            ]
        );

        // Four emoji *plus* case: the length message goes away once the character count
        // is 8, which is what "characters rather than bytes" means in practice.
        let eight = format!("Aa{four_emoji}{four_emoji}");
        assert_eq!(eight.chars().count(), 10);
        assert!(
            password_problems(&eight).is_empty(),
            "a valid password has no problems"
        );
    }

    #[test]
    fn a_long_password_is_refused_on_the_cost_bound_alone() {
        // No composition problem here — it is purely the Argon2 allocation bound (§9.4).
        let long = "Aa".repeat(MAX_PASSWORD_LEN);
        assert_eq!(
            password_problems(&long),
            vec!["A password may be at most 128 characters"]
        );
    }

    #[test]
    fn composition_problems_are_reported_individually() {
        // Each rule gets its own entry so a client can highlight what to fix, rather
        // than one joined sentence (the reason this module stopped using
        // `validator`'s single-message custom function).
        let problems = password_problems("short");
        assert!(problems.contains(&"A password must be at least 8 characters"));
        // "short" is all lowercase, so the message that *must* appear is the uppercase
        // one. Asserting the lowercase message here would pass an inverted check, which
        // is exactly the bug this test exists to catch.
        assert!(!problems.contains(&"A password must contain at least one lowercase letter"));
        assert!(problems.contains(&"A password must contain at least one uppercase letter"));

        // And the mirror case: an all-caps password is short *and* missing a lowercase.
        let all_caps = password_problems("SHORT");
        assert!(all_caps.contains(&"A password must contain at least one lowercase letter"));
        assert!(!all_caps.contains(&"A password must contain at least one uppercase letter"));
    }

    #[test]
    fn a_password_of_only_uppercase_letters_reports_only_that() {
        assert_eq!(
            password_problems("ALLUPPERCASE"),
            vec!["A password must contain at least one lowercase letter"]
        );
    }

    // ── code ───────────────────────────────────────────────────────────────

    #[test]
    fn a_code_must_be_six_digits() {
        assert_eq!(code_problem("123456"), None);
        assert!(
            code_problem("12345").is_some(),
            "five digits is a smaller brute-force space"
        );
        assert!(code_problem("1234567").is_some());
        assert!(code_problem("12345a").is_some());
        assert!(code_problem("").is_some());
    }

    // ── full name ──────────────────────────────────────────────────────────

    #[test]
    fn a_full_name_is_length_checked_and_control_free() {
        assert!(full_name_problems("Ada Lovelace").is_empty());

        assert_eq!(full_name_problems(""), vec!["A full name is required"]);
        assert_eq!(full_name_problems("A"), vec!["A full name is required"]);
        assert_eq!(
            full_name_problems(&"a".repeat(101)),
            vec!["A full name may be at most 100 characters"]
        );

        // A newline in a byline breaks every UI that renders it, and it is the cheapest
        // possible log-injection payload.
        assert_eq!(
            full_name_problems("Ada\nLovelace"),
            vec!["A full name may not contain control characters"]
        );
    }

    // ── device label ───────────────────────────────────────────────────────

    #[test]
    fn a_device_label_is_optional_but_must_be_usable_when_given() {
        assert_eq!(device_label_problem(""), None);
        assert_eq!(device_label_problem("   "), None);
        assert!(device_label_problem("Ada's iPhone").is_none());
        assert!(device_label_problem(&"x".repeat(121)).is_some());
        assert!(device_label_problem("iPad\u{0}2").is_some());
    }
}
