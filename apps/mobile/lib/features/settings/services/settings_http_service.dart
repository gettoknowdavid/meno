import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/settings/model/_model.dart';

/// The settings module's endpoints, one method each.
///
/// Mirrors `apps/api/src/modules/settings/handlers.rs`: a read, a partial update
/// and a token clear. Nothing here caches or decides — the manager owns that.
final class SettingsHttpService {
  /// Creates a service over [_client].
  const new(this._client);

  final HttpClient _client;

  /// `GET /settings` — the signed-in account's switches.
  ///
  /// The API answers with defaults for an account that has never saved any, so
  /// there is no 404 to handle.
  Future<UserSettings> getSettings() =>
      _client.get('/settings', fromJson: _settings);

  /// `PATCH /settings` — a partial update, answering with the fresh record.
  ///
  /// A field left null is untouched by the API. The API refuses a patch that
  /// carries nothing, so callers send only what changed. [display] is sent as
  /// its wire spelling; [notificationPreferences] as the free-form JSON object
  /// the API stores verbatim.
  Future<UserSettings> updateSettings({
    bool? pushNotifications,
    bool? appNotifications,
    bool? emailNotifications,
    UserDisplay? display,
    String? language,
    NotificationPreferences? notificationPreferences,
    String? pushNotificationToken,
  }) {
    return _client.patch(
      '/settings',
      data: {
        'push_notifications': ?pushNotifications,
        'app_notifications': ?appNotifications,
        'email_notifications': ?emailNotifications,
        'display': ?display?.wire,
        'language': ?language,
        'notification_preferences': ?notificationPreferences?.toDto.toJson(),
        'push_notification_token': ?pushNotificationToken,
      },
      fromJson: _settings,
    );
  }

  /// `DELETE /settings/push-token` — unset the device token.
  ///
  /// Idempotent: clearing a token that was never set is the state the caller
  /// asked for, not an error.
  Future<void> clearPushToken() => _client.deleteUnit('/settings/push-token');

  /// Reads the record both `/settings` endpoints answer with.
  static UserSettings _settings(dynamic data) =>
      SettingsDto.fromJson(data as Map<String, dynamic>).toDomain;
}
