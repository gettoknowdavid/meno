import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/auth/model/auth_context.dart';
import 'package:meno_mobile/features/settings/manager/theme_manager.dart';
import 'package:meno_mobile/features/settings/model/_model.dart';
import 'package:meno_mobile/features/settings/services/_services.dart';

/// The signed-in account's settings, and the cache behind it.
///
/// The account it answers for is injected once, as an immutable
/// [AccountContext], rather than followed from a global selector: this manager
/// lives in that account's get_it scope, so a switch builds a new manager and
/// disposes this one — a disposed manager can never begin loading the *next*
/// account's switches (M14/H15). Every change is applied locally first and
/// persisted through the API, so a switch flips the moment it is tapped; a
/// failed save is rolled back by the command's undo.
final class SettingsManager implements Disposable {
  /// Creates a manager for the context's account over its service and cache.
  new({
    required this._context,
    required this._http,
    required this._local,
    required this._theme,
    required this._logger,
  }) {
    changeLanguage = .createUndoableNoResult<String, String>(
      (language, stack) async {
        stack.push(_settings.value.language);
        _apply(_settings.value.copyWith(language: language));
        await _patch(SettingsPatchDto(language: language));
      },
      undo: (stack, _) =>
          _apply(_settings.value.copyWith(language: stack.pop())),
      errorFilterFn: menoExceptionFilter,
      debugName: 'settings.changeLanguage',
    );

    toggleDarkMode = .createUndoableNoResult<bool, UserDisplay>(
      (dark, stack) async {
        stack.push(_settings.value.display);
        final display = dark ? UserDisplay.dark : UserDisplay.light;
        _apply(_settings.value.copyWith(display: display));
        await _patch(SettingsPatchDto(display: display));
      },
      undo: (stack, _) =>
          _apply(_settings.value.copyWith(display: stack.pop())),
      errorFilterFn: menoExceptionFilter,
      debugName: 'settings.toggleDarkMode',
    );

    setAppNotifications = .createUndoableNoResult<bool, bool>(
      (enabled, stack) async {
        stack.push(_settings.value.appNotifications);
        _apply(_settings.value.copyWith(appNotifications: enabled));
        await _patch(SettingsPatchDto(appNotifications: enabled));
      },
      undo: (stack, _) =>
          _apply(_settings.value.copyWith(appNotifications: stack.pop())),
      errorFilterFn: menoExceptionFilter,
      debugName: 'settings.setAppNotifications',
    );

    setPushNotifications = .createUndoableNoResult<bool, bool>(
      (enabled, stack) async {
        stack.push(_settings.value.pushNotifications);
        _apply(_settings.value.copyWith(pushNotifications: enabled));
        await _patch(SettingsPatchDto(pushNotifications: enabled));
      },
      undo: (stack, _) =>
          _apply(_settings.value.copyWith(pushNotifications: stack.pop())),
      errorFilterFn: menoExceptionFilter,
      debugName: 'settings.setPushNotifications',
    );

    setEmailNotifications = .createUndoableNoResult<bool, bool>(
      (enabled, stack) async {
        stack.push(_settings.value.emailNotifications);
        _apply(_settings.value.copyWith(emailNotifications: enabled));
        await _patch(SettingsPatchDto(emailNotifications: enabled));
      },
      undo: (stack, _) =>
          _apply(_settings.value.copyWith(emailNotifications: stack.pop())),
      errorFilterFn: menoExceptionFilter,
      debugName: 'settings.setEmailNotifications',
    );

    setLiveBroadcastNotifications = .createUndoableNoResult<bool, bool>(
      (enabled, stack) async {
        final current = _settings.value;
        stack.push(current.notificationPreferences.liveBroadcastStarted);
        _apply(_updatedPreferences(liveBroadcastStarted: enabled));
        await _patch(_preferencesPatch());
      },
      undo: (stack, _) =>
          _apply(_updatedPreferences(liveBroadcastStarted: stack.pop())),
      errorFilterFn: menoExceptionFilter,
      debugName: 'settings.setLiveBroadcastNotifications',
    );

    setSubscribersNotifications = .createUndoableNoResult<bool, bool>(
      (enabled, stack) async {
        final current = _settings.value;
        stack.push(current.notificationPreferences.userSubscribed);
        _apply(_updatedPreferences(userSubscribed: enabled));
        await _patch(_preferencesPatch());
      },
      undo: (stack, _) =>
          _apply(_updatedPreferences(userSubscribed: stack.pop())),
      errorFilterFn: menoExceptionFilter,
      debugName: 'settings.setSubscribersNotifications',
    );

    setAddedAsCohostNotifications = .createUndoableNoResult<bool, bool>(
      (enabled, stack) async {
        final current = _settings.value;
        stack.push(current.notificationPreferences.addedAsCoHost);
        _apply(_updatedPreferences(addedAsCoHost: enabled));
        await _patch(_preferencesPatch());
      },
      undo: (stack, _) =>
          _apply(_updatedPreferences(addedAsCoHost: stack.pop())),
      errorFilterFn: menoExceptionFilter,
      debugName: 'settings.setAddedAsCohostNotifications',
    );

    setScheduledBroadcastNotifications = .createUndoableNoResult<bool, bool>(
      (enabled, stack) async {
        final current = _settings.value;
        stack.push(current.notificationPreferences.scheduledBroadcast);
        _apply(_updatedPreferences(scheduledBroadcast: enabled));
        await _patch(_preferencesPatch());
      },
      undo: (stack, _) =>
          _apply(_updatedPreferences(scheduledBroadcast: stack.pop())),
      errorFilterFn: menoExceptionFilter,
      debugName: 'settings.setScheduledBroadcastNotifications',
    );

    clearPushToken = .createAsyncNoParamNoResult(
      _clearPushToken,
      errorFilterFn: menoExceptionFilter,
      debugName: 'settings.clearPushToken',
    );
  }

