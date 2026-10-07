import 'package:flutter_test/flutter_test.dart';
import 'package:logger/logger.dart';
import 'package:meno_mobile/_core/_core.dart';

/// Captures both stages of the logging pipeline via the global listeners
/// exposed by `package:logger`:
///
/// - [Logger.addLogListener] fires for every event that reaches the
///   backend, *before* the level filter runs. It shows what
///   [MenoLoggerImpl] chose to log.
/// - [Logger.addOutputListener] fires only when the level filter passes
///   and the printer produced lines, i.e. what actually reaches the
///   console.
class _LogCapture {
  final List<LogEvent> events = <LogEvent>[];
  final List<OutputEvent> outputs = <OutputEvent>[];

  void attach() {
    Logger.addLogListener(_onEvent);
    Logger.addOutputListener(_onOutput);
  }

  void detach() {
    Logger.removeLogListener(_onEvent);
    Logger.removeOutputListener(_onOutput);
  }

  void _onEvent(LogEvent event) => events.add(event);

  void _onOutput(OutputEvent event) => outputs.add(event);
}

/// Discards console writes so test output stays clean. The
/// [Logger.addOutputListener] callbacks still fire, so assertions on
/// captured output are unaffected.
class _NullOutput extends LogOutput {
  @override
  void output(OutputEvent event) {}
}

void main() {
  late _LogCapture capture;
  late LogOutput Function() originalOutput;

  setUp(() {
    originalOutput = Logger.defaultOutput;
    Logger.defaultOutput = _NullOutput.new;
    capture = _LogCapture()..attach();
  });

  tearDown(() {
    capture.detach();
    Logger.defaultOutput = originalOutput;
  });

  group('MenoLogTag', () {
    test('labels match their subsystems', () {
      expect(MenoLogTag.config.label, 'Config');
      expect(MenoLogTag.network.label, 'Network');
      expect(MenoLogTag.auth.label, 'Auth');
      expect(MenoLogTag.chat.label, 'Chat');
      expect(MenoLogTag.push.label, 'Push');
      expect(MenoLogTag.di.label, 'DI');
      expect(MenoLogTag.lifecycle.label, 'Lifecycle');
      expect(MenoLogTag.debug.label, 'Debug');
    });
  });

  group('MenoLoggerImpl in dev', () {
    late MenoLoggerImpl logger;

    setUp(() {
      logger = MenoLoggerImpl(Environment.dev);
    });

    test('forwards every level to the backend', () {
      logger
        ..v('trace message')
        ..d('debug message')
        ..i('info message')
        ..w('warning message')
        ..e('error message')
        ..fatal('fatal message');

      expect(capture.events.map((e) => e.level), <Level>[
        Level.trace,
        Level.debug,
        Level.info,
        Level.warning,
        Level.error,
        Level.fatal,
      ]);
    });

    test('prefixes messages with the tag when provided', () {
      logger
        ..i('connected', tag: MenoLogTag.network.label)
        ..i('plain');

      expect(capture.events[0].message, '[Network] connected');
      expect(capture.events[1].message, 'plain');
    });

    test('passes error and stack trace through', () {
      final error = Exception('boom');
      final stackTrace = StackTrace.current;

      logger.e('failed', error: error, stackTrace: stackTrace);

      final event = capture.events.single;
      expect(event.error, same(error));
      expect(event.stackTrace, same(stackTrace));
    });

    test('formats structured events as name and key-value pairs', () {
      logger.event('user_signed_in', {'method': 'google', 'env': 'dev'});

      expect(
        capture.events.single.message,
        'user_signed_in | method=google | env=dev',
      );
    });

    test('redacts values that look like secrets', () {
      logger
        ..event('auth', {'header': 'Bearer abc', 'method': 'google'})
        ..event('cache', {'value': 'user token xyz'});

      expect(
        capture.events[0].message,
        'auth | header=<redacted> | method=google',
      );
      expect(capture.events[1].message, 'cache | value=<redacted>');
    });

    test('prints debug and above, but not trace', () {
      logger
        ..v('trace only')
        ..d('debug visible');

      expect(capture.outputs, hasLength(1));
      expect(
        capture.outputs.single.lines.join('\n'),
        contains('debug visible'),
      );
    });
  });

  group('MenoLoggerImpl in prod', () {
    late MenoLoggerImpl logger;

    setUp(() {
      logger = MenoLoggerImpl(Environment.prod);
    });

    test('stays completely silent', () {
      logger
        ..v('trace')
        ..d('debug')
        ..i('info')
        ..w('warning')
        ..e('error')
        ..fatal('fatal')
        ..event('app_start', {'env': 'prod'});

      expect(capture.events, isEmpty);
      expect(capture.outputs, isEmpty);
    });
  });
}
