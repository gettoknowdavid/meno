import 'package:meno_mobile/_core/_core.dart';

/// The API's response envelope: `{code, message, status, data?, meta?}`.
///
/// Every endpoint answers with this shape, successes and failures alike — see
/// `apps/api/src/types/meno_response.rs`. Two details matter to callers:
///
/// * the HTTP status line is authoritative, and the body does not repeat it;
/// * `data` is *omitted* when an endpoint has no payload, so it is nullable
///   here and its absence is not an error.
final class MenoResponse<T> {
  /// Creates an envelope.
  const new({
    required this.code,
    required this.message,
    required this.status,
    this.data,
    this.meta,
  });

  /// Parses [json] as the envelope, decoding `data` with [fromJson].
  ///
  /// Throws a [MalformedResponseException] when [json] is not the envelope —
  /// which is what an error page from something in front of the API looks like.
  factory fromJson(dynamic json, T? Function(dynamic data) fromJson) {
    if (json is! Map<String, dynamic>) {
      throw MalformedResponseException(
        cause: json,
        message: 'Expected a response envelope but got ${json.runtimeType}.',
      );
    }

    final code = json['code'];
    final status = json['status'];
    if (code is! String || status is! bool) {
      throw MalformedResponseException(
        cause: json,
        message: 'The response envelope has no code or status.',
      );
    }

    final data = json['data'];
    final meta = json['meta'];

    return MenoResponse<T>(
      code: code,
      message: json['message'] as String? ?? '',
      status: status,
      data: data == null ? null : fromJson(data),
      meta: meta is Map<String, dynamic> ? ResponseMeta.fromJson(meta) : null,
    );
  }

  /// The stable, machine-readable result code — `OK` and `CREATED` on success.
  final String code;

  /// A message written for people. May be reworded at any time.
  final String message;

  /// Whether the request succeeded.
  final bool status;

  /// The payload, absent when the endpoint returns nothing.
  final T? data;

  /// Correlation metadata, when the transport layer supplied any.
  final ResponseMeta? meta;

  /// Whether the envelope carries a payload.
  bool get hasData => data != null;
}

/// Metadata the API attaches to either envelope.
final class ResponseMeta {
  /// Creates metadata.
  const new({this.requestId, this.apiVersion});

  /// Parses [json] as the envelope's `meta` object.
  factory fromJson(Map<String, dynamic> json) => ResponseMeta(
    requestId: json['requestId'] as String?,
    apiVersion: json['apiVersion'] as String?,
  );

  /// Correlates this response with the server-side log lines behind it.
  final String? requestId;

  /// The build of the API that answered.
  final String? apiVersion;
}
