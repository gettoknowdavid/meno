import 'package:fpdart/fpdart.dart' show Either, Left, Right;
import 'package:meno_mobile/_core/value_objects/_value_objects.dart';

class TermsAcceptance extends ValueObject<bool> {
  factory(bool input) => TermsAcceptance._(_validate(input), input);

  const new _(super.value, this.input);

  /// What the user currently has selected, valid or not.
  ///
  /// The checkbox renders this, never [getOrCrash]: an unaccepted box is an
  /// expected state, not a crash.
  final bool input;

  static const TermsAcceptance empty = TermsAcceptance._(
    Left(RequiredValueException()),
    false,
  );

  static Either<ValueException<bool>, bool> _validate(bool input) {
    if (!input) {
      return const Left(
        RequiredValueException<bool>(
          msg: 'You must accept the Terms of Service to continue.',
        ),
      );
    }

    return Right(input);
  }
}
