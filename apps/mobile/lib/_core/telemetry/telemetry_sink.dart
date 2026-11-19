import 'package:meno_mobile/_core/logging/meno_logger.dart';
import 'package:meno_mobile/_core/telemetry/telemetry_models.dart';

/// Where reports and events go.
///
/// # Why an interface and not a vendor SDK
///
/// The pipeline — what is captured, how it is redacted, how it is de-duplicated
/// — is the app's and is testable without a network. Which service receives it
/// is a deployment decision that needs an account and a key, so it is one
/// implementation of this interface, not a shape woven through the callers.
/// `docs/crash-reporting-and-analytics.md` has the production steps, including
/// the two ways to supply one.
///
/// # The contract
///
/// A sink is called once per accepted report or event, in order, and it may
/// throw: the services around it treat a sink failure as a logged warning, not
/// as a second crash. It must not block — the caller does not await it before
/// continuing.
abstract interface class TelemetrySink {
  /// Takes one crash report.
  Future<void> capture(CrashReport report);

  /// Takes one product event.
  Future<void> track(AnalyticsEvent event);
}

/// The default sink: the app log.
///
/// It is what ships until a receiving service is configured. A crash is written
/// at `fatal`/`error` with its stack, so it reaches the platform's own log and
/// the debugging console; a product event is written as one structured line.
/// Nothing leaves the device.
///
/// The one thing this sink cannot do is survive the process: on a device with
/// no attached console, a release crash is in logcat/Console only, and only for
/// as long as the OS keeps it. That is exactly what the production sink in the
/// doc is for.
final class LogTelemetrySink implements TelemetrySink {
  const new({required this._logger});

  final MenoLogger _logger;

  @override
  Future<void> capture(CrashReport report) async {
    final message =
        'Crash [${report.scope}] ${report.error.runtimeType}'
        ' (${report.context.appVersion}+${report.context.buildNumber})';

    if (report.fatal) {
      _logger.fatal(
        message,
        tag: MenoLogTag.debug.label,
        error: report.error,
        stackTrace: report.stackTrace,
      );
      return;
    }

    _logger.e(
      message,
      tag: MenoLogTag.debug.label,
      error: report.error,
      stackTrace: report.stackTrace,
    );
  }

  @override
  Future<void> track(AnalyticsEvent event) async {
    _logger.event(event.name, {
      for (final entry in event.properties.entries)
        entry.key: entry.value ?? '',
      'env': event.context.env,
      'appVersion': event.context.appVersion,
    });
  }
}
