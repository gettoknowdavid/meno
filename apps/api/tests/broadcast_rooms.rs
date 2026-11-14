//! §10 **Contract** layer: the room registry's reverse lookup (gap 1).
//!
//! # Why this suite exists
//!
//! `handle_socket` unsubscribes a user from every room they were in when their last
//! socket closes, and it asks the registry which rooms those are. An empty answer there
//! is indistinguishable from "this user was in no rooms", so the socket would look
//! cleaned up while the replica kept a subscription and a membership SET alive — the
//! leak `docs/backend-gaps.md`'s gap 9 warns about, in realtime form.
//!
//! # What this cannot prove
//!
//! That Redis receives the `SREM`: the shared SET is written by
//! [`WsPubSubBridge`](meno_api::infrastructure::ws::pubsub::WsPubSubBridge), which needs
//! a live Redis and is exercised by the socket-level test described in the gap audit.
//! This is the half that can be asserted without one.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use meno_api::infrastructure::ws::Registry;
use uuid::Uuid;

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

#[test]
fn rooms_of_lists_only_the_rooms_the_user_is_in() {
    let registry = Registry::new();

    // Two rooms, one member each.
    let _ = registry.add_local_room_member(id(10), id(1));
    let _ = registry.add_local_room_member(id(11), id(2));

    assert_eq!(registry.rooms_of(id(1)), vec![id(10)]);
    assert_eq!(registry.rooms_of(id(2)), vec![id(11)]);
    assert!(
        registry.rooms_of(id(3)).is_empty(),
        "a user with no room must have no memberships"
    );

    // A second room for the same user is reported too: delivery is per room, so cleanup
    // must be per room as well.
    let _ = registry.add_local_room_member(id(11), id(1));
    let mut rooms = registry.rooms_of(id(1));
    rooms.sort();
    assert_eq!(rooms, vec![id(10), id(11)]);
}

#[test]
fn leaving_a_room_takes_it_out_of_the_users_list() {
    let registry = Registry::new();

    let _ = registry.add_local_room_member(id(10), id(1));
    let _ = registry.add_local_room_member(id(11), id(1));

    // The bool is the *unsubscribe* decision for that one room: true when the removal
    // emptied it. Room 10 held only this user, so it goes; room 11 still holds them.
    let became_empty = registry.remove_local_room_member(id(10), id(1));
    assert!(
        became_empty,
        "removing a room's only member is what makes it empty"
    );

    assert_eq!(registry.rooms_of(id(1)), vec![id(11)]);
}
