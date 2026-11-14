import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';

/// Routes command failures that no handler claimed to the app log.
///
/// Widgets own what the user sees: the same failures also reach
/// `Command.globalErrors`, which the root widget turns into toasts. Keeping the
/// two apart means logging never needs a `BuildContext`, and this stays the one
/// place to add crash reporting later.
///
/// Call it once from `main()`, after the locator is configured.
void configureGlobalExceptionHandler() {
  Command.globalExceptionHandler = (commandError, stackTrace) {
    final error = commandError.error;
    final logger = di<MenoLogger>();

    if (error is MenoException) {
      // A cancellation is deliberate — a debounce, a screen closing, a request
      // whose account switched away. It is not a failure and reports nowhere.
      if (error is CancelledException) return;

      // An authentication refusal is an account matter, not a crash: the
      // interceptor already classified it, and whoever owns that account's
      // screen decides what to do. No global sign-out — a request that fails
      // as one identity must never touch another (design §5, not C01: D-23).
      if (error is UnauthenticatedException ||
          error is SessionUnavailableException) {
        final requestId = error.requestId;
        logger.w(
          'Auth refusal ${error.code}'
          '${requestId == null ? '' : ' [requestId: $requestId]'}',
          tag: MenoLogTag.auth.label,
          error: error,
          stackTrace: stackTrace,
        );
        return;
      }

      // The request id is the only handle on the server-side log lines for this
      // failure, so it is worth a line of its own in the message. A
      // [TransportFailure] has none — the API never answered — and the reason
      // it did happen, which is the only record of it, rides along on [error].
      final requestId = error.requestId;
      logger.e(
        'Unhandled ${error.runtimeType} (${error.code})'
        '${requestId == null ? '' : ' [requestId: $requestId]'}',
        tag: MenoLogTag.debug.label,
        error: error,
        stackTrace: stackTrace,
      );
      return;
    }

    // Not one of the app's failures at all: a bug, or a failure mode nobody has
    // classified yet, so only the type is known here.
    logger.e(
      'Unhandled ${error.runtimeType}',
      tag: MenoLogTag.debug.label,
      error: error,
      stackTrace: stackTrace,
    );
  };
}
