import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/value_objects/_value_objects.dart';

final class RegisterFormManager implements Disposable {
  new();

  final fullName = ValueNotifier<FullName>(.empty);
  final email = ValueNotifier<Email>(.empty);
  final password = ValueNotifier<Password>(.emptyRegister);
  final terms = ValueNotifier<TermsAcceptance>(.empty);

  late final ValueListenable<Set<ValueException<String>>?> passwordRules =
      password.select(
        (pwd) => pwd.input.isEmpty ? null : pwd.brokenRules.toSet(),
      );

  late final ValueListenable<double> strength = passwordRules.select((rules) {
    if (rules == null) return 0.0;
    const total = 5;
    return (total - rules.length) / total;
  });

  late final ValueListenable<String> passwordStrengthLabel = strength.select((
    s,
  ) {
    if (s == 0.0) return '';
    if (s <= 0.4) return 'Weak';
    if (s <= 0.8) return 'Good';
    return 'Strong';
  });

  late final ValueListenable<bool> isValid = fullName.combineLatest4(
    email,
    password,
    terms,
    (fn, em, pw, tr) => fn.isValid && em.isValid && pw.isValid && tr.isValid,
  );

  void onFullNameChanged(String value) => fullName.value = FullName(value);

  void onEmailChanged(String value) => email.value = Email(value);

  void onPasswordChanged(String value) => password.value = .register(value);

  void onTermsChanged(bool? v) => terms.value = TermsAcceptance(v ?? false);

  void reset() {
    fullName.value = .empty;
    email.value = .empty;
    password.value = .emptyRegister;
    terms.value = .empty;
  }

  @override
  FutureOr<dynamic> onDispose() {
    fullName.dispose();
    email.dispose();
    password.dispose();
    terms.dispose();
  }
}