  final AccountContext _context;
  final SettingsHttpService _http;
  final SettingsLocalService _local;
  final ThemeManager _theme;
  final MenoLogger _logger;

  final ValueNotifier<UserSettings> _settings = ValueNotifier(
    UserSettings.empty,
  );

  bool _disposed = false;

  /// The FIFO every settings write runs on.
  ///
  /// One request at a time, in the order the switches were tapped: the API's
  /// deep-merge is order-sensitive, and two overlapping patches used to race so
  /// that one of them was lost (audit M13). This is the only concurrency
  /// control — no revisions, no overlay, no conflict state.
  Future<void> _serial = Future<void>.value();

  /// Patches queued or in flight.
  ///
  /// A read that lands while this is above zero is stale by construction: the
  /// user has already moved past what the server sent, so the answer is dropped
  /// rather than painted over the edit.
  int _editsQueued = 0;

  /// The signed-in account's settings, or [UserSettings.empty] when there are
  /// none.
  ValueListenable<UserSettings> get settings => _settings;

  /// Changes the language tag, e.g. `en` or `en-GB`.
  late final Command<String, void> changeLanguage;

  /// Switches the theme between [UserDisplay.light] and [UserDisplay.dark].
  late final Command<bool, void> toggleDarkMode;

  /// Enables or disables in-app notifications.
  late final Command<bool, void> setAppNotifications;

  /// Enables or disables push notifications.
  late final Command<bool, void> setPushNotifications;

  /// Enables or disables email notifications.
  late final Command<bool, void> setEmailNotifications;

  /// Enables or disables "a live broadcast started" notifications.
  late final Command<bool, void> setLiveBroadcastNotifications;

  /// Enables or disables "somebody subscribed" notifications.
  late final Command<bool, void> setSubscribersNotifications;

  /// Enables or disables "you were added as a co-host" notifications.
  late final Command<bool, void> setAddedAsCohostNotifications;

  /// Enables or disables "a scheduled broadcast is starting" notifications.
  late final Command<bool, void> setScheduledBroadcastNotifications;

  /// Unsets the device token on the server.
  late final Command<void, void> clearPushToken;

  /// Installs this account's cached settings.
  ///
  /// The local half is synchronous, so the registrar can await it before the
  /// owner publishes `ready`; no remote read starts here — the first refresh
  /// begins only after the account is ready, from the account subtree's first
  /// widget (Step 18/19).
  Future<SettingsManager> init() async {
    _settings.addListener(_applyDisplay);
    final userId = _context.accountId;
    _adopt(userId);
    _applyDisplay();
    return this;
  }

  /// Starts the first background read after the account is ready.
  ///
  /// This is the door the account subtree opens once `ready`; it is the same
  /// read a retry or a pull-to-refresh asks for, and its failure is logged
  /// rather than raised because nobody asked for it.
  Future<void> refresh() => _load(_context.accountId);

  @override
  FutureOr<dynamic> onDispose() {
    _disposed = true;
    changeLanguage.dispose();
    toggleDarkMode.dispose();
    setAppNotifications.dispose();
    setPushNotifications.dispose();
    setEmailNotifications.dispose();
    setLiveBroadcastNotifications.dispose();
    setSubscribersNotifications.dispose();
    setAddedAsCohostNotifications.dispose();
    setScheduledBroadcastNotifications.dispose();
    clearPushToken.dispose();

    _settings.dispose();
  }

  /// The account these settings belong to.
  Id get _userId => _context.accountId;

