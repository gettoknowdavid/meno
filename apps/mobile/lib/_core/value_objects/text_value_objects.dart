import 'package:fpdart/fpdart.dart' show Either, Left, Right;
import 'package:meno_mobile/_core/value_objects/_value_objects.dart';

class FullName extends ValueObject<String> {
  factory(String input) {
    final clean = input.trim();
    return FullName._(_validate(clean));
  }

  /// The wire's own value, kept without running the form policy.
  ///
  /// A name that survived the server's validation must never be rejected on
  /// the way into storage.
  factory fromWire(String input) => FullName._(Right(input.trim()));

  const new _(super.value);

  static const FullName empty = FullName._(Left(RequiredValueException()));

  static Either<ValueException<String>, String> _validate(String input) {
    if (input.isEmpty) return const Left(RequiredValueException());

    if (input.runes.length < _minLength) {
      return Left(
        InvalidValueException(
          input,
          msg: 'Full name must be at least $_minLength characters.',
        ),
      );
    }

    if (input.contains('\n')) {
      return Left(MaxLinesExceededValueException(input, 1));
    }

    if (input.runes.length > _maxLength) {
      return Left(
        LengthExceededValueException(
          input,
          maxLength: _maxLength,
          msg: 'Full name must be less than $_maxLength characters.',
        ),
      );
    }

    if (RegExp(r'[\x00-\x1F\x7F]').hasMatch(input)) {
      return Left(
        InvalidValueException(
          input,
          msg: 'Full name must not contain control characters.',
        ),
      );
    }

    return Right(input);
  }

  /// The bounds `full_name_problems` enforces in
  /// `apps/api/src/modules/auth/validators.rs`, counted in Unicode scalars as
  /// the server counts them.
  static const int _minLength = 2;
  static const int _maxLength = 100;
}

class SingleLineString extends ValueObject<String> {
  factory(String input) {
    final clean = input.trim();
    return SingleLineString._(_validate(clean));
  }

  const new _(super.value);

  static const SingleLineString empty = SingleLineString._(
    Left(RequiredValueException()),
  );

  static Either<ValueException<String>, String> _validate(String input) {
    if (input.isEmpty) return const Left(RequiredValueException());

    if (input.contains('\n')) {
      return Left(MaxLinesExceededValueException(input, 1));
    }

    return Right(input);
  }
}

class MultiLineString extends ValueObject<String> {
  /// Factory accepts an optional [maxLength].
  factory(String input, {int? maxLength}) {
    final sanitized = input.trim();
    return MultiLineString._(_validate(sanitized, maxLength));
  }

  const new _(super.value);

  static const MultiLineString empty = MultiLineString._(
    Left(RequiredValueException()),
  );

  static Either<ValueException<String>, String> _validate(
    String input,
    int? maxLength,
  ) {
    if (input.isEmpty) return const Left(RequiredValueException());

    if (maxLength != null && input.length > maxLength) {
      return Left(LengthExceededValueException(input, maxLength: maxLength));
    }

    return Right(input);
  }

  bool get isEmpty => value.getOrElse((_) => '').isEmpty;
  bool get isNotEmpty => value.getOrElse((_) => '').isNotEmpty;
}
