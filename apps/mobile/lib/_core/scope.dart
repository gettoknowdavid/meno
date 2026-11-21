import 'package:meno_mobile/_core/value_objects/id.dart';

/// The names of the service locator's scopes.
///
/// A locator concern, not configuration: `Config` describes the environment the
/// app was built for, while these name the two layers `locator.dart` registers
/// into. They are exported from `_core.dart`, so callers keep importing the
/// same barrel.
final class Scope {
  const new _();

  static const String root = 'root';
  static const String authenticated = 'authenticated';

  /// The prefix of one live broadcast's scope.
  static const String livePrefix = 'live';

  /// The scope the broadcast [broadcastId] is live in.
  ///
  /// Named for the broadcast rather than a single `live` layer so a session
  /// that replaces another cannot inherit the first one's registrations.
  static String liveFor(Id broadcastId) =>
      '$livePrefix:${broadcastId.getOrCrash}';
}
