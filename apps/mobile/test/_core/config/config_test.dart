import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';

void main() {
  group('Config', () {
    test('keeps the injected environment and urls', () {
      const config = Config(
        env: Environment.dev,
        menoApiUrl: 'https://dev.api',
        webSocketUrl: 'wss://dev.socket',
        menoLiveKitUrl: 'https://dev.livekit',
      );

      expect(config.env, Environment.dev);
      expect(config.menoApiUrl, 'https://dev.api');
      expect(config.webSocketUrl, 'wss://dev.socket');
      expect(config.menoLiveKitUrl, 'https://dev.livekit');
    });

    test('a development build ignores the hosted room url', () {
      const config = Config(
        env: Environment.dev,
        menoApiUrl: 'https://dev.api',
        webSocketUrl: 'wss://dev.socket',
        menoLiveKitUrl: 'wss://meno.livekit.cloud',
      );

      expect(config.liveKitUrl, Config.localLiveKitUrl);
      expect(config.usesLocalRoomServer, isTrue);
    });

    test('a release build uses the room url it was given', () {
      const config = Config(
        env: Environment.prod,
        menoApiUrl: 'https://api',
        webSocketUrl: 'wss://socket',
        menoLiveKitUrl: 'wss://meno.livekit.cloud',
      );

      expect(config.liveKitUrl, 'wss://meno.livekit.cloud');
      expect(config.usesLocalRoomServer, isFalse);
    });
  });

  group('Environment', () {
    test('isProd is true only for prod', () {
      expect(Environment.prod.isProd, isTrue);
      expect(Environment.dev.isProd, isFalse);
    });

    test('isDev is true only for dev', () {
      expect(Environment.dev.isDev, isTrue);
      expect(Environment.prod.isDev, isFalse);
    });
  });

  group('Scope', () {
    test('exposes stable scope names', () {
      expect(Scope.root, 'root');
      expect(Scope.authenticated, 'authenticated');
    });
  });
}
