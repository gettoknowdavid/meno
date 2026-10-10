import 'package:fpdart/fpdart.dart';
import 'package:meno_mobile/_shared/model/value_objects/value_object.dart';

/// A display name, checked the way the API checks one.
///
/// The rules and messages come from `full_name_problems` in
/// `apps/api/src/modules/auth/validators.rs`. Two characters is the floor
/// because a single character is not a name, and 100 is the ceiling because
///  that is what the `users` table renders.
final class FullName extends ValueObject<String> {
  /// Checks [input] as a display name.
  factory(String input) {
    final trimmed = input.trim();

    if (trimmed.length < _minLength) return FullName.empty;

    if (trimmed.length > _maxLength) {
      return const FullName._(Left<ValueException, String>(_tooLongProblem));
    }
    if (RegExp(r'[\x00-\x1F\x7F]').hasMatch(trimmed)) {
      return const FullName._(
        Left<ValueException, String>(_controlCharactersProblem),
      );
    }

    return FullName._(Right<ValueException, String>(trimmed));
  }

  const new _(super.value);

  /// The value a form starts from: empty, and therefore refused.
  static const FullName empty = FullName._(
    Left<ValueException, String>(_requiredProblem),
  );

  static const int _minLength = 2;
  static const int _maxLength = 100;

  static const String _required = 'A full name is required';
  static const String _tooLong = 'A full name may be at most 100 characters';
  static const String _hasControlCharacters =
      'A full name may not contain control characters';

  static const ValueException _requiredProblem = ValueException(
    ValueRule.tooShort,
    _required,
  );
  static const ValueException _tooLongProblem = ValueException(
    ValueRule.tooLong,
    _tooLong,
  );
  static const ValueException _controlCharactersProblem = ValueException(
    ValueRule.hasControlCharacters,
    _hasControlCharacters,
  );
}
