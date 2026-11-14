import 'package:flutter/foundation.dart';
import 'package:meno_mobile/_core/value_objects/id.dart';
import 'package:meno_mobile/_routing/routes.dart';

/// A destination a notification or deep link asked the app to open.
///
/// Held in memory only: a persisted copy would need a retention decision the
/// audit does not make. [recipientAccountId] is attribution — which account
/// the backend addressed the tap to — never proof of access; the router
/// decides whether it may be executed.
@immutable
final class PendingDestination {
  const new({required this.path, this.recipientAccountId});

  /// The internal location, already validated by [PendingDestinationSlot].
  final String path;

  /// The account the tap was addressed to, when the payload carried one.
  final Id? recipientAccountId;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is PendingDestination &&
          runtimeType == other.runtimeType &&
          path == other.path &&
          recipientAccountId == other.recipientAccountId;

  @override
  int get hashCode => Object.hash(path, recipientAccountId);

  @override
  String toString() =>
      'PendingDestination($path, recipient: $recipientAccountId)';
}

/// The one in-memory slot a pending destination lives in.
///
/// Newest wins: a second tap before the first was consumed replaces it, which
/// is what a person expects from two notifications in a row. The allowlist is
/// the security boundary — an external URL, an auth route or a malformed path
/// is refused here so no platform payload can steer the navigator.
final class PendingDestinationSlot {
  PendingDestination? _held;

  /// The destination being held, without consuming it.
  PendingDestination? peek() => _held;

  /// Returns the held destination and clears the slot.
  PendingDestination? take() {
    final held = _held;
    _held = null;
    return held;
  }

  /// Holds [destination] for the router, or refuses it.
  ///
  /// Returns whether it was held, so a caller can tell a rejected payload from
  /// a consumed one.
  bool set(PendingDestination destination) {
    if (!_isAllowedInternalPath(destination.path)) return false;
    _held = destination;
    return true;
  }

  /// Whether [path] names an internal location this app actually routes.
  ///
  /// Only internal, public-facing app paths: no scheme (`https://`), no auth
  /// route (a tap must never re-enter sign-in or reset flows) and no path that
  /// is not rooted at a known first segment.
  static bool _isAllowedInternalPath(String path) {
    if (!path.startsWith('/')) return false;
    if (path.contains('://') || path.contains('@')) return false;

    final firstSegment = path
        .split('/')
        .where((part) => part.isNotEmpty)
        .firstOrNull;
    if (firstSegment == null) return false;

    const blocked = {
      MRoutes.login,
      MRoutes.register,
      MRoutes.emailVerification,
      MRoutes.forgotPassword,
      MRoutes.resetPassword,
      MRoutes.authGoogle,
      MRoutes.onboarding,
    };
    final route = '/$firstSegment';
    if (blocked.contains(route)) return false;

    const allowed = {
      MRoutes.home,
      MRoutes.discover,
      MRoutes.notes,
      MRoutes.myProfile,
      MRoutes.broadcasts,
      MRoutes.settings,
      MRoutes.notifications,
      MRoutes.bible,
      MRoutes.profiles,
      MRoutes.nowLive,
      MRoutes.recentlyLive,
      MRoutes.liveSessionInit,
    };
    return allowed.contains(route);
  }
}
