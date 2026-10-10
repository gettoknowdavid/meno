import 'package:fpdart/fpdart.dart';
import 'package:meno_mobile/_shared/model/value_objects/value_object.dart';

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
}
