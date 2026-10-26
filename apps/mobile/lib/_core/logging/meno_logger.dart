import 'package:logger/logger.dart';
import 'package:meno_mobile/_core/_core.dart';

/// Tag constants for common subsystems so log lines stay grep-friendly.
enum MenoLogTag {
  config('Config'),
  network('Network'),
  auth('Auth'),
  chat('Chat'),
  push('Push'),
  permissions('Permissions'),
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

  /// Whether trace and debug lines are written.
  ///
  /// Warnings, errors, fatals, info and structured events are written in every
  /// environment: a production lifecycle or storage failure must leave a
  /// trace (M23). What dev-only verbosity buys is the noise, not the signal.
  bool get _verbose => env.isDev;

  @override
  void v(String message, {String? tag, Object? error, StackTrace? stackTrace}) {
    if (!_verbose) return;
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
    if (!_verbose) return;
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
    _write(Level.info, message, tag: tag, error: error, stackTrace: stackTrace);
  }

  @override
  void w(String message, {String? tag, Object? error, StackTrace? stackTrace}) {
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
    // Everything written goes through the scrubber: message and error alike,
    // at every level, so a token embedded in an exception string never lands
    // in production output (M23).
    final safeMessage = _scrub(prefix);
    final safeError = error == null ? null : _scrub(error.toString());
    if (safeError != null) {
      _backend.log(
        level,
        safeMessage,
        error: safeError,
        stackTrace: stackTrace,
      );
    } else {
      _backend.log(level, safeMessage, stackTrace: stackTrace);
    }
  }

  /// Masks credential-shaped values in [text] before it is written.
  ///
  /// Three shapes cover where secrets actually appear: a bearer header, a
  /// `key=value` style field of a structured message, and a URL's query
  /// string (which may carry a token parameter). Exception messages and event
  /// values are plain strings, so this is the one place that sees them.
  String _scrub(String text) => text
      .replaceAll(_bearerPattern, r'$1<redacted>')
      .replaceAllMapped(_assignmentPattern, (match) => '${match[1]}=<redacted>')
      .replaceAll(_urlQueryPattern, '?<redacted>');

  static final RegExp _bearerPattern = RegExp(
    r'(Bearer\s+)\S+',
    caseSensitive: false,
  );

  static final RegExp _assignmentPattern = RegExp(
    r'\b(password|passwd|refresh_token|access_token|refreshToken|accessToken|'
    r'token|secret|api[_-]?key|code|otp)\b\s*[=:]\s*[^\s,;]+',
    caseSensitive: false,
  );

  static final RegExp _urlQueryPattern = RegExp(r'''\?[^\s'"]+''');

  String _sanitize(Object value) {
    final string = _scrub(value.toString());
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
