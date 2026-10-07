import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/locator.dart';
import 'package:mocktail/mocktail.dart';

class MockMenoLogger extends Mock implements MenoLogger;

void main() {
  setUp(di.reset);
  tearDown(di.reset);

  group('configureDependencies', () {
    test('registers config, logger and secure storage in the root scope', () {
      const config = Config(
        env: Environment.dev,
        menoApiUrl: 'https://dev.api',
        webSocketUrl: 'wss://dev.socket',
        menoLiveKitUrl: 'https://dev.livekit',
      );

      configureDependencies(config);

      expect(di.currentScopeName, Scope.root);
      expect(di<Config>(), same(config));
      expect(di<MenoLogger>(), isA<MenoLoggerImpl>());
      expect(di.isRegistered<FlutterSecureStorage>(), isTrue);
    });

    test('wires the logger policy to the config environment', () {
      const prodConfig = Config(
        env: Environment.prod,
        menoApiUrl: 'https://api',
        webSocketUrl: 'wss://socket',
        menoLiveKitUrl: 'https://livekit',
      );

      configureDependencies(prodConfig);

      final logger = di<MenoLogger>() as MenoLoggerImpl;
      expect(logger.env, Environment.prod);
    });
  });

  group('DI with a mocked logger', () {
    test('resolves the mock and verifies startup interactions', () {
      final logger = MockMenoLogger();
      di.registerSingleton<MenoLogger>(logger);

      di<MenoLogger>()
        ..i('App starting', tag: MenoLogTag.lifecycle.label)
        ..event('app_start', {'env': 'dev'});

      verify(() => logger.i('App starting', tag: MenoLogTag.lifecycle.label))
          .called(1);
      verify(() => logger.event('app_start', {'env': 'dev'})).called(1);
      verifyNoMoreInteractions(logger);
    });
  });
}
