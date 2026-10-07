import 'package:logger/logger.dart';
import 'package:meno_mobile/_core/_core.dart';

/// Tag constants for common subsystems so log lines stay grep-friendly.
enum MenoLogTag {
  config('Config'),
  network('Network'),
  auth('Auth'),
  chat('Chat'),
  push('Push'),
  di('DI'),
  lifecycle('Lifecycle'),
  debug('Debug');

  new(this.label);

  final String label;
}

/// App-wide logging abstraction.
///
/// Production code should depend on this interface, not on [Logger]
/// directly, so tests and alternate outputs can be swapped in.
abstract class MenoLogger {
  void v(String message, {String? tag, Object? error, StackTrace? stackTrace});
  void d(String message, {String? tag, Object? error, StackTrace? stackTrace});
  void i(String message, {String? tag, Object? error, StackTrace? stackTrace});
  void w(String message, {String? tag, Object? error, StackTrace? stackTrace});
  void e(String message, {String? tag, Object? error, StackTrace? stackTrace});
  void fatal(
    String message, {
    String? tag,
    Object? error,
    StackTrace? stackTrace,
  });

  /// Minimal structured helper for operational events people grep for.
  void event(String name, Map<String, Object> context);
}

/// Concrete logger backed by [`logger`] with env-aware behavior.
///
/// Policy is driven by the active environment decided once at startup:
/// - In dev, verbose/debug output is allowed.
/// - In prod, debug/trace are disabled, but warnings/errors and structured
///   events are still recorded.
///
/// This is not a substitute for crash reporting or PII policy, but it gives
/// a consistent, non-throwing logging surface across the app.
final class MenoLoggerImpl implements MenoLogger {
  new(this.env)
    : _backend = Logger(
        printer: env.isProd
            ? SimplePrinter()
            : PrettyPrinter(
                dateTimeFormat: DateTimeFormat.onlyTimeAndSinceStart,
                errorMethodCount: 2,
                stackTraceBeginIndex: 1,
              ),
        level: env.isProd ? Level.warning : Level.debug,
      );

  final Environment env;

  late final Logger _backend;

  bool get _on => env.isDev;

  @override
  void v(String message, {String? tag, Object? error, StackTrace? stackTrace}) {
    if (!_on) return;
    _write(
      Level.trace,
      message,
      tag: tag,
      error: error,
      stackTrace: stackTrace,
    );
  }

  @override
  void d(String message, {String? tag, Object? error, StackTrace? stackTrace}) {
    if (!_on) return;
    _write(
      Level.debug,
      message,
      tag: tag,
      error: error,
      stackTrace: stackTrace,
    );
  }

  @override
  void i(String message, {String? tag, Object? error, StackTrace? stackTrace}) {
    if (!_on) return;
    _write(Level.info, message, tag: tag, error: error, stackTrace: stackTrace);
  }

  @override
  void w(String message, {String? tag, Object? error, StackTrace? stackTrace}) {
    if (!_on) return;
    _write(
      Level.warning,
      message,
      tag: tag,
      error: error,
      stackTrace: stackTrace,
    );
  }

  @override
  void e(String message, {String? tag, Object? error, StackTrace? stackTrace}) {
    if (!_on) return;
    _write(
      Level.error,
      message,
      tag: tag,
      error: error,
      stackTrace: stackTrace,
    );
  }

  @override
  void fatal(
    String message, {
    String? tag,
    Object? error,
    StackTrace? stackTrace,
  }) {
    if (!_on) return;
    _write(
      Level.fatal,
      message,
      tag: tag,
      error: error,
      stackTrace: stackTrace,
    );
  }

  @override
  void event(String name, Map<String, Object> context) {
    if (!_on) return;
    final parts = <String>[name];
    for (final entry in context.entries) {
      parts.add('${entry.key}=${_sanitize(entry.value)}');
    }
    i(parts.join(' | '));
  }

  void _write(
    Level level,
    String message, {
    String? tag,
    Object? error,
    StackTrace? stackTrace,
  }) {
    final prefix = tag == null ? message : '[$tag] $message';
    if (error != null) {
      _backend.log(level, prefix, error: error, stackTrace: stackTrace);
    } else {
      _backend.log(level, prefix, stackTrace: stackTrace);
    }
  }

  String _sanitize(Object value) {
    final string = value.toString();
    if (_looksSecret(string)) {
      return '<redacted>';
    }
    return string;
  }

  bool _looksSecret(String value) {
    final lower = value.toLowerCase();
    return lower.contains('key') ||
        lower.contains('token') ||
        lower.contains('secret') ||
        lower.contains('password') ||
        lower.contains('authorization') ||
        lower.contains('bearer');
  }
}
