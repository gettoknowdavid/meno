import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/exceptions/meno_exception.dart';

/// Decides how a failed command is reported.
///
/// A command's failure is either handled by the widget that ran it (a form
/// marking its fields) or by the app's global handler (a toast, plus the log).
/// This filter is where that choice is made for every command, so no call site
/// has to remember it:
///
/// * infrastructure failures — no connectivity, a timeout, the API or a
///   dependency being down, a bad response, a failed write — go global. No
///   widget can do anything better with them than say so.
/// * a [TransportFailure] goes to a local handler when one exists and to a
///   toast when none does, because the manager that raised it usually owns the
///   outcome — a room connection is retried and shown in place, and one toast
///   per attempt would report a single failure several times.
/// * `VALIDATION_FAILED` goes local: the message belongs on the fields.
/// * rejected credentials go local too — the sign-in form is the only sensible
///   place to show "that email and password do not match".
/// * anything else the API answered about the request (400/403/404/409/429)
///   goes to a local handler when one exists and to a toast when none does,
///   because a widget may legitimately want to react to a specific code.
/// * cancellations go nowhere: they are deliberate, not failures.
///
/// Apply it when creating a command:
/// ```dart
/// Command.createAsyncNoResult(request, errorFilter: menoExceptionFilter);
/// ```
ErrorReaction menoExceptionFilter(Object error, StackTrace stackTrace) {
  if (error is! MenoException) return .globalHandler;

  return switch (error) {
    CancelledException() => .none,
    ValidationException() => .localHandler,
    UnauthenticatedException() =>
      error.code == MenoErrorCode.invalidCredentials
          ? .localHandler
          : .globalHandler,
    // Raised by one of the app's own transport clients rather than answered by
    // the API: a room that would not open, a socket that is not up, an
    // acknowledgement that never came. There is no response to read a status
    // off, so the whole branch is classified in one case.
    TransportFailure() => .firstLocalThenGlobalHandler,
    BadRequestException() ||
    ForbiddenException() ||
    NotFoundException() ||
    ConflictException() ||
    RateLimitedException() => .firstLocalThenGlobalHandler,
    NetworkException() ||
    RequestTimeoutException() ||
    UpstreamUnavailableException() ||
    ServerException() ||
    StorageException() ||
    MalformedResponseException() ||
    WrongOriginException() ||
    SessionUnavailableException() ||
    RedirectNotAllowedException() ||
    InsecureConnectionException() ||
    UnknownException() => .globalHandler,
  };
}
