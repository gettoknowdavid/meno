import 'dart:convert' show utf8;

import 'package:fpdart/fpdart.dart';
import 'package:meno_mobile/_core/value_objects/_value_objects.dart';

/// An email address, measured against the *server's* policy rather than a
/// client-side guess.
///
/// The rules here mirror `email_problem` in
/// `apps/api/src/modules/auth/validators.rs`, because a server-valid address
/// the client rejects means an account that cannot sign in. The numbers come
/// from that file's constants, not from this class's own opinion.
class Email extends ValueObject<String> {
  factory(String input) => Email._(_validate(input));

  /// The wire's own value, kept without running the form policy.
  ///
  /// Mapping a server response must accept server truth: an address that
  /// survived the server's validation can never be rejected on the way into
  /// storage.
  factory fromWire(String input) => Email._(Right(input.trim()));

  const new _(super.value);

  /// Returns a ValueObject that contains a Failure (Left).
  static const Email empty = Email._(Left(RequiredValueException()));

  static Either<ValueException<String>, String> _validate(String input) {
    final trimmed = input.trim();

    if (trimmed.isEmpty) return const Left(RequiredValueException());

    if (utf8.encode(trimmed).length > _maxEmailLength) {
      return Left(
        LengthExceededValueException(
          trimmed,
          maxLength: _maxEmailLength,
          msg: 'Email address must not exceed $_maxEmailLength characters.',
        ),
      );
    }

    if (!_acceptedByServer(trimmed)) {
      return Left(InvalidValueException(trimmed, msg: 'Invalid email address'));
    }

    return Right(trimmed);
  }

  /// Whether [address] passes the server's structural check.
  ///
  /// Mirrors `email_problem`: exactly one `@`, a local part that is non-empty,
  /// within the length bound, free of unquoted whitespace, and with no
  /// leading/trailing/double dot; and a domain whose labels are ASCII
  /// alphanumeric or hyphen, within the label bound, and containing at least
  /// one dot.
  static bool _acceptedByServer(String address) {
    final at = address.indexOf('@');
    if (at < 0 || address.indexOf('@', at + 1) >= 0) return false;

    final local = address.substring(0, at);
    final domain = address.substring(at + 1);

    // The RFC's quoted local part: the quotes are syntax, so whitespace
    // inside them is fine — what must not happen is a stray quote smuggling
    // a space past the check.
    final quoted =
        local.startsWith('"') && local.endsWith('"') && local.length >= 2;
    if (quoted) {
      final inner = local.substring(1, local.length - 1);
      if (inner.isEmpty || inner.contains(RegExp(r'[\x00-\x1F\x7F]'))) {
        return false;
      }
    } else if (local.contains(RegExp(r'\s'))) {
      return false;
    }

    if (local.isEmpty || utf8.encode(local).length > _maxLocalLength) {
      return false;
    }
    if (local.startsWith('.') || local.endsWith('.') || local.contains('..')) {
      return false;
    }

    return !_badDomain(domain);
  }

  /// Whether [domain] is unusable as a mail domain — the server's
  /// `bad_domain`, including its refusal of a non-ASCII (IDN) domain.
  static bool _badDomain(String domain) {
    if (domain.isEmpty || !domain.contains('.')) return true;
    // A trailing dot is legal in DNS (the root) and never what a user typed.
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

  /// The same bounds the server declares, in the same units (bytes).
  static const int _maxEmailLength = 254;
  static const int _maxLocalLength = 64;
  static const int _maxLabelLength = 63;
}
