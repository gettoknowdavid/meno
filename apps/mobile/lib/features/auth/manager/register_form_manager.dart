import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_shared/_shared.dart';

/// What the registration form currently holds.
///
/// One immutable value rather than a notifier per field: every derived view a
/// screen needs — "can this be submitted", "how strong is the password", "which
/// rules are unmet" — is a projection of this, so there is no second place for
/// the form's state to live and disagree.
@immutable
final class RegisterForm {
  /// Creates form state.
  const new({
    this.fullName = FullName.empty,
    this.email = Email.empty,
    this.password = Password.emptyRegister,
  });

  /// The display name.
  final FullName fullName;

  /// The login address.
  final Email email;

  /// The chosen password.
  final Password password;

  /// Whether every field is acceptable to the API's rules.
  bool get isValid => fullName.isValid && email.isValid && password.isValid;

  /// A copy with the named fields replaced.
  RegisterForm copyWith({
    FullName? fullName,
    Email? email,
    Password? password,
  }) => RegisterForm(
    fullName: fullName ?? this.fullName,
    email: email ?? this.email,
    password: password ?? this.password,
  );

  @override
  bool operator ==(Object other) =>
      other is RegisterForm &&
      other.fullName == fullName &&
      other.email == email &&
      other.password == password;

  @override
  int get hashCode => Object.hash(fullName, email, password);
}

/// Owns the registration form's fields.
///
/// Registered in a page scope rather than the root, so two visits to the
/// registration screen cannot share a half-typed password, and the fields are
/// gone the moment the page is: no manual reset, and nothing to leak into the
/// next account.
///
/// Validation lives in the value objects ([FullName], [Email], [Password]) so it
/// matches the API's rules exactly; this manager only holds them.
final class RegisterFormManager implements Disposable {
  /// Creates an empty form.
  RegisterFormManager();

  final ValueNotifier<RegisterForm> _form = ValueNotifier(const RegisterForm());

  /// The whole form, for a widget that wants to render from one value.
  ValueListenable<RegisterForm> get form => _form;

  /// The display name as typed.
  ValueListenable<FullName> get fullName => _form.select((f) => f.fullName);

  /// The address as typed.
  ValueListenable<Email> get email => _form.select((f) => f.email);

  /// The password as typed.
  ValueListenable<Password> get password => _form.select((f) => f.password);

  /// Whether the form may be submitted.
  ValueListenable<bool> get isValid => _form.select((f) => f.isValid);

  /// How much of the password policy is satisfied, from 0 to 1.
  ValueListenable<double> get passwordStrength =>
      _form.select((f) => f.password.strength);

  /// The password rules the current input breaks.
  ValueListenable<List<ValueRule>> get brokenPasswordRules =>
      _form.select((f) => f.password.brokenRules);

  /// Records what was typed into the name field.
  void onFullNameChanged(String value) =>
      _form.value = _form.value.copyWith(fullName: FullName(value));

  /// Records what was typed into the address field.
  void onEmailChanged(String value) =>
      _form.value = _form.value.copyWith(email: Email(value));

  /// Records what was typed into the password field.
  void onPasswordChanged(String value) =>
      _form.value = _form.value.copyWith(password: Password.register(value));

  /// Empties every field.
  void reset() => _form.value = const RegisterForm();

  /// Releases the form state.
  @override
  FutureOr<dynamic> onDispose() {
    _form.dispose();
  }
}
