/// The app's error taxonomy.
///
/// Every failure the app surfaces is a [MenoException]. Each one carries a
/// stable code — the value the app branches on — plus a human-readable message
/// the server may reword at any time. The failures the API answered carry the
/// wire code it documents, along with the status line and request id it sent;
/// the ones the app detects itself (no connectivity, a cancelled request, a
/// failed write, a room that would not open) use the client-side codes at the
/// bottom of [MenoErrorCode] and have no server-side counterpart to look up.
/// One vocabulary covers the whole app.
///
/// The hierarchy is sealed, and that is what keeps the app honest about it: the
/// error filter classifies a failure by switching over these types, so a
/// failure added here will not compile until somebody has said where it is
/// reported.
library;

/// Wire error codes, mirroring `ErrorCode` in `crates/core/src/error.rs`.
///
/// These strings are a stability contract: the API may add a code, but never
/// rename or remove one without an API version bump, so matching on them is
/// safe. Codes from [network] down are produced by the app and never travel on
/// the wire.
final class MenoErrorCode {
  /// Prevents instantiation: this type is a namespace for the codes.
  const new _();

  // 400 — the request is malformed and the caller must change it.
  static const String badRequest = 'BAD_REQUEST';
  static const String invalidCursor = 'INVALID_CURSOR';
  static const String invalidTimeZone = 'INVALID_TIME_ZONE';
  static const String startTimeInPast = 'START_TIME_IN_PAST';

  // 401 — no usable credentials, or credentials that identify nobody.
  static const String unauthorized = 'UNAUTHORIZED';
  static const String invalidCredentials = 'INVALID_CREDENTIALS';
  static const String invalidToken = 'INVALID_TOKEN';
  static const String tokenExpired = 'TOKEN_EXPIRED';
  static const String refreshTokenExpired = 'REFRESH_TOKEN_EXPIRED';

  // 403 — authenticated but not permitted.
  static const String forbidden = 'FORBIDDEN';
  static const String notCreator = 'NOT_CREATOR';
  static const String notOwner = 'NOT_OWNER';
  static const String notParticipant = 'NOT_PARTICIPANT';
  static const String emailNotVerified = 'EMAIL_NOT_VERIFIED';
  static const String providerDisabled = 'PROVIDER_DISABLED';

  // 404
  static const String notFound = 'NOT_FOUND';

  // 409 — the request conflicts with the resource's state.
  static const String conflict = 'CONFLICT';
  static const String emailTaken = 'EMAIL_TAKEN';
  static const String alreadyExists = 'ALREADY_EXISTS';
  static const String versionConflict = 'VERSION_CONFLICT';

  // 422 — field-level detail arrives in `data`.
  static const String validationFailed = 'VALIDATION_FAILED';

  // 429
  static const String rateLimited = 'RATE_LIMITED';

  // 503 — a dependency behind the API is unavailable.
  static const String upstreamUnavailable = 'UPSTREAM_UNAVAILABLE';

  // 500
  static const String internal = 'INTERNAL_ERROR';

  // Client-side only: produced by the app, never by the API.
  static const String network = 'NETWORK';
  static const String timeout = 'TIMEOUT';
  static const String cancelled = 'CANCELLED';
  static const String storage = 'STORAGE';
  static const String malformedResponse = 'MALFORMED_RESPONSE';
  static const String unknown = 'UNKNOWN';

  // Client-side only: refused before any bytes left the device, because the
  // request would have carried a credential somewhere it must not go.
  static const String wrongOrigin = 'WRONG_ORIGIN';

  // Client-side only: the account a request was made as has no usable session
  // any more. Sending it anonymously would turn "sign in again" into a server
  // rejection with no explanation.
  static const String sessionUnavailable = 'SESSION_UNAVAILABLE';

  // Client-side only: the API answered with a redirect, which is never
  // followed — a followed redirect would carry whatever it carried onward.
  static const String redirectNotAllowed = 'REDIRECT_NOT_ALLOWED';

  // Client-side only, transport: raised when reaching the server failed rather
  // than the server answering badly, so there is no wire code to mirror and
  // [timeout], [network], [invalidToken] and [unknown] are reused wherever they
  // already describe the failure honestly.
  static const String liveKitConnectFailed = 'LIVEKIT_CONNECT_FAILED';
  static const String liveKitMic = 'LIVEKIT_MIC';
}

