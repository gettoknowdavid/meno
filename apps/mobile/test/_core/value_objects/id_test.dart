import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:uuid/uuid.dart';

void main() {
  const valid = 'a1a1a1a1-0001-4a1a-8a1a-1a1a1a1a1a1a';

  group('Id', () {
    test('accepts a valid uuid', () {
      final id = Id.fromString(valid);

      expect(id.isValid, isTrue);
      expect(id.getOrNull, valid);
      expect(id.isEmpty, isFalse);
    });

    test('trims surrounding whitespace before validating', () {
      final id = Id.fromString('  $valid  ');

      expect(id.isValid, isTrue);
      expect(id.getOrCrash, valid);
    });

    test('rejects an empty string as empty', () {
      final id = Id.fromString('');

      expect(id.isValid, isFalse);
      expect(id.isEmpty, isTrue);
      expect(id.failureOrNull, isA<RequiredValueException<String>>());
    });

    test('rejects whitespace only as empty', () {
      final id = Id.fromString('   ');

      expect(id.isValid, isFalse);
      expect(id.isEmpty, isTrue);
    });

    test('rejects a string that is not a uuid', () {
      final id = Id.fromString('not-a-uuid');

      expect(id.isValid, isFalse);
      expect(id.isEmpty, isFalse);
      expect(id.failureOrNull, isA<InvalidValueException<String>>());
    });

    test('Id.empty is empty', () {
      expect(Id.empty.isEmpty, isTrue);
      expect(Id.empty.isValid, isFalse);
    });

    test('a blank id is empty in the same way Id.empty is', () {
      expect(Id.fromString('').isEmpty, Id.empty.isEmpty);
    });

    test('unique is a valid uuid', () {
      final id = Id.unique();

      expect(id.isValid, isTrue);
      expect(id.isEmpty, isFalse);
      expect(Uuid.isValidUUID(fromString: id.getOrCrash), isTrue);
    });

    test('unique generates distinct ids', () {
      expect(Id.unique(), isNot(Id.unique()));
    });
  });
}
