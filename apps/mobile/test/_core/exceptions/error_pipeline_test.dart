import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';

/// The two decisions the error pipeline makes about a failure: what a person is
/// told, and who gets to handle it.
///
/// Both rest on the failure being a [MenoException]. A transport failure has no
/// wire code to read a status off, so these pin that it is recognised by the
/// same plumbing as every other failure rather than falling through to the
/// generic sentence that is reserved for failures nobody has classified.
void main() {
  group('errorMessage', () {
    test('prefers the message a transport failure carries', () {
      expect(
        errorMessage(const LiveKitInvalidToken()),
        'Unable to connect: Invalid or expired token',
      );
      expect(
        errorMessage(const LiveKitConnectionTimeout()),
        'Connection timeout: Please check your internet connection',
      );
      expect(
        errorMessage(const LiveKitConnectionFailed('the room refused us')),
        'the room refused us',
      );
      expect(
        errorMessage(const SocketNotConnectedException('socket is down')),
        'socket is down',
      );
      expect(
        errorMessage(const SocketTimeoutException('no ack within 20s')),
        'no ack within 20s',
      );
    });

    test('still unwraps an API failure', () {
      expect(
        errorMessage(const NotFoundException(message: 'No such broadcast')),
        'No such broadcast',
      );
    });

    test('falls back to a generic sentence for anything unclassified', () {
      expect(
        errorMessage(StateError('leaked internals')),
        'Something went wrong. Please try again.',
      );
      expect(errorMessage(null), 'Something went wrong. Please try again.');
    });
  });

  group('menoExceptionFilter', () {
    test('lets a local handler own a transport failure', () {
      expect(
        menoExceptionFilter(
          const LiveKitConnectionTimeout(),
          StackTrace.current,
        ),
        ErrorReaction.firstLocalThenGlobalHandler,
      );
      expect(
        menoExceptionFilter(
          const SocketNotConnectedException('not connected'),
          StackTrace.current,
        ),
        ErrorReaction.firstLocalThenGlobalHandler,
      );
    });

    test('still sends an unclassified failure straight to the toast', () {
      expect(
        menoExceptionFilter(StateError('boom'), StackTrace.current),
        ErrorReaction.globalHandler,
      );
    });

    test('keeps the policy the API failures have today', () {
      expect(
        menoExceptionFilter(const CancelledException(), StackTrace.current),
        ErrorReaction.none,
      );
      expect(
        menoExceptionFilter(const NetworkException(), StackTrace.current),
        ErrorReaction.globalHandler,
      );
      expect(
        menoExceptionFilter(const ValidationException({}), StackTrace.current),
        ErrorReaction.localHandler,
      );
    });
  });
}