/// A failure the app can act on.
///
/// [code] is the branchable value. [message] is written for people and may be
/// reworded at any time, so never match on it. [statusCode] and [requestId] are
/// set when the API answered: the status line is authoritative, and the request
/// id is what the server-side logs that produced the failure are tagged with.
///
/// The failures hang off two branches: the ones the API answered, each of which
/// carries a status, and [TransportFailure], which the app raised while
/// reaching the server and which therefore has none.
sealed class MenoException implements Exception {
  /// Creates a failure with a wire [code] and a user-facing [message].
  const new({
    required this.code,
    required this.message,
    this.statusCode,
    this.requestId,
    this.cause,
  });

  /// Maps a failure envelope onto the exception that matches its status.
  ///
  /// [code] is carried through untouched, so a caller can still tell
  /// `EMAIL_TAKEN` from `VERSION_CONFLICT` within the same 409 family.
  factory fromResponse({
    required int? statusCode,
    required String code,
    required String message,
    Map<String, List<String>>? fieldErrors,
    String? requestId,
    Duration? retryAfter,
  }) {
    // 503 is the exception to the 5xx rule: it says a dependency behind the
    // API is down rather than the API itself failing, and it is safe to retry —
    // so it is classified before the generic server branch would swallow it.
    if (statusCode != null && statusCode >= 500 && statusCode != 503) {
      return ServerException(
        message: message,
        code: code,
        statusCode: statusCode,
        requestId: requestId,
      );
    }

    return switch (statusCode) {
      400 => BadRequestException(
        message: message,
        code: code,
        requestId: requestId,
      ),
      401 => UnauthenticatedException(
        message: message,
        code: code,
        requestId: requestId,
      ),
      403 => ForbiddenException(
        message: message,
        code: code,
        requestId: requestId,
      ),
      404 => NotFoundException(
        message: message,
        code: code,
        requestId: requestId,
      ),
      409 => ConflictException(
        message: message,
        code: code,
        requestId: requestId,
      ),
      422 => ValidationException(
        fieldErrors ?? const {},
        message: message,
        requestId: requestId,
      ),
      429 => RateLimitedException(
        message: message,
        retryAfter: retryAfter,
        requestId: requestId,
      ),
      503 => UpstreamUnavailableException(
        message: message,
        retryAfter: retryAfter,
        requestId: requestId,
      ),
      _ => UnknownException(
        message: message,
        code: code,
        statusCode: statusCode,
        requestId: requestId,
      ),
    };
  }

  /// The stable code to branch on: the wire code when the API answered, and one
  /// of the client-side codes in [MenoErrorCode] when the app raised it.
  final String code;

  /// The message to show a person for this failure.
  final String message;

  /// The HTTP status, when the failure came from the API.
  final int? statusCode;

  /// The API's correlation id for the request, when it sent one.
  final String? requestId;

  /// The failure this one wraps, when there is one.
  final Object? cause;

  @override
  String toString() =>
      'MenoException($code, statusCode: $statusCode, message: $message)';
}

/// The request was rejected before it reached a handler.
///
/// Real cases are `INVALID_CURSOR` and `START_TIME_IN_PAST`: the caller can fix
/// the request, but no generic UI can, so callers that know how to react branch
/// on [code].
final class BadRequestException extends MenoException {
  /// Creates a 400 failure.
  const new({
    required super.message,
    super.code = MenoErrorCode.badRequest,
    super.statusCode = 400,
    super.requestId,
    super.cause,
  });
}

/// The caller is not signed in, or the session is no longer valid.
///
/// [code] says which: `INVALID_CREDENTIALS` means a sign-in attempt was
/// rejected (a form shows it), anything else means the stored session must be
/// discarded and the user signed in again.
final class UnauthenticatedException extends MenoException {
  /// Creates a 401 failure.
  const new({
    required super.message,
    super.code = MenoErrorCode.unauthorized,
    super.statusCode = 401,
    super.requestId,
    super.cause,
  });
}

/// The caller is signed in but not allowed to do this.
final class ForbiddenException extends MenoException {
  /// Creates a 403 failure.
  const new({
    required super.message,
    super.code = MenoErrorCode.forbidden,
    super.statusCode = 403,
    super.requestId,
    super.cause,
  });
}

/// The resource does not exist, or is soft-deleted.
final class NotFoundException extends MenoException {
  /// Creates a 404 failure.
  const new({
    required super.message,
    super.code = MenoErrorCode.notFound,
    super.statusCode = 404,
    super.requestId,
    super.cause,
  });
}

