import 'package:flutter/foundation.dart';

/// One translation the API's catalogue offers for download.
///
/// The wire row is deliberately six fields — see
/// `apps/api/src/modules/bible/dto.rs` — and this type keeps exactly those:
/// what the translation is called, which version it is, and the two values a
/// download is verified against.
///
/// Nothing here is trusted until it has been verified: [bytes] and [sha256]
/// are what `BibleDownloadService` checks the received file against before
/// any of it reaches the store.
@immutable
final class BibleTranslationRelease {
  /// Creates a catalogue row.
  const new({
    required this.code,
    required this.name,
    required this.version,
    required this.bytes,
    required this.sha256,
    required this.payloadUrl,
  });

  /// Reads one catalogue row, refusing anything a download could not verify.
  ///
  /// Thrown from here rather than half-built: a row without a usable hash, a
  /// size, or an absolute URL would fail the download it advertises, and
  /// `HttpClient.getList` drops a row whose reader throws (and logs it), so
  /// one malformed entry does not hide the others.
  factory fromJson(Object? json) {
    if (json is! Map<String, dynamic>) {
      throw FormatException(
        'Expected a catalogue row but got ${json.runtimeType}.',
      );
    }

    final code = json['code'];
    final name = json['name'];
    final version = json['version'];
    final bytes = json['bytes'];
    final sha256 = json['sha256'];
    final payloadUrl = json['payloadUrl'];

    if (code is! String || code.isEmpty) {
      throw const FormatException('The catalogue row has no code.');
    }
    if (version is! num || version < 1) {
      throw FormatException('The row for $code has no usable version.');
    }
    if (bytes is! num || bytes <= 0) {
      throw FormatException('The row for $code has no usable size.');
    }
    if (sha256 is! String || !_sha256Pattern.hasMatch(sha256)) {
      throw FormatException('The row for $code has no usable SHA-256.');
    }

    final url = payloadUrl is String ? Uri.tryParse(payloadUrl) : null;
    if (url == null || !url.isScheme('http') && !url.isScheme('https')) {
      throw FormatException('The row for $code has no usable payload URL.');
    }

    return BibleTranslationRelease(
      code: code,
      name: name is String && name.isNotEmpty ? name : code.toUpperCase(),
      version: version.toInt(),
      bytes: bytes.toInt(),
      sha256: sha256,
      payloadUrl: payloadUrl as String,
    );
  }

  /// The translation code the verses are stored under, for example `kjv`.
  final String code;

  /// The name to show beside the code.
  final String name;

  /// The payload's version, compared with the device's seed ledger.
  final int version;

  /// The payload's exact byte length.
  final int bytes;

  /// Lowercase hex SHA-256 of the payload.
  final String sha256;

  /// Absolute URL the payload is served from, on the CDN rather than the API.
  final String payloadUrl;

  /// A hash is 32 bytes, written as 64 lowercase hex characters.
  static final RegExp _sha256Pattern = RegExp(r'^[0-9a-f]{64}$');

  @override
  bool operator ==(Object other) =>
      other is BibleTranslationRelease &&
      other.code == code &&
      other.version == version;

  @override
  int get hashCode => Object.hash(code, version);
}
