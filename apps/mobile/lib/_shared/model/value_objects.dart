import 'package:fpdart/fpdart.dart';
import 'package:meno_mobile/_shared/model/value_object.dart';

/// A login address, checked the way the API checks it.
///
/// The rules are a port of `email_problem` in
/// `apps/api/src/modules/auth/validators.rs`, and the messages are that
/// function's, word for word. The API remains the authority — this only saves a
/// round trip on an address the server would have refused anyway.
///
/// Surrounding whitespace is trimmed rather than refused: a phone keyboard adds
/// a trailing space to the whole form, and rejecting a login over a character
/// the user cannot see is a support ticket.
final class Email extends ValueObject<String> {
  /// Checks [input] as an address.
  factory(String input) {
    final trimmed = input.trim();

    if (trimmed.isEmpty) return Email.empty;

    if (trimmed.length > _maxLength) {
      return const Email._(Left<ValueException, String>(_tooLongProblem));
    }

    final at = trimmed.indexOf('@');
    if (at < 0 || trimmed.indexOf('@', at + 1) >= 0) {
      return const Email._(Left<ValueException, String>(_noAtSignProblem));
    }

    final local = trimmed.substring(0, at);
    final domain = trimmed.substring(at + 1);

    if (_hasBadLocalPart(local)) {
      return const Email._(Left<ValueException, String>(_badLocalPartProblem));
    }
    if (_hasBadDomain(domain)) {
      return const Email._(Left<ValueException, String>(_badDomainProblem));
    }

    return Email._(Right<ValueException, String>(trimmed));
  }

  const new _(super.value);

  /// The value a form starts from: empty, and therefore refused.
  static const Email empty = Email._(
    Left<ValueException, String>(_requiredProblem),
  );

  /// Longest address that can exist, per RFC 5321 §4.5.3.1.
  static const int _maxLength = 254;

  /// Longest local part, per RFC 5321 §4.5.3.1.
  static const int _maxLocalLength = 64;

  /// Longest DNS label, per RFC 1035 §2.3.4.
  static const int _maxLabelLength = 63;

  static const String _required = 'An email address is required';
  static const String _tooLong =
      'An email address may be at most 254 characters';
  static const String _noAtSign =
      "An email address must contain exactly one '@'";
  static const String _badLocalPart =
      "The part before '@' may not be empty, start or end with '.', "
      'or contain ".."';
  static const String _badDomain =
      "The part after '@' must be a domain such as 'example.com', without a "
      "leading or trailing '-'";

  static const ValueException _requiredProblem = ValueException(
    ValueRule.required,
    _required,
  );
  static const ValueException _tooLongProblem = ValueException(
    ValueRule.tooLong,
    _tooLong,
  );
  static const ValueException _noAtSignProblem = ValueException(
    ValueRule.invalidFormat,
    _noAtSign,
  );
  static const ValueException _badLocalPartProblem = ValueException(
    ValueRule.invalidFormat,
    _badLocalPart,
  );
  static const ValueException _badDomainProblem = ValueException(
    ValueRule.invalidFormat,
    _badDomain,
  );

  /// Whether [domain] is unusable as a mail domain.
  ///
  /// At least one dot is required: `ada@localhost` is syntactically valid and
  /// undeliverable, and accepting it registers an account that can never
  /// receive a verification code.
  static bool _hasBadDomain(String domain) {
    if (domain.isEmpty || !domain.contains('.')) return true;
    if (domain.endsWith('.')) return true;

    return domain
        .split('.')
        .any(
          (label) =>
              label.isEmpty ||
              label.length > _maxLabelLength ||
              label.startsWith('-') ||
              label.endsWith('-') ||
              !RegExp(r'^[A-Za-z0-9-]+$').hasMatch(label),
        );
  }

  /// Whether [local] is unusable as the part before the `@`.
  ///
  /// A *quoted* local part is the RFC's own exception — the quotes are part of
  /// the syntax — so it is accepted as long as it encloses no control
  /// character. Anything else containing whitespace is a paste error, and
  /// trimming it would register an account at an address nobody typed.
  static bool _hasBadLocalPart(String local) {
    final quoted = local.startsWith('"') && local.endsWith('"');
    if (quoted) {
      final inner = local.substring(1, local.length - 1);
      return RegExp(r'[\x00-\x1F\x7F]').hasMatch(inner);
    }

    if (local.isEmpty || local.length > _maxLocalLength) return true;
    if (RegExp(r'\s').hasMatch(local)) return true;
    if (local.startsWith('.') || local.endsWith('.') || local.contains('..')) {
      return true;
    }
    return false;
  }
}

/// A display name, checked the way the API checks one.
///
/// The rules and messages come from `full_name_problems` in
/// `apps/api/src/modules/auth/validators.rs`. Two characters is the floor
/// because a single character is not a name, and 100 is the ceiling because
///  that is what the `users` table renders.
final class FullName extends ValueObject<String> {
  /// Checks [input] as a display name.
  factory(String input) {
    final trimmed = input.trim();

    if (trimmed.length < _minLength) return FullName.empty;

    if (trimmed.length > _maxLength) {
      return const FullName._(Left<ValueException, String>(_tooLongProblem));
    }
    if (RegExp(r'[\x00-\x1F\x7F]').hasMatch(trimmed)) {
      return const FullName._(
        Left<ValueException, String>(_controlCharactersProblem),
      );
    }

    return FullName._(Right<ValueException, String>(trimmed));
  }

  const new _(super.value);

  /// The value a form starts from: empty, and therefore refused.
  static const FullName empty = FullName._(
    Left<ValueException, String>(_requiredProblem),
  );

  static const int _minLength = 2;
  static const int _maxLength = 100;

  static const String _required = 'A full name is required';
  static const String _tooLong = 'A full name may be at most 100 characters';
  static const String _hasControlCharacters =
      'A full name may not contain control characters';

  static const ValueException _requiredProblem = ValueException(
    ValueRule.tooShort,
    _required,
  );
  static const ValueException _tooLongProblem = ValueException(
    ValueRule.tooLong,
    _tooLong,
  );
  static const ValueException _controlCharactersProblem = ValueException(
    ValueRule.hasControlCharacters,
    _hasControlCharacters,
  );
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
  /// Checks [input] as a presented password.
  factory login(String input) => Password._(
    check(input, PasswordPolicy.login),
    policy: PasswordPolicy.login,
    input: input,
  );

  /// Checks [input] as a chosen password.
  factory register(String input) => Password._(
    check(input, PasswordPolicy.register),
    policy: PasswordPolicy.register,
    input: input,
  );

  /// Wraps an already-checked [value].
  ///
  /// The check runs in the factories, not in the initializer list: a `const`
  /// constructor cannot call [check], and the value a form starts from has to
  /// be a compile-time constant to be a field default.
  const new _(super.value, {required this.policy, required this.input});

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

  /// Which rules [input] was held to.
  final PasswordPolicy policy;

  /// What the user typed, kept so a form can show its progress without
  /// re-reading the checked value.
  final String input;

  /// The rules [problems] names, for widgets that only need to check them off.
  List<ValueRule> get brokenRules => [for (final p in problems) p.rule];

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

  @override
  List<Object?> get props => [input, policy, value];

  /// How much of the policy the current input satisfies, from 0 to 1.
  double get strength {
    if (input.isEmpty) return 0;
    return (ruleCount - brokenRules.length) / ruleCount;
  }

  /// Whether [rule] is among the rules this password breaks.
  bool breaks(ValueRule rule) => brokenRules.contains(rule);

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
}

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
