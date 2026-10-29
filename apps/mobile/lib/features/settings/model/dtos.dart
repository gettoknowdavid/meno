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

/// A `PATCH /settings` body carrying only what changed.
///
/// # Why not the whole record
///
/// The API deep-merges what it is sent, so a field that is absent is untouched.
/// Sending the whole preferences object (or the whole settings record) means
/// every key this build does not know about is deleted — a newer client's
/// preference, removed by an older one (audit M13). A patch built field by
/// field cannot do that: a null field is simply not in the JSON, because
/// `includeIfNull: false` drops it.
@freezed
abstract class SettingsPatchDto with _$SettingsPatchDto {
  /// One changed setting, or several changed together.
  const factory({
    @JsonKey(name: 'push_notifications', includeIfNull: false)
    bool? pushNotifications,
    @JsonKey(name: 'app_notifications', includeIfNull: false)
    bool? appNotifications,
    @JsonKey(name: 'email_notifications', includeIfNull: false)
    bool? emailNotifications,
    @JsonKey(includeIfNull: false) UserDisplay? display,
    @JsonKey(includeIfNull: false) String? language,
    @JsonKey(name: 'notification_preferences', includeIfNull: false)
    NotificationPreferencesDto? notificationPreferences,
    @JsonKey(name: 'push_notification_token', includeIfNull: false)
    String? pushNotificationToken,
  }) = _SettingsPatchDto;

  const new _();

  /// Reads a patch back, for symmetry and for tests that assert its shape.
  factory fromJson(Map<String, dynamic> json) =>
      _$SettingsPatchDtoFromJson(json);

  /// Whether this patch would send nothing, which the API refuses.
  bool get isEmpty => toJson().isEmpty;
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
