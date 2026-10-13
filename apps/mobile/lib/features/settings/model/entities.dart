import 'package:freezed_annotation/freezed_annotation.dart';

part 'entities.freezed.dart';

/// The UI theme, mirroring the API's `Display` enum.
///
/// The wire spelling is lowercase (`system`, `light`, `dark`), which is also
/// the enum value's name, so it needs no `@JsonValue`. [fromWire] is lenient
/// like the server's: an unrecognised value reads as [system] rather than
/// failing.
enum UserDisplay {
  /// Follow the operating system.
  system('system'),

  /// Always light.
  light('light'),

  /// Always dark.
  dark('dark');

  /// Creates a display value from its wire spelling.
  new(this.wire);

  /// The exact string the API stores and sends.
  final String wire;

  /// The display [wire] names, defaulting to [system].
  static UserDisplay fromWire(String wire) {
    for (final value in UserDisplay.values) {
      if (value.wire == wire) return value;
    }
    return UserDisplay.system;
  }
}

/// The broadcast/account notification preferences.
///
/// The API stores `notification_preferences` verbatim as free-form JSON and has
/// no opinion about its shape; the app owns it. These four flags are the
/// toggles the notifications screen exposes.
@freezed
abstract class NotificationPreferences with _$NotificationPreferences {
  /// Creates the notification preferences.
  const factory({
    @Default(false) bool liveBroadcastStarted,
    @Default(false) bool userSubscribed,
    @Default(false) bool addedAsCoHost,
    @Default(false) bool scheduledBroadcast,
  }) = _NotificationPreferences;

  const new _();
}

/// The signed-in account's settings.
///
/// The defaults match `Settings::new` in
/// `apps/api/src/modules/settings/model.rs`: a missing row is a set of defaults,
/// not a 404, so what the app shows before the first save is what the API would
/// answer anyway.
@freezed
abstract class UserSettings with _$UserSettings {
  /// Creates the account's settings.
  const factory({
    @Default(UserDisplay.system) UserDisplay display,
    @Default('en') String language,
    @Default(true) bool appNotifications,
    @Default(true) bool pushNotifications,
    @Default(true) bool emailNotifications,
    @Default(NotificationPreferences())
    NotificationPreferences notificationPreferences,
    @Default(false) bool hasPushToken,
  }) = _UserSettings;

  const new _();

  /// The defaults for an account that has never saved any settings.
  static const UserSettings empty = UserSettings();
}
