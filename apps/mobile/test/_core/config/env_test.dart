import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';

void main() {
  group('DevEnv', () {
    test('exposes non-empty values', () {
      expect(DevEnv.menoApiUrl, isNotEmpty);
      expect(DevEnv.webSocketUrl, isNotEmpty);
      expect(DevEnv.menoLiveKitUrl, isNotEmpty);
    });
  });

  group('ProdEnv', () {
    test('exposes non-empty values', () {
      expect(ProdEnv.menoApiUrl, isNotEmpty);
      expect(ProdEnv.webSocketUrl, isNotEmpty);
      expect(ProdEnv.menoLiveKitUrl, isNotEmpty);
    });
  });
}
