import 'package:meno_mobile/_core/logging/meno_logger.dart';
import 'package:meno_mobile/_core/telemetry/crash_reporting_service.dart';
import 'package:meno_mobile/_core/telemetry/telemetry_models.dart';
import 'package:meno_mobile/_core/telemetry/telemetry_sink.dart';

/// The app's product events.
///
/// # What belongs here
///
/// Things the team would ask a question about: a launch, a sign-in, a
/// broadcast started, a download finished. Not every tap, and never content —
/// no titles, no messages, no search terms, no email addresses. An event's
/// properties are counters, enums and durations.
///
/// # Why it is this small
///
/// A vendor SDK would add its own global API, its own buffering and its own
/// consent surface. The app already has one seam for that ([TelemetrySink]),
/// one context ([TelemetryContext]) and one place where a failure is written
/// ([CrashReportingService]); analytics is a name and a map over the same
/// machinery, which keeps version, environment and account attribution
/// consistent between an event and a crash from the same session.
///
/// # Failure policy
///
/// [track] never throws and never awaits the caller. An analytics failure is a
/// warning in the log; losing an event must not lose the action it described.
final class AnalyticsService {
  new({
    required this._sink,
    required this._reporting,
    required this._logger,
    DateTime Function()? clock,
  }) : _clock = clock ?? DateTime.now;

  final TelemetrySink _sink;
  final CrashReportingService _reporting;
  final MenoLogger _logger;
  final DateTime Function() _clock;

  /// Records one event.
  ///
  /// [properties] are copied into the event as strings, so a caller cannot
  /// attach an object graph that a sink would have to understand.
  Future<void> track(
    String name, [
    Map<String, Object?> properties = const {},
  ]) async {
    try {
      await _sink.track(
        AnalyticsEvent(
          name: name,
          properties: Map.unmodifiable(properties),
          timestamp: _clock(),
          context: _reporting.context,
        ),
      );
    } on Object catch (error, stackTrace) {
      _logger.w(
        'The telemetry sink could not take the "$name" event',
        tag: MenoLogTag.debug.label,
        error: error,
        stackTrace: stackTrace,
      );
    }
  }
}
