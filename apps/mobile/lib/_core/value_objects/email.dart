import 'package:fpdart/fpdart.dart';
import 'package:meno_mobile/_core/value_objects/_value_objects.dart';

class Email extends ValueObject<String> {
  factory(String input) => Email._(_validate(input));

  const new _(super.value);

  static final RegExp _emailRegex = RegExp(
    r'^[a-zA-Z0-9.]+@[a-zA-Z0-9]+\.[a-zA-Z]+',
  );

  /// Returns a ValueObject that contains a Failure (Left).
  static const Email empty = Email._(Left(RequiredValueException()));

  static Either<ValueException<String>, String> _validate(String input) {
    final trimmed = input.trim();

    if (trimmed.isEmpty) return const Left(RequiredValueException());

    if (trimmed.length > _maxLength) {
      return Left(
        LengthExceededValueException(
          trimmed,
          maxLength: _maxLength,
          msg: 'Email address must not exceed $_maxLength characters.',
        ),
      );
    }

    if (!_emailRegex.hasMatch(trimmed)) {
      return Left(InvalidValueException(trimmed, msg: 'Invalid email address'));
    }

    return Right(trimmed);
  }

  /// Longest address that can exist, per RFC 5321 §4.5.3.1.
  static const int _maxLength = 254;
}
