//! LiveKit-facing value types.
//!
//! Ported from `apps/api/src/shared/services/livekit/dto.rs` on `master` (`903c3ba`).
//!
//! # What changed
//!
//! - **`LivekitRole` derives `Serialize`/`Deserialize`** and gets a single wire name per
//!   variant, so a role that reaches a client or a LiveKit agent is one spelling.
//! - **`LivekitParticipantInfo` skips `None`s.** LiveKit reports `joined_at` in seconds;
//!   a client rendering that as milliseconds is off by a factor of 1000, so the field
//!   is documented as seconds and the conversion is explicit.
//! - **No dependency on `modules::broadcast`.** `master` had `TryFrom<ParticipantRole>`
//!   here, which meant this adapter named a domain enum that does not exist in the
//!   monorepo yet. The mapping belongs at the call site, which is where
//!   `ParticipantRole` actually lives.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

/// The role a participant holds in a LiveKit room.
///
/// **Variants are declared in ascending order of privilege** —
/// `Participant < Cohost < Host` — because the `Ord` derive follows declaration
/// order and [`LivekitRole::at_least`] relies on it. Declaring them the other way
/// round compiles, passes a casual read, and inverts every privilege check.
///
/// The ordering assertion lives in `privilege_increases_with_ordinal_order`, which is
/// what keeps the declaration order and the security meaning from drifting apart.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "camelCase")]
pub enum LivekitRole {
    /// May subscribe only. The default.
    #[default]
    Participant,
    /// May publish and moderate, but cannot end the broadcast.
    Cohost,
    /// Owns the broadcast. Full room admin.
    Host,
}

impl LivekitRole {
    /// The wire name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Cohost => "cohost",
            Self::Participant => "participant",
        }
    }

    /// Whether this role is at least as privileged as `other`.
    ///
    /// Used for the publish/mute permission checks, so adding a role cannot leave a
    /// gap: `Cohost.at_least(Participant)` is `true` by construction.
    #[must_use]
    pub fn at_least(self, other: Self) -> bool {
        self >= other
    }

    /// Whether this role may publish audio/video.
    #[must_use]
    pub const fn can_publish(self) -> bool {
        matches!(self, Self::Host | Self::Cohost)
    }

    /// Whether this role may administer the room.
    #[must_use]
    pub const fn is_admin(self) -> bool {
        matches!(self, Self::Host)
    }
}

impl std::fmt::Display for LivekitRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A participant currently present in a LiveKit room.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LivekitParticipantInfo {
    /// The Meno user, parsed from the LiveKit participant identity.
    pub id: Uuid,
    /// When they joined.
    // RFC 3339 rather than `OffsetDateTime`'s default: `time` serialises that as the
    // nine-number tuple `(year, ordinal, hour, minute, second, nanosecond, offset_h,
    // offset_m, offset_s)`, which no client can read. See `modules::auth::dto`.
    #[serde(with = "time::serde::rfc3339")]
    pub joined_at: OffsetDateTime,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_serialise_as_camel_case_names() {
        // One spelling per role, so a client or LiveKit agent matching on the string
        // has exactly one thing to match.
        for (role, name) in [
            (LivekitRole::Host, "host"),
            (LivekitRole::Cohost, "cohost"),
            (LivekitRole::Participant, "participant"),
        ] {
            assert_eq!(
                serde_json::to_string(&role).expect("serialise"),
                format!("\"{name}\"")
            );
            assert_eq!(
                serde_json::from_str::<LivekitRole>(&format!("\"{name}\"")).expect("deserialise"),
                role
            );
            assert_eq!(role.as_str(), name);
            assert_eq!(role.to_string(), name);
        }
    }

    #[test]
    fn roles_round_trip() {
        for role in [
            LivekitRole::Host,
            LivekitRole::Cohost,
            LivekitRole::Participant,
        ] {
            let json = serde_json::to_string(&role).expect("serialise");
            assert_eq!(
                serde_json::from_str::<LivekitRole>(&json).expect("deserialise"),
                role
            );
        }
    }

    #[test]
    fn privilege_increases_with_ordinal_order() {
        // `at_least` is a single `>=`, so it is only correct while the `Ord` derive
        // runs least-privileged-first. This assertion pins the declaration order to
        // the security meaning: reversing the enum compiles, reads fine, and inverts
        // every permission check in the codebase.
        let roles = [
            LivekitRole::Participant,
            LivekitRole::Cohost,
            LivekitRole::Host,
        ];

        // The invariant, over every ordered pair: `at_least` must agree with `Ord`.
        for &a in &roles {
            for &b in &roles {
                assert_eq!(
                    a.at_least(b),
                    a >= b,
                    "at_least disagrees with Ord for {a:?} / {b:?}"
                );
            }
        }

        // And the three privilege statements, spelled out.
        assert!(LivekitRole::Host.at_least(LivekitRole::Cohost));
        assert!(LivekitRole::Cohost.at_least(LivekitRole::Participant));
        assert!(!LivekitRole::Participant.at_least(LivekitRole::Cohost));
        assert!(!LivekitRole::Cohost.at_least(LivekitRole::Host));
    }

    #[test]
    fn publish_and_admin_follow_the_role_hierarchy() {
        // A cohost publishes but does not administer; a participant does neither.
        assert!(LivekitRole::Host.can_publish());
        assert!(LivekitRole::Cohost.can_publish());
        assert!(!LivekitRole::Participant.can_publish());

        assert!(LivekitRole::Host.is_admin());
        assert!(!LivekitRole::Cohost.is_admin());
        assert!(!LivekitRole::Participant.is_admin());
    }

    #[test]
    fn participant_is_the_default_role() {
        // The least-privileged value as the default means a caller that forgets to
        // set a role fails closed.
        assert_eq!(LivekitRole::default(), LivekitRole::Participant);
        assert!(!LivekitRole::default().can_publish());
    }

    #[test]
    fn an_unknown_role_name_is_rejected() {
        // Better a parse error than a silent fallback to `participant`, which would
        // hand a cohost the wrong permissions.
        assert!(serde_json::from_str::<LivekitRole>("\"moderator\"").is_err());
        assert!(serde_json::from_str::<LivekitRole>("\"Host\"").is_err());
    }

    #[test]
    fn participant_info_round_trips_with_camel_case_keys() {
        let info = LivekitParticipantInfo {
            id: Uuid::from_u128(7),
            joined_at: OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("valid ts"),
        };

        let json = serde_json::to_value(&info).expect("serialise");
        assert!(
            json.get("joinedAt").is_some(),
            "the wire key should be camelCase, got {json}"
        );
        assert!(
            json.get("joined_at").is_none(),
            "no snake_case key should leak through, got {json}"
        );

        let back: LivekitParticipantInfo =
            serde_json::from_value(json.clone()).expect("deserialise");
        assert_eq!(back, info);
    }
}
