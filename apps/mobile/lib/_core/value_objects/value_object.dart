import 'package:equatable/equatable.dart';
import 'package:flutter/foundation.dart';
import 'package:fpdart/fpdart.dart';
import 'package:meno_mobile/_core/value_objects/_value_objects.dart';

abstract class IValueObject {
  bool get isValid;
}

@immutable
abstract class ValueObject<T> with Equatable implements IValueObject {
  /// Base constructor takes the pre-validated Either result.
  const new(this.value);

  /// The core state: the result of validation, stored immutably.
  final Either<ValueException<T>, T> value;

  ValueException<T>? get failureOrNull => value.fold((f) => f, (_) => null);

  /// The message for [failureOrNull], ready to show.
  String? get errorMsg => failureOrNull?.msg;

  T get getOrCrash => value.fold((failure) => throw failure, (valid) => valid);

  T? get getOrNull => value.fold((_) => null, (validV) => validV);

  T? get validInputOrNull => value.fold((_) => null, (validV) => validV);

  T getOrElse(T Function(ValueException<T>) dflt) => value.getOrElse(dflt);

  B fold<B>(
    B Function(ValueException<T> failure) ifLeft,
    B Function(T value) ifRight,
  ) => value.fold(ifLeft, ifRight);

  @override
  bool get isValid => value.isRight();

  @override
  List<Object?> get props => [value];

  @override
  bool? get stringify => true;
}

extension StringValueObjectX on ValueObject<String> {
  String get orEmpty => getOrElse((_) => '');
}