/// The request conflicts with the resource's current state.
final class ConflictException extends MenoException {
  /// Creates a 409 failure.
  const new({
    required super.message,
    super.code = MenoErrorCode.conflict,
    super.statusCode = 409,
    super.requestId,
    super.cause,
  });
}

/// One or more fields failed validation.
///
/// The per-field messages travel in the failure envelope's `data`, keyed by
/// field name — exactly what a form needs to mark its inputs.
final class ValidationException extends MenoException {
  /// Creates a 422 failure carrying [fieldErrors].
  const new(
    this.fieldErrors, {
    super.message = 'One or more fields are invalid',
    super.code = MenoErrorCode.validationFailed,
    super.statusCode = 422,
    super.requestId,
    super.cause,
  });

  /// The messages the API sent, keyed by field name.
  final Map<String, List<String>> fieldErrors;

  /// The messages for [field], or an empty list when the field was accepted.
  List<String> messagesFor(String field) =>
      fieldErrors[field] ?? const <String>[];
}

/// The API is throttling this client.
final class RateLimitedException extends MenoException {
  /// Creates a 429 failure, optionally with the server's retry hint.
  const new({
    required super.message,
    this.retryAfter,
    super.code = MenoErrorCode.rateLimited,
    super.statusCode = 429,
    super.requestId,
    super.cause,
  });

  /// How long the API asked the client to wait, from `Retry-After`.
  final Duration? retryAfter;
}

/// A dependency behind the API is unavailable.
///
/// The request never reached domain code, so it is safe to try again once
/// [retryAfter] has passed.
final class UpstreamUnavailableException extends MenoException {
  /// Creates a 503 failure, optionally with the server's retry hint.
  const new({
    required super.message,
    this.retryAfter,
    super.code = MenoErrorCode.upstreamUnavailable,
    super.statusCode = 503,
    super.requestId,
    super.cause,
  });

  /// How long the API asked the client to wait, from `Retry-After`.
  final Duration? retryAfter;
}

/// The API failed while handling the request.
///
/// The detail is in the server log, correlated by [requestId]; the message is
/// deliberately generic.
final class ServerException extends MenoException {
  /// Creates a 5xx failure.
  const new({
    required super.message,
    super.code = MenoErrorCode.internal,
    super.statusCode = 500,
    super.requestId,
    super.cause,
  });
}

/// The device could not reach the API.
final class NetworkException extends MenoException {
  /// Creates a connectivity failure.
  const new({
    super.message = 'You appear to be offline. Check your connection.',
    super.code = MenoErrorCode.network,
    super.cause,
    super.statusCode,
  });
}

/// The API did not answer within the request's timeout.
final class RequestTimeoutException extends MenoException {
  /// Creates a timeout failure.
  const new({
    super.message = 'The request took too long. Please try again.',
    super.code = MenoErrorCode.timeout,
    super.cause,
  });
}

/// The request was cancelled before it completed.
///
/// Never user-facing: cancellations are deliberate (a debounce, a screen
/// closing), so the error filter keeps them out of both handlers and toasts.
final class CancelledException extends MenoException {
  /// Creates a cancellation failure.
  const new({
    super.message = 'The request was cancelled.',
    super.code = MenoErrorCode.cancelled,
    super.cause,
  });
}

/// A credential was about to be sent somewhere other than the approved API
/// origin, so the request was refused before it was sent.
final class WrongOriginException extends MenoException {
  /// Creates an origin refusal.
  const new({
    super.message = 'The request was sent to an untrusted address.',
    super.code = MenoErrorCode.wrongOrigin,
    super.cause,
  });
}

/// The account a request belongs to has no usable session any more.
///
/// "Sign in again" rather than "send it anonymously": the caller asked to act
/// as somebody, and the API's answer to an anonymous request would hide that.
final class SessionUnavailableException extends MenoException {
  /// Creates a session-unavailable failure.
  const new({
    super.message = 'Please sign in again.',
    super.code = MenoErrorCode.sessionUnavailable,
    super.cause,
  });
}

/// The API answered with a redirect.
///
/// Redirects are never followed: following one would forward the request — and
/// anything attached to it — to an address that was never approved.
final class RedirectNotAllowedException extends MenoException {
  /// Creates a redirect refusal.
  const new({
    super.message = 'The server redirected the request.',
    super.code = MenoErrorCode.redirectNotAllowed,
    super.statusCode,
    super.requestId,
    super.cause,
  });
}

