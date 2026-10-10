//
// ignore_for_file: avoid_equals_and_hash_code_on_mutable_classes

import 'package:fpdart/fpdart.dart';

/// The rule a value broke.
///
/// A closed set rather than free-form text: the rule is what a screen compares,
/// switches on and checks off in a list, while [ValueException.msg] is only
/// what a person reads. Two failures that mean the same thing therefore share a
/// [ValueRule] even when the API words them differently.
enum ValueRule {
  /// Nothing was entered.
  required,

  /// Too few characters.
  tooShort,

  /// Too many characters.
  tooLong,

  /// The right length, but not a usable shape.
  invalidFormat,

  /// No lowercase letter.
  missingLowercase,

  /// No uppercase letter.
  missingUppercase,

  /// A control character (a newline, a NUL) that no field should carry.
  hasControlCharacters,
}

/// Why a value was refused.
///
/// The messages mirror the ones `apps/api/src/modules/auth/validators.rs`
/// produces, so a form that checks on the client shows the same words the API
/// would have used — and a field never says one thing offline and another thing
/// online.
final class ValueException implements Exception {
  /// Creates a refusal with the [rule] it broke and the [msg] to show.
  const new(this.rule, this.msg);

  /// The rule that failed.
  final ValueRule rule;

  /// The sentence a form shows next to the field.
  final String msg;

  @override
  bool operator ==(Object other) =>
      other is ValueException && other.rule == rule && other.msg == msg;

  @override
  int get hashCode => Object.hash(rule, msg);

  @override
  String toString() => msg;
}

/// A value that has been checked before it is used.
///
/// The check happens once, at construction, and the result is carried as an
/// `Either` so a caller cannot treat an invalid value as usable: reading it
/// requires saying which side it is on. Managers pass [getOrCrash] on the way
/// to the API (validation has already happened) and forms read [failureOrNull]
/// to mark their fields.
abstract class ValueObject<T extends Object> {
  /// Wraps an already-checked [value].
  const new(this.value);

  /// The value, or why it was refused.
  final Either<ValueException, T> value;

  /// Whether the input was accepted.
  bool get isValid => value.isRight();

  /// The accepted value, or null.
  T? get getOrNull => value.fold((_) => null, (accepted) => accepted);

  /// Why the input was refused, or null.
  ValueException? get failureOrNull =>
      value.fold((error) => error, (_) => null);

  /// The message for [failureOrNull], ready to show.
  String? get errorMessage => failureOrNull?.msg;

  /// The accepted value.
  ///
  /// Throws the [ValueException] when the input was refused — which is only
  /// reachable from a caller that skipped its own validation.
  T get getOrCrash =>
      value.fold((error) => throw error, (accepted) => accepted);

  /// The accepted value, or [fallback]'s result for the refusal.
  T getOrElse(T Function(ValueException error) fallback) =>
      value.fold(fallback, (accepted) => accepted);

  @override
  String toString() =>
      value.fold((error) => error.msg, (accepted) => '$accepted');
}
