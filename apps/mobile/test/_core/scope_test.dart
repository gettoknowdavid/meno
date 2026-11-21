import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';

void main() {
  group('Scope', () {
    test('exposes stable scope names', () {
      expect(Scope.root, 'root');
      expect(Scope.authenticated, 'authenticated');
    });
  });
}
