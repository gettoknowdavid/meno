import 'package:fpdart/fpdart.dart';
import 'package:meno_mobile/_core/exceptions/meno_exception.dart';
import 'package:meno_mobile/_core/value_objects/_value_objects.dart';
import 'package:uuid/uuid.dart';

class Id extends ValueObject<String> {
  /// Creates a valid, unique UUID v4.
  factory unique() => Id._(Right(const Uuid().v4()));

  /// Validates a string input.
  factory fromString(String input) => Id._(_validate(input));

  /// Converts a value the wire sent into an id, or fails the response.
  ///
  /// Used at the boundary — DTO mapping, router parameters — where an id that
  /// is not a UUID means the server sent something the app cannot act on.
  factory fromWire(String input) {
    final id = Id.fromString(input);
    if (id.isValid) return id;

    throw MalformedResponseException(
      cause: id.failureOrNull,
      message: 'Expected a UUID but the wire said ${input.trim()}.',
    );
  }

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
  bool get isEmpty => value.fold(
    (failure) => failure is RequiredValueException<String>,
    (_) => false,
  );
}
