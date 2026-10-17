import 'package:firebase_messaging/firebase_messaging.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';

/// The app's Firebase Cloud Messaging boundary.
final class PushNotificationsService implements Disposable {
  /// Creates a service over the platform's messaging client.
  const new(this._fcm, this._logger);

  final FirebaseMessaging _fcm;
  final MenoLogger _logger;

  /// Foreground messages.
  Stream<RemoteMessage> get onMessage => FirebaseMessaging.onMessage;

  /// A new device token, whenever FCM rotates it.
  Stream<String> get onTokenRefresh => _fcm.onTokenRefresh;

  /// Messages the user tapped while the app was backgrounded.
  Stream<RemoteMessage> get onMessageOpenedApp =>
      FirebaseMessaging.onMessageOpenedApp;

  /// Asks for (or reports) notification permission.
  Future<NotificationSettings> requestPermissions() =>
      _fcm.requestPermission(provisional: true);

  /// The message that launched the app, when it was opened from one.
  Future<RemoteMessage?> getInitialMessage() => _fcm.getInitialMessage();

  /// The current device token, or null when it cannot be read.
  ///
  /// Never logged: the token is a credential, and a log line is where it would
  /// leak.
  Future<String?> getToken() async {
    try {
      return await _fcm.getToken();
    } on Exception catch (error, stackTrace) {
      _logger.e(
        'Could not read the FCM token',
        tag: MenoLogTag.push.label,
        error: error,
        stackTrace: stackTrace,
      );
      return null;
    }
  }

  @override
  Future<void> onDispose() async {}
}
