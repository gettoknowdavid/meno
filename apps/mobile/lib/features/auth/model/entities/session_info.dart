/// One row of the account's device list.
///
/// The shape `GET /auth/sessions` returns (`SessionResponse` in
/// `apps/api/src/modules/auth/dto.rs`). The id is what
/// `POST /auth/sessions/{id}` revokes, and the label is the part a person
/// recognises when deciding which device to sign out.
final class SessionInfo {
  /// Creates a device list entry.
  const new({
    required this.id,
    required this.deviceLabel,
    required this.createdAt,
    required this.lastUsedAt,
    required this.rotations,
  });

  /// The session id.
  final String id;

  /// Something a person recognises, e.g. "Ada's iPhone".
  final String deviceLabel;

  /// When the session was created.
  final DateTime? createdAt;

  /// When it was last used to refresh.
  final DateTime? lastUsedAt;

  /// How many times this device's refresh chain has been rotated.
  final int rotations;

  @override
  bool operator ==(Object other) =>
      other is SessionInfo &&
      other.id == id &&
      other.deviceLabel == deviceLabel &&
      other.lastUsedAt == lastUsedAt &&
      other.rotations == rotations;

  @override
  int get hashCode => Object.hash(id, deviceLabel, lastUsedAt, rotations);
}
