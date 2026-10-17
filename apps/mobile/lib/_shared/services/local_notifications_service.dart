import 'dart:async';

import 'package:firebase_messaging/firebase_messaging.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_local_notifications/flutter_local_notifications.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';

/// Presents Firebase messages as local notifications, and routes their taps.
///
/// Firebase draws its own notification when the app is backgrounded; this
/// service owns the foreground case, the tap that follows either one, and the
/// Android channel both need.
final class LocalNotificationsService {
  /// Creates a service over [_logger].
  const new(this._logger);

  final MenoLogger _logger;

  static final FlutterLocalNotificationsPlugin _plugin =
      FlutterLocalNotificationsPlugin();
  static const AndroidNotificationChannel _channel = AndroidNotificationChannel(
    'high_importance_channel',
    'High Importance Notifications',
    description: 'Meno notification channel',
    importance: Importance.high,
  );
  static const _androidDetails = AndroidNotificationDetails(
    'high_importance_channel',
    'High Importance Notifications',
    channelDescription: 'Meno notification channel',
    icon: '@drawable/ic_stat_ic_notification',
    importance: Importance.high,
    priority: Priority.high,
  );
  static const _iosDetails = DarwinNotificationDetails(
    presentAlert: true,
    presentBadge: true,
    presentSound: true,
  );
  static bool _channelReady = false;

  /// Creates the Android notification channel. Idempotent.
  static Future<void> setupChannel() async {
    if (_channelReady) return;
    await _plugin
        .resolvePlatformSpecificImplementation<
          AndroidFlutterLocalNotificationsPlugin
        >()
        ?.createNotificationChannel(_channel);
    _channelReady = true;
  }

  /// Shows [message] with no tap routing.
  ///
  /// Static because it runs on the background isolate, where no service exists
  /// and nothing can be injected.
  static Future<void> showStatic(RemoteMessage message) => _show(message);

  /// Wires the plugin and asks Firebase to show foreground messages too.
  Future<void> initialize() async {
    await setupChannel();

    await _plugin.initialize(
      settings: const InitializationSettings(
        android: AndroidInitializationSettings(
          '@drawable/ic_stat_ic_notification',
        ),
        iOS: DarwinInitializationSettings(),
      ),
      onDidReceiveNotificationResponse: (response) => _open(response.payload),
    );

    await FirebaseMessaging.instance
        .setForegroundNotificationPresentationOptions(
          alert: true,
          badge: true,
          sound: true,
        );
  }

  /// Shows [message] as a notification while the app is in the foreground.
  Future<void> show(RemoteMessage message) => _show(message);

  /// Handles a tap on a notification the app was launched from.
  void handleMessageTap(RemoteMessage message) => _open(_routeOf(message));

  /// Opens the notifications screen, or the route [payload] names.
  void _open(String? payload) {
    final context = rootNavigatorKey.currentContext;
    if (context == null) {
      _logger.w(
        'Dropped a notification tap: the app is not mounted',
        tag: MenoLogTag.push.label,
      );
      return;
    }
    unawaited(
      context.push(
        payload == null || payload.isEmpty ? MRoutes.notifications : payload,
      ),
    );
  }

  /// The route a message asks to open, when it names one.
  static String? _routeOf(RemoteMessage message) =>
      message.data['route'] as String?;

  /// Draws [message] on the channel, or does nothing when it carries no text.
  static Future<void> _show(RemoteMessage message) async {
    final notification = message.notification;
    // Firebase drew this one itself; redrawing it would double it.
    if (notification == null || message.notification?.android == null) return;
    if (kIsWeb) return;

    await setupChannel();
    await _plugin.show(
      id: notification.hashCode,
      title: notification.title,
      body: notification.body,
      notificationDetails: const NotificationDetails(
        android: _androidDetails,
        iOS: _iosDetails,
      ),
      payload: _routeOf(message),
    );
  }
}
