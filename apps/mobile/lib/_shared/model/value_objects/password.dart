// 
// ignore_for_file: avoid_equals_and_hash_code_on_mutable_classes

import 'package:fpdart/fpdart.dart';
import 'package:meno_mobile/_shared/model/value_objects/value_object.dart';

/// How much of the policy a password is checked against.
///
/// Sign-in only asks that something was typed: applying the composition rules
/// there would tell an attacker which passwords *fail the rules*, which is free
/// feedback, and it would lock out any account whose password predates the
/// policy. Creating or resetting a password checks the whole thing.
enum PasswordPolicy {
  /// The password is being presented, not chosen.
  login,

  /// The password is being chosen, so every rule applies.
  register,
}

/// A password, checked the way the API checks one.
///
/// The rules are `password_problems` in
/// `apps/api/src/modules/auth/validators.rs`: 8–128 characters with at least
/// one lowercase and one uppercase letter. The upper bound is not a policy
/// opinion but a cost bound — the server refuses to allocate for an unbounded
/// field — so it is enforced here too.
///
/// Composition is deliberately only these three rules. The API does not require
/// a digit or a symbol, so a form that demanded one would refuse passwords the
/// server accepts.
final class Password extends ValueObject<String> {
  /// Checks [input] as a chosen password.
  factory register(String input) => Password._(
    check(input, PasswordPolicy.register),
    policy: PasswordPolicy.register,
    input: input,
  );

  /// Checks [input] as a presented password.
  factory login(String input) => Password._(
    check(input, PasswordPolicy.login),
    policy: PasswordPolicy.login,
    input: input,
  );

  /// Wraps an already-checked [value].
  ///
  /// The check runs in the factories, not in the initializer list: a `const`
  /// constructor cannot call [check], and the value a form starts from has to
  /// be a compile-time constant to be a field default.
  const new _(super.value, {required this.policy, required this.input});

  /// Which rules [input] was held to.
  final PasswordPolicy policy;

  /// What the user typed, kept so a form can show its progress without
  /// re-reading the checked value.
  final String input;

  /// The value a sign-in form starts from.
  static const Password emptyLogin = Password._(
    Left<ValueException, String>(_requiredProblem),
    policy: PasswordPolicy.login,
    input: '',
  );

  /// The value a sign-up form starts from.
  static const Password emptyRegister = Password._(
    Left<ValueException, String>(_requiredProblem),
    policy: PasswordPolicy.register,
    input: '',
  );

  /// What an empty password was refused for, shared by the two empties so they
  /// and [check] say the same thing.
  static const ValueException _requiredProblem = ValueException(
    ValueRule.required,
    'A password is required',
  );

  /// Shortest password the API accepts.
  static const int minLength = 8;

  /// Longest password the API accepts.
  static const int maxLength = 128;

  /// How many rules a chosen password is measured against.
  static const int ruleCount = 3;

  /// Every rule this password breaks, in the order they are shown.
  ///
  /// Empty while nothing has been typed, and empty under
  /// [PasswordPolicy.login]: an untouched field has not broken anything yet,
  /// and colouring the rules red before the first keystroke reads as an error
  /// the user did not make. A presented password has no composition rules to
  /// show at all.
  List<ValueException> get problems {
    if (input.isEmpty) return const [];
    if (policy == PasswordPolicy.login) return const [];
    return collectProblems(input);
  }

  /// The rules [problems] names, for widgets that only need to check them off.
  List<ValueRule> get brokenRules => [for (final p in problems) p.rule];

  /// Whether [rule] is among the rules this password breaks.
  bool breaks(ValueRule rule) => brokenRules.contains(rule);

  /// How much of the policy the current input satisfies, from 0 to 1.
  double get strength {
    if (input.isEmpty) return 0;
    return (ruleCount - brokenRules.length) / ruleCount;
  }

  /// Checks [input] against [policy].
  ///
  /// A presented password only has to be non-empty; a chosen one is held to
  /// every rule, and the first broken rule is the refusal.
  static Either<ValueException, String> check(
    String input,
    PasswordPolicy policy,
  ) {
    if (input.isEmpty) {
      return const Left<ValueException, String>(_requiredProblem);
    }
    if (policy == PasswordPolicy.login) {
      return Right<ValueException, String>(input);
    }

    final problems = collectProblems(input);
    if (problems.isNotEmpty) {
      return Left<ValueException, String>(problems.first);
    }
    return Right<ValueException, String>(input);
  }

  /// Every rule [input] breaks, as refusals.
  static List<ValueException> collectProblems(String input) {
    final problems = <ValueException>[];

    // Characters, not code units: "8 characters" has to mean the same thing for
    // a password containing an emoji.
    final length = input.runes.length;
    if (length < minLength) {
      problems.add(
        const ValueException(
          ValueRule.tooShort,
          'A password must be at least 8 characters',
        ),
      );
    } else if (length > maxLength) {
      problems.add(
        const ValueException(
          ValueRule.tooLong,
          'A password may be at most 128 characters',
        ),
      );
    }

    if (!RegExp('[a-z]').hasMatch(input)) {
      problems.add(
        const ValueException(
          ValueRule.missingLowercase,
          'A password must contain at least one lowercase letter',
        ),
      );
    }
    if (!RegExp('[A-Z]').hasMatch(input)) {
      problems.add(
        const ValueException(
          ValueRule.missingUppercase,
          'A password must contain at least one uppercase letter',
        ),
      );
    }

    return problems;
  }

  @override
  bool operator ==(Object other) =>
      other is Password &&
      other.input == input &&
      other.policy == policy &&
      other.value == value;

  @override
  int get hashCode => Object.hash(input, policy, value);
}
