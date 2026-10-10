import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:freezed_annotation/freezed_annotation.dart';
import 'package:meno_mobile/_shared/_shared.dart';

part 'register_form_manager.freezed.dart';

@freezed
abstract class RegisterForm with _$RegisterForm {
  /// Creates form state.
  const factory RegisterForm({
    @Default(FullName.empty) FullName fullName,
    @Default(Email.empty) Email email,
    @Default(Password.emptyRegister) Password password,
  }) = _RegisterForm;

  const RegisterForm._();

  /// Whether every field is acceptable to the API's rules.
  bool get isValid => fullName.isValid && email.isValid && password.isValid;
}

final class RegisterFormManager implements Disposable {
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

  @override
  FutureOr<dynamic> onDispose() {
    _form.dispose();
  }
}
