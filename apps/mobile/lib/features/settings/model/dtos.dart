import 'package:freezed_annotation/freezed_annotation.dart';
import 'package:meno_mobile/features/settings/model/entities.dart';

part 'dtos.freezed.dart';
part 'dtos.g.dart';

/// The `notification_preferences` sub-object on the wire.
///
/// The API stores this verbatim, so the keys are the app's own. They are
/// camelCase, matching the domain entity.
@freezed
abstract class NotificationPreferencesDto with _$NotificationPreferencesDto {
  /// Creates the wire preferences.
  const factory({
    @Default(false) bool liveBroadcastStarted,
    @Default(false) bool userSubscribed,
    @Default(false) bool addedAsCoHost,
    @Default(false) bool scheduledBroadcast,
  }) = _NotificationPreferencesDto;

  /// Reads the sub-object the API returned.
  factory fromJson(Map<String, dynamic> json) =>
      _$NotificationPreferencesDtoFromJson(json);
}

/// `GET /settings` and `PATCH /settings` in Dart.
///
/// Field names are the wire's — snake_case, as `SettingsResponse` declares them
/// in `apps/api/src/modules/settings/dto.rs`. `display` decodes to the enum whose
/// name is the wire spelling. `has_push_token` reports only whether a device
/// token is on file; the token itself never leaves the server.
@freezed
abstract class SettingsDto with _$SettingsDto {
  /// Creates the wire settings.
  const factory({
    @JsonKey(name: 'push_notifications') @Default(true) bool pushNotifications,
    @JsonKey(name: 'app_notifications') @Default(true) bool appNotifications,
    @JsonKey(name: 'email_notifications')
    @Default(true)
    bool emailNotifications,
    @Default(UserDisplay.system) UserDisplay display,
    @Default('en') String language,
    @JsonKey(name: 'notification_preferences')
    @Default(NotificationPreferencesDto())
    NotificationPreferencesDto notificationPreferences,
    @JsonKey(name: 'has_push_token') @Default(false) bool hasPushToken,
  }) = _SettingsDto;

  /// Reads the settings the API returned.
  factory fromJson(Map<String, dynamic> json) => _$SettingsDtoFromJson(json);
}

/// The domain view of a [NotificationPreferencesDto].
extension NotificationPreferencesDtoX on NotificationPreferencesDto {
  /// This sub-object as the app models it.
  NotificationPreferences get toDomain => NotificationPreferences(
    liveBroadcastStarted: liveBroadcastStarted,
    userSubscribed: userSubscribed,
    addedAsCoHost: addedAsCoHost,
    scheduledBroadcast: scheduledBroadcast,
  );
}

/// The wire view of [NotificationPreferences].
extension NotificationPreferencesX on NotificationPreferences {
  /// This sub-object as the API writes it.
  NotificationPreferencesDto get toDto => NotificationPreferencesDto(
    liveBroadcastStarted: liveBroadcastStarted,
    userSubscribed: userSubscribed,
    addedAsCoHost: addedAsCoHost,
    scheduledBroadcast: scheduledBroadcast,
  );
}

/// The domain view of a [SettingsDto].
extension SettingsDtoX on SettingsDto {
  /// This record as the app models it.
  UserSettings get toDomain => UserSettings(
    display: display,
    language: language,
    appNotifications: appNotifications,
    pushNotifications: pushNotifications,
    emailNotifications: emailNotifications,
    notificationPreferences: notificationPreferences.toDomain,
    hasPushToken: hasPushToken,
  );
}

/// The wire view of [UserSettings].
extension UserSettingsX on UserSettings {
  /// This record as the API writes it, for the local cache.
  SettingsDto get toDto => SettingsDto(
    display: display,
    language: language,
    appNotifications: appNotifications,
    pushNotifications: pushNotifications,
    emailNotifications: emailNotifications,
    notificationPreferences: notificationPreferences.toDto,
    hasPushToken: hasPushToken,
  );
}
