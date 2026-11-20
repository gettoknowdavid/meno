/// The five reactions a room offers.
///
/// [wire] is the token the API accepts and broadcasts as the reaction's
/// `content` (`REACTION_TOKENS` in `apps/api/src/infrastructure/ws/dto.rs`). A
/// stable token rather than an emoji: both clients draw the same picture from
/// the design system's asset pack, the server can validate the set, and the
/// picture never depends on a font. Nothing is stored server-side — the
/// reaction is the frame.
enum ReactionType {
  /// Applause.
  clap('clap'),

  /// A flame.
  flame('flame'),

  /// A red heart.
  like('like'),

  /// Raised hands.
  raise('raise'),

  /// A finger snap.
  snap('snap');

  new(this.wire);

  /// The token carried on the wire, in both directions.
  final String wire;

  /// The reaction [wire] names, or null when this build cannot draw it.
  ///
  /// An unknown label is not a failure: the API admits only the five tokens,
  /// but a server newer than this build may add a sixth, and a picture with no
  /// asset here is a dropped sprite rather than an exception. The caller drops
  /// it and the room carries on.
  static ReactionType? fromWire(String? wire) {
    final clean = wire?.trim().toLowerCase();
    if (clean == null || clean.isEmpty) return null;

    return switch (clean) {
      'clap' || '👏' => ReactionType.clap,
      'flame' || '🔥' => ReactionType.flame,
      'like' || '❤' || '❤️' => ReactionType.like,
      'raise' || '🙌' => ReactionType.raise,
      'snap' || '🤌' => ReactionType.snap,
      _ => null,
    };
  }
}
