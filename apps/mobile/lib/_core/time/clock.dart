/// A source of the current instant.
///
/// Injected wherever time decides behaviour (token expiry, cooldowns) so tests
/// can move time instead of sleeping. Production uses [SystemClock].
abstract class Clock {
  const new();

  /// The current instant, always in UTC.
  DateTime nowUtc();

  /// Time since this reading's own origin, which never moves backwards.
  ///
  /// Unlike [nowUtc] this is unaffected by the device's clock being changed,
  /// corrected or time-zoned: it is what measures how long a broadcast has
  /// been on air, where a wall-clock jump would otherwise add or remove hours
  /// at once (audit L03). Only differences are meaningful.
  Duration monotonic();
}

/// The real clock.
final class SystemClock implements Clock {
  const new();

  /// One reading for the process, started on first use so no caller sees it
  /// reset.
  static final Stopwatch _uptime = Stopwatch()..start();

  @override
  DateTime nowUtc() => DateTime.now().toUtc();

  @override
  Duration monotonic() => _uptime.elapsed;
}
