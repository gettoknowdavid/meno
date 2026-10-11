import 'package:fpdart/fpdart.dart';
import 'package:meno_mobile/_core/value_objects/_value_objects.dart';
import 'package:uuid/uuid.dart';

class Id extends ValueObject<String> {
  /// Creates a valid, unique UUID v4.
  factory unique() => Id._(Right(const Uuid().v4()));

  /// Validates a string input.
  factory fromString(String input) => Id._(_validate(input));

  const new _(super.value);

  /// Failed ID to handle specific empty logic
  static const Id empty = Id._(Left(RequiredValueException(msg: 'Invalid ID')));

  /// Validation Logic
  static Either<ValueException<String>, String> _validate(String input) {
    final clean = input.trim();
    if (clean.isEmpty) return const Left(RequiredValueException<String>());

    try {
      if (!Uuid.isValidUUID(fromString: clean)) {
        return Left(InvalidValueException(clean, msg: 'Invalid UUID format'));
      }
    } on Exception catch (_) {
      return Left(InvalidValueException(clean, msg: 'Invalid UUID format'));
    }

    return Right(clean);
  }
}

extension IdX on Id {
  bool get isEmpty => this == Id.empty;
}
