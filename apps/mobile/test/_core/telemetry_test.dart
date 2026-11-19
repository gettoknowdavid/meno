import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';

import '../_support/stub_api.dart';

/// A sink that keeps what it was handed.
final class _RecordingSink implements TelemetrySink {
  final List<CrashReport> reports = [];
  final List<AnalyticsEvent> events = [];

  bool failNext = false;

  @override
  Future<void> capture(CrashReport report) async {
    if (failNext) {
      failNext = false;
      throw StateError('the transport is down');
    }
    reports.add(report);
  }

  @override
  Future<void> track(AnalyticsEvent event) async {
    if (failNext) {
      failNext = false;
      throw StateError('the transport is down');
    }
    events.add(event);
  }
}

CrashReportingService _service({
  required _RecordingSink sink,
  DateTime Function()? clock,
  String accountId = 'account-1',
}) => CrashReportingService(
  sink: sink,
  context: TelemetryContext(
    env: 'prod',
    platform: 'android',
    appVersion: 'unknown',
    buildNumber: 'unknown',
    accountId: accountId,
  ),
  logger: MockMenoLogger(),
  clock: clock,
);

void main() {
  group('crash reports', () {
    test('carry the context, scope and fatality of the failure', () async {
      final sink = _RecordingSink();
      final service = _service(
        sink: sink,
        clock: () => DateTime.utc(2026, 11, 19, 12),
      )..setAccount('account-2');

      await service.report(
        StateError('boom'),
        StackTrace.current,
        fatal: true,
        scope: 'flutter',
      );

      expect(sink.reports, hasLength(1));
      final report = sink.reports.single;
      expect(report.scope, 'flutter');
      expect(report.fatal, isTrue);
      expect(report.timestamp, DateTime.utc(2026, 11, 19, 12));
      expect(report.context.accountId, 'account-2');

      final json = report.toJson();
      expect(json['type'], 'StateError');
      expect(json['message'], 'Bad state: boom');
      expect((json['context']! as Map)['appVersion'], 'unknown');
    });

    test('redact credentials in the message, the stack and the fields', () {
      final sink = _RecordingSink();
      final report = CrashReport(
        error: ArgumentError(
          'GET https://api.meno.app/v1/me?token=abc123 failed',
        ),
        stackTrace: StackTrace.fromString(
          '#0  Api.get (Bearer eyJhbGciOi.payload.sig)\n'
          '#1  main (package:meno_mobile/main.dart:1:1)',
        ),
        scope: 'command',
        fatal: false,
        timestamp: DateTime.utc(2026),
        context: const TelemetryContext(
          env: 'prod',
          platform: 'ios',
          appVersion: '0.1.0',
          buildNumber: '1',
        ),
      );

      final json = report.toJson();
      final message = json['message']! as String;
      final stack = json['stackTrace']! as String;

      expect(message, isNot(contains('abc123')));
      expect(message, contains('?<redacted>'));
      expect(stack, isNot(contains('eyJhbGciOi')));
      expect(sink.reports, isEmpty);
    });

    test('report the same failure once inside the window', () async {
      var now = DateTime.utc(2026, 11, 19, 12);
      final sink = _RecordingSink();
      final service = _service(sink: sink, clock: () => now);
      final stack = StackTrace.fromString(
        '#0  Home.build (package:meno_mobile/features/home.dart:1:1)',
      );

      await service.report(StateError('one'), stack, fatal: true);
      await service.report(StateError('two'), stack, fatal: true);
      now = now.add(const Duration(minutes: 6));
      await service.report(StateError('three'), stack, fatal: true);

      expect(sink.reports, hasLength(2));
      expect(sink.reports.first.toJson()['message'], 'Bad state: one');
    });

    test('a failing sink is a warning, not a second crash', () async {
      final sink = _RecordingSink()..failNext = true;
      final service = _service(sink: sink);

      await expectLater(
        service.report(StateError('boom'), StackTrace.current, fatal: true),
        completes,
      );
      expect(sink.reports, isEmpty);
    });

    test('reports nothing after disposal', () async {
      final sink = _RecordingSink();
      final service = _service(sink: sink)..dispose();

      await service.report(StateError('late'), StackTrace.current, fatal: true);

      expect(sink.reports, isEmpty);
    });

    test('the handlers follow the instance that installed last', () async {
      final first = _RecordingSink();
      final second = _RecordingSink();
      final before = FlutterError.onError;
      addTearDown(() => FlutterError.onError = before);

      // The installed handler presents through whatever was there before it;
      // in a test that is the framework's reporter, which would report the
      // deliberately-thrown error as a test failure.
      FlutterError.onError = (details) {};

      _service(sink: first).install();
      _service(sink: second).install();

      final details = FlutterErrorDetails(
        exception: StateError('from the framework'),
        stack: StackTrace.current,
        library: 'test',
      );
      FlutterError.onError!(details);
      await pumpEventQueue();

      expect(first.reports, isEmpty);
      expect(second.reports, hasLength(1));
      expect(second.reports.single.scope, 'flutter');
      expect(second.reports.single.fatal, isTrue);
    });
  });

  group('analytics events', () {
    test('share the crash context and keep their properties', () async {
      final sink = _RecordingSink();
      final service = _service(
        sink: sink,
        clock: () => DateTime.utc(2026, 11, 19, 13),
      );
      final analytics = AnalyticsService(
        sink: sink,
        reporting: service,
        logger: MockMenoLogger(),
        clock: () => DateTime.utc(2026, 11, 19, 13),
      );

      await analytics.track('broadcast_started', {'viewers': 3});

      final event = sink.events.single;
      expect(event.name, 'broadcast_started');
      expect(event.toJson()['properties'], {'viewers': '3'});
      expect(event.context.env, 'prod');
      expect(
        event.toJson()['timestamp'],
        DateTime.utc(2026, 11, 19, 13).toIso8601String(),
      );
    });

    test('a failing sink does not surface to the caller', () async {
      final sink = _RecordingSink()..failNext = true;
      final analytics = AnalyticsService(
        sink: sink,
        reporting: _service(sink: _RecordingSink()),
        logger: MockMenoLogger(),
      );

      await expectLater(analytics.track('app_start'), completes);
      expect(sink.events, isEmpty);
    });
  });
}
