import 'dart:async';

import 'package:firebase_messaging/firebase_messaging.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/notification/manager/notifications_feed_data_source.dart';
import 'package:meno_mobile/features/notification/model/_model.dart';
import 'package:meno_mobile/features/notification/services/notifications_http_service.dart';
import 'package:meno_mobile/features/notification/services/notifications_socket_service.dart';
import 'package:meno_mobile/features/settings/model/_model.dart';
import 'package:meno_mobile/features/settings/services/settings_http_service.dart';

/// The account's inbox: the badge, the toast stack, and this device's push
/// registration.
///
/// # Two sources, one truth
///
/// The badge is the API's unread count, not a count of the rows this client
/// happens to hold: the inbox pages, so a number derived from the first page is
/// wrong the moment twenty-one unread notifications exist. A socket frame bumps
/// it optimistically and the count is re-read straight after, which settles the
/// badge on the server's answer.
///
/// # The frame is a nudge, not a record
///
/// `notification` carries `{userId, title, body, timestamp}` — no id, no
/// type, no deep link. A card built from it would have nothing to open, so the
/// refreshes the inbox and toasts its newest row: the same row the list draws,
/// with the same copy and the same action.
///
/// # Push is best-effort
///
/// A device without Firebase, or a build without the plugin, keeps the inbox
/// and the socket it can have. Nothing here asks for permission either — the
/// permissions flow owns that prompt.
final class NotificationsManager implements Disposable {
  /// Creates the manager over the account's services.
  new({
    required NotificationsHttpService http,
    required this._socket,
    required this._pushService,
    required this._localNotifications,
    required this._settings,
    required this._logger,
  }) : _http = http,
       feed = NotificationsFeedSource(http);

  final NotificationsHttpService _http;
  final NotificationsSocketService _socket;
  final PushNotificationsService _pushService;
  final LocalNotificationsService _localNotifications;
  final SettingsHttpService _settings;
  final MenoLogger _logger;

  /// The inbox list this manager and the notifications page share.
  final NotificationsFeedSource feed;

  /// The number the home badge draws.
  final unreadCount = ValueNotifier<int>(0);

  /// This device's Firebase token, once it is known.
  final fcmToken = ValueNotifier<String?>(null);

  /// How many notifications arrived while the app was open.
  final newNotificationCount = ValueNotifier<int>(0);

  /// The notifications the toast stack draws, newest first.
  final recentNotifications = ValueNotifier<List<Notification>>([]);

  StreamSubscription<NotificationSignal>? _notificationSubscription;
  StreamSubscription<RemoteMessage>? _pushNotificationSubscription;
  StreamSubscription<String>? _tokenRefreshSubscription;
  StreamSubscription<RemoteMessage>? _backgroundTapSubscription;

  Timer? _dismissTimer;
  bool _isDisposed = false;

  /// Starts delivery: the inbox, the badge, and this device's registration.
  ///
  /// Called once per account, as the account scope creates the manager.
  Future<void> init() async {
    feed.updateDataCommand.listen((_, _) => unawaited(syncUnreadCount()));

    _notificationSubscription = _socket.onNotification.listen(_onSignal);

    await _wirePush();

    feed.updateDataCommand.run();
    await syncUnreadCount();
  }

  /// Re-reads the unread count and puts it on the badge.
  ///
  /// A failure is logged and leaves the last known count in place: a badge that
  /// briefly lags is better than one that clears itself because a request timed
  /// out.
  Future<void> syncUnreadCount() async {
    if (_isDisposed) return;

    try {
      final count = await _http.unreadCount();
      if (!_isDisposed) unreadCount.value = count;
    } on Object catch (error, stackTrace) {
      _logger.w(
        'Could not read the unread notification count',
        tag: MenoLogTag.network.label,
        error: error,
        stackTrace: stackTrace,
      );
    }
  }

  /// Clears the toast stack.
  void dismissToasts() => recentNotifications.value = [];

  /// Resets the count of notifications that arrived while the app was open.
  void resetNewCount() => newNotificationCount.value = 0;

  /// Handles one socket frame: bump what is known, then re-read the truth.
  void _onSignal(NotificationSignal signal) {
    unreadCount.value += 1;
    newNotificationCount.value += 1;

    _logger.d(
      'Notification arrived: ${signal.title}',
      tag: MenoLogTag.push.label,
    );

    unawaited(_toastLatest());
  }

  /// Refreshes the inbox and toasts its newest row.
  Future<void> _toastLatest() async {
    try {
      await feed.updateDataCommand.runAsync();
    } on Object catch (error, stackTrace) {
      _logger.w(
        'Could not refresh the inbox after a notification arrived',
        tag: MenoLogTag.network.label,
        error: error,
        stackTrace: stackTrace,
      );
    }

    if (_isDisposed) return;

    final latest = feed.items.isEmpty ? null : feed.items.first.notification;
    if (latest == null) return;

    recentNotifications.value = [
      latest,
      ...recentNotifications.value.where((n) => n.id != latest.id),
    ].take(3).toList();

    _dismissTimer?.cancel();
    _dismissTimer = Timer(const Duration(seconds: 4), dismissToasts);
  }

  /// Wires the device's push channel: reads only, nothing is prompted for.
  Future<void> _wirePush() async {
    try {
      fcmToken.value = await _pushService.getToken();
      unawaited(_registerToken(fcmToken.value));

      _tokenRefreshSubscription = _pushService.onTokenRefresh.listen((token) {
        fcmToken.value = token;
        unawaited(_registerToken(token));
      });

      await _localNotifications.initialize();

      _pushNotificationSubscription = _pushService.onMessage.listen(
        _localNotifications.show,
      );

      _backgroundTapSubscription = _pushService.onMessageOpenedApp.listen(
        _localNotifications.handleMessageTap,
      );

      final initial = await _pushService.getInitialMessage();
      if (initial != null) _localNotifications.handleMessageTap(initial);
    } on Object catch (error, stackTrace) {
      _logger.w(
        'Push notifications are unavailable on this device',
        tag: MenoLogTag.push.label,
        error: error,
        stackTrace: stackTrace,
      );
    }
  }

  /// `PATCH /settings` — tells the API which device token to deliver to.
  ///
  /// The token is written through the settings service because that is the
  /// field the API stores it in. The token itself is never logged: it is a
  /// credential.
  Future<void> _registerToken(String? token) async {
    if (token == null || token.isEmpty) return;

    try {
      await _settings.updateSettings(
        SettingsPatchDto(pushNotificationToken: token),
      );
    } on Object catch (error, stackTrace) {
      _logger.w(
        'Could not register this device for push notifications',
        tag: MenoLogTag.push.label,
        error: error,
        stackTrace: stackTrace,
      );
    }
  }

  @override
  FutureOr<dynamic> onDispose() async {
    _isDisposed = true;

    await _notificationSubscription?.cancel();
    await _pushNotificationSubscription?.cancel();
    await _tokenRefreshSubscription?.cancel();
    await _backgroundTapSubscription?.cancel();

    _notificationSubscription = null;
    _pushNotificationSubscription = null;
    _tokenRefreshSubscription = null;
    _backgroundTapSubscription = null;

    _dismissTimer?.cancel();
    _dismissTimer = null;

    unreadCount.dispose();
    fcmToken.dispose();
    newNotificationCount.dispose();
    recentNotifications.dispose();

    await feed.onDispose();
  }
}
