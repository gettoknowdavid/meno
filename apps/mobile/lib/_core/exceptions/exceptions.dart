/// The failures the app's transport clients raise.
///
/// Two families live here, each raised by one client in `_shared/services` and
/// consumed a layer above it:
///
/// * [LiveKitFailure] — a room that could not be entered, or a microphone that
///   could not be switched;
/// * [SocketNotConnectedException] and [SocketTimeoutException] — a socket
///   write with nowhere to go, or an acknowledgement that never arrived.
///
/// They live here rather than beside those clients because they travel: the
/// client hands one back, the manager that called it decides whether to retry,
/// and the screen above that decides what to say. A vocabulary shared by those
/// three layers is not one transport's private business.
///
/// Nothing here knows about the LiveKit or Socket.IO SDKs. Each client
/// translates its transport's errors into these terms, which is what keeps the
/// SDKs out of every layer above them.
///
/// Everything in this file implements [InfrastructureFailure], which is the one
/// hook the rest of the app uses to recognise a transport failure: the app's
/// message resolver reads the message off it, and the command error filter
/// decides from it where a failure goes. A new transport failure therefore
/// needs to do nothing else to be accounted for.
library;

/// A failure raised by one of the app's transport clients.
///
/// These come from *reaching* the server rather than from what it answered, so
/// they are not `MenoException`s: there is no status line, no wire code and no
/// request id to carry, and there is nothing in the API's log to match one
/// against. What they do share with those failures is [message] — a sentence
/// written for a person — and that is all the app's error plumbing needs.
abstract interface class InfrastructureFailure implements Exception {
  /// The message to show a person for this failure.
  String get message;
}

/// A reason a room could not be entered.
sealed class LiveKitFailure implements InfrastructureFailure {
  const new(this.message);

  @override
  final String message;
}

/// The token was rejected, so no retry can help.
final class LiveKitInvalidToken extends LiveKitFailure {
  const new([super.message = 'Unable to connect: Invalid or expired token']);
}

/// The room did not answer in time.
final class LiveKitConnectionTimeout extends LiveKitFailure {
  const new([
    super.message = 'Connection timeout: Please check your internet connection',
  ]);
}

/// The room refused the connection for any other reason it reported.
final class LiveKitConnectionFailed extends LiveKitFailure {
  const new(super.message);
}

/// The client is not in a room, so there is nothing to act on.
final class LiveKitNotConnected extends LiveKitFailure {
  const new([super.message = 'Failed: Not connected to room']);
}

/// The local microphone could not be switched.
final class LiveKitMicError extends LiveKitFailure {
  const new(super.message);
}

/// A failure that is not one of the cases above.
final class LiveKitUnexpectedError extends LiveKitFailure {
  const new(super.message);
}

/// Thrown when a write is attempted on a socket that is not connected.
///
/// Nothing is wrong with the request: it simply has nowhere to go yet, which is
/// why it is a different failure from [SocketTimeoutException] — one is worth
/// retrying once the socket is up, the other is about the server.
final class SocketNotConnectedException implements InfrastructureFailure {
  const new(this.message);

  @override
  final String message;

  @override
  String toString() => 'SocketNotConnectedException: $message';
}

/// Thrown when an acknowledgement does not arrive within its deadline.
///
/// The write did reach the server; what did not come back is the server's
/// answer, so the request may or may not have been applied.
final class SocketTimeoutException implements InfrastructureFailure {
  const new(this.message);

  @override
  final String message;

  @override
  String toString() => 'SocketTimeoutException: $message';
}
