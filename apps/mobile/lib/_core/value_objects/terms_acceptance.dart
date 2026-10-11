import 'package:fpdart/fpdart.dart' show Either, Left, Right;
import 'package:meno_mobile/_core/value_objects/_value_objects.dart';

class TermsAcceptance extends ValueObject<bool> {
  factory(bool input) => TermsAcceptance._(_validate(input));

  const new _(super.value);

  static const TermsAcceptance empty = TermsAcceptance._(
    Left(RequiredValueException()),
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