/// The device could not read or write its own storage.
final class StorageException extends MenoException {
  /// Creates a storage failure.
  const new({
    super.message = 'The app could not save your data. Please try again.',
    super.code = MenoErrorCode.storage,
    super.cause,
  });
}

/// The API answered with a body that is not the documented envelope.
///
/// Seen when something in front of the API answers instead of it — a proxy
/// error page, a truncated body.
final class MalformedResponseException extends MenoException {
  /// Creates a malformed-response failure.
  const new({
    super.message = 'The server sent a response the app could not read.',
    super.code = MenoErrorCode.malformedResponse,
    super.statusCode,
    super.requestId,
    super.cause,
  });
}

/// A failure the app has no specific handling for.
///
/// Reaching this means either a bug or a failure mode nobody has classified
/// yet; the log carries the detail.
final class UnknownException extends MenoException {
  /// Creates an unclassified failure.
  const new({
    super.message = 'Something went wrong. Please try again.',
    super.code = MenoErrorCode.unknown,
    super.statusCode,
    super.requestId,
    super.cause,
  });
}

/// A failure raised while reaching the server rather than by what it answered.
///
/// These come from the app's own transport clients: a room that would not open,
/// a microphone that would not switch, a socket write with nowhere to go, an
/// acknowledgement that never arrived. There was no status line, no request id
/// and nothing in the API's log to match one against, so [code] is one of the
/// client-side codes and [statusCode] and [requestId] are left unset.
///
/// They are part of the taxonomy rather than a family of their own so that the
/// error plumbing recognises them by type alone: the error filter routes the
/// whole branch in one case, and a new failure of this kind cannot be added
/// without deciding where it is reported.
sealed class TransportFailure extends MenoException {
  const new({required super.code, required super.message, super.cause});
}

/// A reason a room could not be entered, or a microphone could not be switched.
sealed class LiveKitFailure extends TransportFailure {
  const new({required super.code, required super.message, super.cause});
}

/// The token was rejected, so no retry can help.
final class LiveKitInvalidToken extends LiveKitFailure {
  const new([String message = 'Unable to connect: Invalid or expired token'])
    : super(message: message, code: MenoErrorCode.invalidToken);
}

/// The room did not answer in time.
final class LiveKitConnectionTimeout extends LiveKitFailure {
  const new([
    String message =
        'Connection timeout: Please check your internet connection',
  ]) : super(message: message, code: MenoErrorCode.timeout);
}

/// The room refused the connection for any other reason it reported.
final class LiveKitConnectionFailed extends LiveKitFailure {
  const new(String message)
    : super(message: message, code: MenoErrorCode.liveKitConnectFailed);
}

/// The client is not in a room, so there is nothing to act on.
final class LiveKitNotConnected extends LiveKitFailure {
  const new([String message = 'Failed: Not connected to room'])
    : super(message: message, code: MenoErrorCode.network);
}

/// The local microphone could not be switched.
final class LiveKitMicError extends LiveKitFailure {
  const new(String message)
    : super(message: message, code: MenoErrorCode.liveKitMic);
}

/// A failure that is not one of the cases above.
final class LiveKitUnexpectedError extends LiveKitFailure {
  const new(String message)
    : super(message: message, code: MenoErrorCode.unknown);
}

/// A write was attempted on a socket that is not connected.
///
/// Nothing is wrong with the request: it simply has nowhere to go yet, which is
/// why it is a different failure from [SocketTimeoutException] — one is worth
/// retrying once the socket is up, the other is about the server.
final class SocketNotConnectedException extends TransportFailure {
  const new(String message)
    : super(message: message, code: MenoErrorCode.network);

  @override
  String toString() => 'SocketNotConnectedException: $message';
}

/// An acknowledgement did not arrive within its deadline.
///
/// The write did reach the server; what did not come back is the server's
/// answer, so the request may or may not have been applied.
final class SocketTimeoutException extends TransportFailure {
  const new(String message)
    : super(message: message, code: MenoErrorCode.timeout);

  @override
  String toString() => 'SocketTimeoutException: $message';
}

/// The message to show a person for [error].
///
/// Every failure the app recognises carries a message written for users — the
/// ones the API answered and the ones a transport client raised. Anything else
/// is unclassified, and a raw `toString()` would leak internals into the UI —
/// so the fallback is a generic sentence and the detail goes to the log.
String errorMessage(Object? error) {
  if (error is MenoException) return error.message;
  return 'Something went wrong. Please try again.';
}
