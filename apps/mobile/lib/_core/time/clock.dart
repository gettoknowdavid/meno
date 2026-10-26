/// A source of the current instant.
///
/// Injected wherever time decides behaviour (token expiry, cooldowns) so tests
/// can move time instead of sleeping. Production uses [SystemClock].
abstract class Clock {
  const new();

  /// The current instant, always in UTC.
  DateTime nowUtc();
}

/// The real clock.
final class SystemClock implements Clock {
  const new();

  @override
  DateTime nowUtc() => DateTime.now().toUtc();
}