  /// Reads the settings for [userId] and saves them for next time.
  ///
  /// The account can change while the request is in flight; the answer then
  /// belongs to the account that asked, so it is dropped rather than shown
  /// under the wrong account.
  Future<void> _load(Id userId) async {
    try {
      final remote = await _http.getSettings();
      // The manager can be torn down while the request is in flight; the
      // answer then belongs to a moment that has passed.
      if (_disposed) return;

      // An edit is queued or on its way, so this answer predates it. Adopting
      // it would put the old value back on screen until the patch answered.
      if (_editsQueued > 0) return;

      _settings.value = remote;
      await _local.saveSettings(remote);
    } on MenoException catch (error) {
      _logger.w(
        'Could not load settings (${error.code}); showing the cached copy',
        tag: MenoLogTag.lifecycle.label,
      );
    }
  }

  /// Unsets the device token, then reflects that locally.
  Future<void> _clearPushToken() async {
    if (_disposed || _userId.isEmpty) return;
    await _http.clearPushToken();
    // The account may have gone while the call was out; its settings object is
    // not this manager's to touch any more.
    if (_disposed) return;
    _apply(_settings.value.copyWith(hasPushToken: false));
  }

  /// Publishes [next] and writes it to the device cache.
  void _apply(UserSettings next) {
    _settings.value = next;
    _persistLocally();
  }

  /// Replaces one flag on [settings]' notification preferences.
  UserSettings _updatedPreferences({
    bool? liveBroadcastStarted,
    bool? userSubscribed,
    bool? addedAsCoHost,
    bool? scheduledBroadcast,
  }) {
    final current = _settings.value;
    final preferences = current.notificationPreferences;
    return current.copyWith(
      notificationPreferences: preferences.copyWith(
        liveBroadcastStarted:
            liveBroadcastStarted ?? preferences.liveBroadcastStarted,
        userSubscribed: userSubscribed ?? preferences.userSubscribed,
        addedAsCoHost: addedAsCoHost ?? preferences.addedAsCoHost,
        scheduledBroadcast:
            scheduledBroadcast ?? preferences.scheduledBroadcast,
      ),
    );
  }

  /// Queues [patch] behind every write already in flight.
  ///
  /// Only the fields the user moved are in [patch], so an unknown preference
  /// key is never sent and therefore never deleted. The API's answer is adopted
  /// as canonical, which settles the optimistic value the command already
  /// painted; a rejection propagates, and the command's undo reverts that one
  /// field.
  ///
  /// A guest has no row to write, so the optimistic change stays local — which
  /// is also why [_persistLocally] skips them: device storage is keyed by user.
  Future<void> _patch(SettingsPatchDto patch) {
    if (_disposed || _userId.isEmpty) return Future<void>.value();
    return _enqueue(patch);
  }

  /// Appends [patch] to the FIFO and waits for its turn.
  Future<void> _enqueue(SettingsPatchDto patch) async {
    _editsQueued += 1;

    final result = _serial.then((_) async {
      // Queued work for an account that is gone sends nothing (audit H15).
      if (_disposed) return;

      final remote = await _http.updateSettings(patch);
      if (_disposed) return;

      _settings.value = remote;
      _persistLocally();
    });

    // The chain must survive a rejected patch, or one failure would stop every
    // later write from being sent.
    _serial = result.then((_) {}, onError: (Object _) {});

    try {
      await result;
    } finally {
      _editsQueued -= 1;
    }
  }

  /// A patch carrying the notification preferences as they now stand.
  SettingsPatchDto _preferencesPatch() => SettingsPatchDto(
    notificationPreferences: _settings.value.notificationPreferences.toDto,
  );

  /// Writes the current settings to the device cache. No-op for guests.
  void _persistLocally() {
    if (_userId.isEmpty) return;
    unawaited(_local.saveSettings(_settings.value));
  }

  /// Repaints the app in whatever theme the current account's settings ask for.
  ///
  /// Called on every settings change rather than only on the toggle, so a
  /// sign-in and an account switch repaint too — that is what puts a returning
  /// account's own choice back on screen.
  ///
  /// It deliberately paints nothing when nobody is signed in. Appearance is a
  /// device preference on both platforms — Android recommends "follow system"
  /// as the default with Light and Dark as overrides, and the choice is kept
  /// per install — and the app keeps it that way: signing out leaves the app in
  /// the theme the user was already looking at rather than flashing the system
  /// one back over the sign-in screen. A device that has never signed in is
  /// following the system by [ThemeManager]'s own default, and the next account
  /// to sign in repaints from its own cached settings.

  void _applyDisplay() {
    if (_userId.isEmpty) return;
    _theme.apply(_settings.value.display);
  }

  /// Puts this account's cached settings in place.
  void _adopt(Id userId) {
    if (userId.isEmpty) {
      _settings.value = UserSettings.empty;
      return;
    }
    _settings.value = _local.getSettings() ?? UserSettings.empty;
  }
}
