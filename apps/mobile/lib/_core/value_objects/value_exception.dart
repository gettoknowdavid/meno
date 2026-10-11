import 'package:equatable/equatable.dart';

sealed class ValueException<T> with Equatable implements Exception {
  const new({required this.code, required this.msg});

  final String code;
  final String msg;

  @override
  List<Object?> get props => [code, msg];
}

final class UnexpectedValueException<T> extends ValueException<T> {
  const new({
    super.code = 'unexpected',
    super.msg = 'An unexpected exception occurred.',
  });
}

final class RequiredValueException<T> extends ValueException<T> {
  const new({
    super.code = 'required-value',
    super.msg = 'This value cannot be empty.',
  });
}

final class InvalidValueException<T> extends ValueException<T> {
  const new(
    this.value, {
    super.code = 'invalid-value',
    super.msg = 'Invalid value.',
  });

  final T value;
}

final class LengthExceededValueException<T> extends ValueException<T> {
  const new(
    this.value, {
    this.maxLength = 244,
    super.code = 'length-exceeded',
    super.msg = 'Value length exceeded.',
  });

  final T value;
  final int maxLength;
}

final class MaxLinesExceededValueException<T> extends ValueException<T> {
  const new(
    this.value,
    this.maxLines, {
    super.code = 'max-lines-exceeded',
    super.msg = 'Must be a single line text.',
  });

  final T value;
  final int maxLines;
}

final class InvalidFileFormatException<T> extends ValueException<T> {
  const new(
    this.value, {
    super.code = 'invalid-format',
    super.msg = 'The file format selected is invalid.',
  });

  final T value;
}

final class PasswordTooShort extends ValueException<String> {
  const new({
    super.code = 'too-short',
    super.msg = 'Password must be at least 8 characters',
  });
}

final class PasswordMissingLowercase extends ValueException<String> {
  const new({
    super.code = 'missing-lowercase',
    super.msg = 'Please include a lowercase letter in your password',
  });
}

final class PasswordMissingUppercase extends ValueException<String> {
  const new({
    super.code = 'missing-uppercase',
    super.msg = 'Please include a uppercase letter in your password',
  });
}

final class PasswordMissingSpecialCharacter extends ValueException<String> {
  const new({
    super.code = 'missing-special-character',
    super.msg = 'Please include a special character in your password',
  });
}

final class PasswordMissingNumber extends ValueException<String> {
  const new({
    super.code = 'missing-number',
    super.msg = 'Please include a number in your password',
  });
}

extension PasswordValidationErrorX on ValueException<String> {
  String? get title {
    switch (this) {
      case PasswordMissingLowercase():
        return 'a';
      case PasswordMissingNumber():
        return '123';
      case PasswordMissingSpecialCharacter():
        return '%';
      case PasswordMissingUppercase():
        return 'A';
      case PasswordTooShort():
        return '8+';
      default:
        return null;
    }
  }

  String? get subtitle {
    switch (this) {
      case PasswordMissingLowercase():
        return 'Lowercase';
      case PasswordMissingNumber():
        return 'Number';
      case PasswordMissingSpecialCharacter():
        return 'Symbol';
      case PasswordMissingUppercase():
        return 'Uppercase';
      case PasswordTooShort():
        return 'Characters';
      default:
        return null;
    }
  }
}
