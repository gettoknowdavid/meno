import 'package:fpdart/fpdart.dart' show Either, Left, Right;
import 'package:meno_mobile/_core/value_objects/_value_objects.dart';

class Password extends ValueObject<String> {
  factory login(String input) {
    const mode = PasswordMode.login;
    return Password._(_validate(input, mode), input, mode);
  }

  factory register(String input) {
    const mode = PasswordMode.register;
    return Password._(_validate(input, mode), input, mode);
  }

  const new _(super.value, this.input, this.mode);

  final String input;
  final PasswordMode mode;

  List<ValueException<String>> get brokenRules => _collectBrokenRules(input);

  bool isRuleBroken(ValueException<String> rule) {
    return brokenRules.any((r) => r.runtimeType == rule.runtimeType);
  }

  static const Password emptyLogin = Password._(
    Left(RequiredValueException()),
    '',
    PasswordMode.login,
  );

  static const Password emptyRegister = Password._(
    Left(RequiredValueException()),
    '',
    PasswordMode.register,
  );

  static Either<ValueException<String>, String> _validate(
    String input,
    PasswordMode mode,
  ) {
    if (input.isEmpty) return const Left(RequiredValueException());

    if (mode == PasswordMode.register) {
      final rules = _collectBrokenRules(input);
      if (rules.isNotEmpty) {
        return Left(rules.first);
      }
    }
    return Right(input);
  }

  static List<ValueException<String>> _collectBrokenRules(String input) {
    if (input.isEmpty) return [];

    final errors = <ValueException<String>>[];

    if (input.length < 8) {
      errors.add(const PasswordTooShort());
    }

    if (!RegExp('[A-Z]').hasMatch(input)) {
      errors.add(const PasswordMissingUppercase());
    }

    if (!RegExp('[a-z]').hasMatch(input)) {
      errors.add(const PasswordMissingLowercase());
    }

    if (!RegExp('[0-9]').hasMatch(input)) {
      errors.add(const PasswordMissingNumber());
    }

    if (!RegExp(r'[@$!%*?&_]').hasMatch(input)) {
      errors.add(const PasswordMissingSpecialCharacter());
    }

    return errors;
  }

  @override
  bool operator ==(Object other) {
    if (identical(this, other)) return true;
    return other is Password &&
        other.value == value &&
        other.input == input &&
        other.mode == mode;
  }

  @override
  int get hashCode => value.hashCode ^ input.hashCode ^ mode.hashCode;
}

enum PasswordMode { login, register }
