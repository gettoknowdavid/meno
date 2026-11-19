import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:meno_mobile/_core/logging/meno_logger.dart';
import 'package:meno_mobile/_core/telemetry/telemetry_models.dart';
import 'package:meno_mobile/_core/telemetry/telemetry_sink.dart';
import 'package:package_info_plus/package_info_plus.dart';

/// Catches the failures nobody else caught, and hands them to a 
/// [TelemetrySink].
///
/// # What it captures
///
/// * **Framework errors** — [FlutterError.onError]: a build, layout or paint
///   failure, and anything a `WidgetsBinding` reports.
/// * **Platform errors** — [PlatformDispatcher.onError]: an error that escaped
///   the Flutter framework, including an uncaught asynchronous one.
/// * **Command failures** — through
///   `configureGlobalExceptionHandler`, which already routes them; only the
///   unclassified ones are reported, because a network refusal is a failure
///   the app expects and handles, not a crash.
///
/// # What it must never do
///
/// Throwing from here would turn one crash into two. Every step is guarded:
/// a sink that fails is a logged warning, a clock that misbehaves still
/// produces a report, and [report] is safe to call after disposal.
///
/// # De-duplication
///
/// The same frame hit in a loop is one bug. Reports matching a fingerprint
/// ([CrashReport.fingerprint]) already accepted in [_dedupeWindow] are dropped,
/// and a run stops forwarding after [_maxReportsPerRun] distinct failures —
/// the purpose is diagnosis, not a denial-of-service on the device's battery.
final class CrashReportingService {
  new({
    required this._sink,
    required this._context,
    required this._logger,
    DateTime Function()? clock,
    this._installFrameworkHandlers = true,
  }) : _clock = clock ?? DateTime.now;

  final TelemetrySink _sink;
  final MenoLogger _logger;
  final DateTime Function() _clock;

  TelemetryContext _context;

  /// The facts a report from this run carries.
  TelemetryContext get context => _context;

  /// Whether this instance has been installed. [install] is idempotent.
  bool get isInstalled => _installed;
  bool _installed = false;

  bool _disposed = false;

  /// Whether [install] takes the framework's two global error hooks.
  ///
  /// True for the app's own pipeline. A receiving SDK that installs its own
  /// handlers (Sentry's `SentryFlutter.init`, for instance) is the one case
  /// where this is false: two handlers for one failure means two events, and
  /// the same crash counted twice is worse than a report from only one path.
  /// Command failures still reach the sink either way.
  final bool _installFrameworkHandlers;

  /// The instance the isolate's handlers currently forward to.
  ///
  /// The handlers are global and installed once; the service is not — a
  /// bootstrap retry resets the container and builds a new one. Routing through
  /// this field is what makes the handlers follow the live instance instead of
  /// chaining a new handler per retry, which would report every failure twice.
  static CrashReportingService? _active;
  static bool _handlersInstalled = false;

  /// Fingerprint → when it was last accepted.
  final Map<String, DateTime> _recent = {};

  /// How many distinct failures this run has forwarded.
  int _forwarded = 0;

  /// How long the same failure is reported once.
  static const Duration _dedupeWindow = Duration(minutes: 5);

  /// How many distinct failures one run forwards before it stops.
  static const int _maxReportsPerRun = 50;

  /// Points the framework's two global error hooks at this service.
  ///
  /// Called once from `configureGlobalExceptionHandler`, after the locator is
  /// configured and before the first frame. The framework's own presenter is
  /// still called, so a debug console and the platform log keep working.
  void install() {
    if (_disposed) return;
    _installed = true;
    _active = this;

    if (!_installFrameworkHandlers) return;
    if (_handlersInstalled) return;
    _handlersInstalled = true;

    final previous = FlutterError.onError;
    FlutterError.onError = (details) {
      // The console presentation first: it is what a developer sees, and it
      // must not depend on telemetry succeeding.
      if (previous != null) {
        previous(details);
      } else {
        FlutterError.presentError(details);
      }

      unawaited(
        _active?.report(
          details.exception,
          details.stack,
          fatal: true,
          scope: 'flutter',
        ),
      );
    };

    PlatformDispatcher.instance.onError = (error, stackTrace) {
      unawaited(
        _active?.report(error, stackTrace, fatal: true, scope: 'platform'),
      );
      // Handled: the engine must not also take the process down for a failure
      // that has been recorded. The report is the record.
      return true;
    };
  }

  /// Fills in the packaged version and build number.
  ///
  /// Asynchronous and failure-tolerant: the plugin answers after the first
  /// frame, and a build that cannot read its own package metadata is still a
  /// build whose crashes matter. Until it answers, reports say `unknown`.
  Future<void> loadAppInfo() async {
    try {
      final info = await PackageInfo.fromPlatform();
      _context = _context.withAppInfo(
        version: info.version,
        build: info.buildNumber,
      );
    } on Object catch (error) {
      _logger.w(
        'Could not read the packaged version for telemetry',
        tag: MenoLogTag.lifecycle.label,
        error: error,
      );
    }
  }

  /// Names the signed-in account, or forgets it.
  ///
  /// An account identifier is not a secret, but it is only correct while that
  /// account is the active one. Whoever ends a session should call this with
  /// `null`, or the next crash is attributed to the previous person.
  void setAccount(String? accountId) {
    _context = _context.withAccount(accountId);
  }

  /// Reports one failure. Never throws, never blocks the caller.
  Future<void> report(
    Object error,
    StackTrace? stackTrace, {
    required bool fatal,
    String scope = 'app',
  }) async {
    if (_disposed) return;
    if (_forwarded >= _maxReportsPerRun) return;

    final report = CrashReport(
      error: error,
      stackTrace: stackTrace,
      scope: scope,
      fatal: fatal,
      timestamp: _clock(),
      context: _context,
    );

    if (!_accept(report.fingerprint, report.timestamp)) return;
    _forwarded++;

    try {
      await _sink.capture(report);
    } on Object catch (failure, failureStack) {
      _logger.w(
        'The telemetry sink could not take a crash report',
        tag: MenoLogTag.debug.label,
        error: failure,
        stackTrace: failureStack,
      );
    }
  }

  /// Whether [fingerprint] is new enough to forward.
  bool _accept(String fingerprint, DateTime now) {
    // Bounded: a long session with many distinct bugs must not grow a map
    // without limit. Clearing is cheaper than tracking insertion order, and the
    // cost of a clear is one repeated report per fingerprint.
    if (_recent.length > 200) _recent.clear();

    final seen = _recent[fingerprint];
    if (seen != null && now.difference(seen) < _dedupeWindow) return false;

    _recent[fingerprint] = now;
    return true;
  }

  /// Stops forwarding. Handlers stay installed — restoring them would turn a
  /// teardown into a new source of failures — but every later report is 
  /// a no-op.
  void dispose() {
    _disposed = true;
    _recent.clear();
    if (identical(_active, this)) _active = null;
  }
}
