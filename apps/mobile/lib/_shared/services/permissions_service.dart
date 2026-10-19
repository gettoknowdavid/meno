import 'dart:async';
import 'dart:io';

import 'package:flutter/widgets.dart';
import 'package:flutter_background/flutter_background.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/local_storage.dart';
import 'package:permission_handler/permission_handler.dart';

/// Tracks and requests the app's runtime permissions.
///
/// A permission belongs to the device, not to an account, so this service lives
/// in the root scope: switching accounts neither re-asks nor forgets. The
/// platform is the source of truth, and the notifiers below are a cache of it
/// that is refreshed whenever the app comes back to the foreground — which is
/// also how a change made in Settings reaches the screens.
///
/// Permissions are asked for when a feature needs them, never at startup:
///
/// * the microphone, on the host's "Start Broadcast" and on unmuting;
/// * notifications, on an explicit switch in settings, or after the user
///   subscribes to somebody (their first reason to want one);
/// * the camera and the photo library are *not* here. `image_picker` uses the
///   system camera and the system photo picker, which need no permission, and
///   `MediaService` reports the one case where the platform refuses.
///
/// How a request talks to the user is up to the screen: it hands in a
/// [PermissionContext] and the service decides *when* each callback is needed.
final class PermissionsService implements Disposable {
  /// Creates a service over [_logger] and the device's [_storage].
  new(this._storage, this._logger) {
    _lifecycle = AppLifecycleListener(onResume: _onResume);
    checkPermissions = .createAsyncNoParamNoResult(
      _checkAllPermissions,
      errorFilterFn: menoExceptionFilter,
      debugName: 'permissions.check',
    );
    requestMicrophone = .createAsyncNoResult(
      (context) => _request(_microphoneSpec, context),
      errorFilterFn: menoExceptionFilter,
      debugName: 'permissions.microphone',
    );
    requestNotification = .createAsyncNoResult(
      (context) => _request(_notificationSpec, context),
      errorFilterFn: menoExceptionFilter,
      debugName: 'permissions.notifications',
    );
    requestBackground = .createAsyncNoParamNoResult(
      _requestBackground,
      errorFilterFn: menoExceptionFilter,
      debugName: 'permissions.background',
    );
    requestBroadcast = .createAsyncNoResult(
      _requestBroadcast,
      errorFilterFn: menoExceptionFilter,
      debugName: 'permissions.broadcast',
    );
    disableBackground = .createAsyncNoParam<bool>(
      _disableBackground,
      initialValue: false,
      errorFilterFn: menoExceptionFilter,
      debugName: 'permissions.disableBackground',
    );
  }

  final LocalStorage _storage;
  final MenoLogger _logger;

  late final AppLifecycleListener _lifecycle;

  /// How long "not now" to an explanation is respected.
  static const Duration _primerCooldown = Duration(days: 14);

  /// Ask, explain and re-ask, send to Settings: the longest a request goes.
  static const int _maxAttempts = 3;

  /// The microphone permission's last known state.
  final ValueNotifier<PermissionState> microphone = ValueNotifier(
    const .unknown(),
  );

  /// The notification permission's last known state.
  final ValueNotifier<PermissionState> notifications = ValueNotifier(
    const .unknown(),
  );

  /// The background-execution permission's last known state (Android only).
  final ValueNotifier<PermissionState> background = ValueNotifier(
    const .unknown(),
  );

  late final _Spec _microphoneSpec = _Spec(
    PermissionType.microphone,
    Permission.microphone,
    microphone,
    _microphoneCopy,
  );

  late final _Spec _notificationSpec = _Spec(
    PermissionType.notification,
    Permission.notification,
    notifications,
    _notificationCopy,
  );

  /// Refreshes every tracked permission.
  late final Command<void, void> checkPermissions;

  /// Requests the microphone.
  late final Command<PermissionContext, void> requestMicrophone;

  /// Requests notifications.
  late final Command<PermissionContext, void> requestNotification;

  /// Requests background execution (Android only).
  late final Command<void, void> requestBackground;

  /// Requests what going on air needs.
  ///
  /// The microphone is the gate, and the only one: without it the broadcast
  /// would be silent. Background execution (Android) is asked for after it so
  /// the broadcast survives a locked screen, but refusing it does not stop the
  /// broadcast. Notifications are not part of this: they are not needed to
  /// broadcast, and a third system dialog in a row is how permissions get
  /// denied.
  late final Command<PermissionContext, void> requestBroadcast;

  /// Turns background execution off.
  late final Command<void, bool> disableBackground;

  /// Whether a broadcast could start right now.
  bool get canBroadcast =>
      microphone.value is PermissionGranted &&
      (!Platform.isAndroid || background.value is PermissionGranted);

  /// Whether the microphone is granted right now, read from the platform.
  ///
  /// For code that must not prompt — joining a room — and only needs to know
  /// whether it may publish. A failed read answers false.
  Future<bool> hasMicrophone() async {
    try {
      final status = await Permission.microphone.status;
      _set(microphone, status);
      return status.isGranted;
    } on Exception catch (error) {
      _logger.w(
        'Could not read the microphone permission',
        tag: MenoLogTag.permissions.label,
        error: error,
      );
      return false;
    }
  }

  /// Opens the app's page in the system settings.
  Future<bool> openSettings() => openAppSettings();

  /// Refreshes every permission at once.
  Future<void> _checkAllPermissions() async {
    await Future.wait([
      _check(_microphoneSpec),
      _check(_notificationSpec),
      if (Platform.isAndroid) _checkBackground(),
    ]);
  }

  /// Re-reads what Settings may have changed while the app was away.
  void _onResume() {
    if (!checkPermissions.isRunning.value) checkPermissions.run();
  }

  Future<void> _check(_Spec spec) async {
    try {
      _set(spec.notifier, await spec.permission.status);
    } on Exception catch (error) {
      _fail(spec.notifier, 'checking ${spec.type.displayName}', error);
    }
  }

  Future<void> _checkBackground() async {
    try {
      background.value = await FlutterBackground.hasPermissions
          ? const .granted()
          : const .denied();
    } on Exception catch (error) {
      _fail(background, 'checking background permission', error);
    }
  }

  /// Gets [spec]'s permission granted, as far as the user allows.
  ///
  /// One state machine for every permission, driven by what the platform says
  /// and by which callbacks [context] supplies:
  ///
  /// * granted — nothing to do, and nothing shown;
  /// * never asked — the system prompt, preceded by [PermissionContext
  ///   .showPrimer] when the screen wants to explain first (a "not now" there
  ///   is remembered, and the question is not repeated for a while);
  /// * refused once — [PermissionContext.showRationale] decides whether to ask
  ///   again, which is only useful where the platform will show its prompt a
  ///   second time (Android);
  /// * refused for good — [PermissionContext.showSettingsPrompt] decides
  ///   whether to send the user to Settings, the only place left to change it.
  ///
  /// A callback the screen did not supply is a step it did not want: no
  /// primer means ask straight away, and no rationale or settings prompt means
  /// give up quietly.
  Future<void> _request(_Spec spec, PermissionContext context) async {
    try {
      for (var attempt = 0; attempt < _maxAttempts; attempt++) {
        final status = await spec.permission.status;
        _set(spec.notifier, status);

        if (status.isGranted ||
            status.isLimited ||
            status.isProvisional ||
            status.isRestricted) {
          return;
        }

        if (status.isPermanentlyDenied) {
          final open = await context.showSettingsPrompt?.call(spec.copy);
          if (open ?? false) await openAppSettings();
          return;
        }

        final isReAsk =
            attempt > 0 || await spec.permission.shouldShowRequestRationale;
        if (isReAsk) {
          final retry = await context.showRationale?.call(spec.copy);
          if (!(retry ?? false)) return;
        } else if (context.showPrimer case final primer?) {
          if (_primerSnoozed(spec.type)) return;
          if (!await primer(spec.copy)) {
            await _snoozePrimer(spec.type);
            return;
          }
        }

        spec.notifier.value = const .requesting();
        _set(spec.notifier, await spec.permission.request());
      }
    } on Exception catch (error) {
      _fail(spec.notifier, 'requesting ${spec.type.displayName}', error);
    }
  }

  Future<void> _requestBackground() async {
    if (!Platform.isAndroid) {
      background.value = const .granted();
      return;
    }

    background.value = const .requesting();
    try {
      var granted = await FlutterBackground.hasPermissions;
      if (!granted) {
        granted = await FlutterBackground.initialize(
          androidConfig: const FlutterBackgroundAndroidConfig(
            notificationTitle: 'Menō Broadcasting',
            notificationText: 'Broadcast is running in background',
            notificationImportance: .high,
            notificationIcon: AndroidResource(name: 'ic_stat_ic_notification'),
          ),
        );
      }

      // Best effort: the system dialog was the explanation, and a broadcast
      // that cannot outlive a locked screen is still a broadcast.
      background.value = granted ? const .granted() : const .denied();
    } on Exception catch (error) {
      _fail(background, 'requesting background permission', error);
    }
  }

  Future<void> _requestBroadcast(PermissionContext context) async {
    await _request(_microphoneSpec, context);

    if (microphone.value is! PermissionGranted) {
      _logger.w(
        'Broadcast permissions abandoned: the microphone was not granted',
        tag: MenoLogTag.permissions.label,
      );
      return;
    }

    if (Platform.isAndroid && background.value is! PermissionGranted) {
      await _requestBackground();
    }
  }

  Future<bool> _disableBackground() async {
    if (!Platform.isAndroid) return true;
    try {
      return await FlutterBackground.disableBackgroundExecution();
    } on Exception catch (error) {
      _logger.e(
        'Could not disable background execution',
        tag: MenoLogTag.permissions.label,
        error: error,
      );
      return false;
    }
  }

  /// Whether the user recently declined [type]'s explanation.
  bool _primerSnoozed(PermissionType type) {
    final declinedAt = _storage.getInt(
      StorageKeys.permissionPrimerDeclined(type.name),
    );
    if (declinedAt == null) return false;

    final since = DateTime.now().difference(
      DateTime.fromMillisecondsSinceEpoch(declinedAt),
    );
    return since < _primerCooldown;
  }

  Future<void> _snoozePrimer(PermissionType type) async {
    try {
      await _storage.setInt(
        StorageKeys.permissionPrimerDeclined(type.name),
        value: DateTime.now().millisecondsSinceEpoch,
      );
    } on StorageException catch (error) {
      // The worst case is being asked again sooner than intended.
      _logger.w(
        'Could not remember declining ${type.displayName}',
        tag: MenoLogTag.permissions.label,
        error: error,
      );
    }
  }

  /// Publishes [status] as [notifier]'s new state.
  void _set(ValueNotifier<PermissionState> notifier, PermissionStatus status) {
    final next = PermissionState.fromStatus(status);
    if (notifier.value.runtimeType == next.runtimeType) return;

    notifier.value = next;
    _logger.d(
      '${_name(notifier)} is now ${next.runtimeType}',
      tag: MenoLogTag.permissions.label,
    );
  }

  /// Records a failed platform call, surfacing it as a [Command] error.
  void _fail(
    ValueNotifier<PermissionState> notifier,
    String action,
    Object error,
  ) {
    _logger.e('Error $action', tag: MenoLogTag.permissions.label, error: error);
    notifier.value = .error(error.toString());
    throw UnknownException(cause: error);
  }

  /// A short name for [notifier], for logs.
  String _name(ValueNotifier<PermissionState> notifier) {
    if (identical(notifier, microphone)) return 'Microphone';
    if (identical(notifier, notifications)) return 'Notifications';
    return 'Background execution';
  }

  @override
  Future<void> onDispose() async {
    _lifecycle.dispose();

    microphone.dispose();
    notifications.dispose();
    background.dispose();

    checkPermissions.dispose();
    requestMicrophone.dispose();
    requestNotification.dispose();
    requestBackground.dispose();
    requestBroadcast.dispose();
    disableBackground.dispose();
  }
}

/// One permission, with everything [PermissionsService._request] needs.
final class _Spec {
  const new(this.type, this.permission, this.notifier, this.copy);

  final PermissionType type;
  final Permission permission;
  final ValueNotifier<PermissionState> notifier;
  final PermissionRationale copy;
}

const PermissionRationale _microphoneCopy = PermissionRationale(
  permission: PermissionType.microphone,
  title: 'Microphone access needed',
  message:
      'Menō needs your microphone so listeners can hear you. You can mute '
      'yourself at any time during a broadcast.',
  settingsMessage:
      'Microphone access is turned off for Menō. Turn it on in Settings to '
      'go live.',
);

const PermissionRationale _notificationCopy = PermissionRationale(
  permission: PermissionType.notification,
  title: 'Know when they go live',
  message:
      'Turn on notifications to hear when the people you subscribe to go live.',
  settingsMessage:
      'Notifications are turned off for Menō. Turn them on in Settings to '
      'hear when broadcasts go live.',
);

/// A permission's state, as far as the app knows it.
sealed class PermissionState {
  const new();

  /// Nobody has asked the platform yet.
  const factory unknown() = PermissionUnknown;

  /// The platform's prompt is up.
  const factory requesting() = PermissionRequesting;

  /// The user allowed it.
  const factory granted() = PermissionGranted;

  /// The user refused this time.
  const factory denied() = PermissionDenied;

  /// The user refused and the platform will not ask again.
  const factory permanentlyDenied() = PermissionPermanentlyDenied;

  /// The platform forbids it, and the user cannot change that.
  const factory restricted() = PermissionRestricted;

  /// The platform call failed; [message] is the detail.
  const factory error(String message) = PermissionError;

  /// Reads a platform status.
  factory fromStatus(PermissionStatus status) => switch (status) {
    PermissionStatus.granted ||
    PermissionStatus.limited ||
    PermissionStatus.provisional => const .granted(),
    PermissionStatus.denied => const .denied(),
    PermissionStatus.permanentlyDenied => const .permanentlyDenied(),
    PermissionStatus.restricted => const .restricted(),
  };
}

/// The permission has not been asked for yet.
final class PermissionUnknown extends PermissionState {
  const new();
}

/// The platform's prompt is on screen.
final class PermissionRequesting extends PermissionState {
  const new();
}

/// The permission was granted.
final class PermissionGranted extends PermissionState {
  const new();
}

/// The permission was refused for now.
final class PermissionDenied extends PermissionState {
  const new();
}

/// The permission was refused permanently.
final class PermissionPermanentlyDenied extends PermissionState {
  const new();
}

/// The platform forbids the permission.
final class PermissionRestricted extends PermissionState {
  const new();
}

/// The platform call failed.
final class PermissionError extends PermissionState {
  const new(this.message);

  /// What the platform reported.
  final String message;
}

/// The permissions the app asks for.
enum PermissionType {
  /// Recording audio.
  microphone,

  /// Showing notifications.
  notification,

  /// Running while the app is backgrounded.
  background,
}

extension PermissionTypeX on PermissionType {
  /// A short, human-readable name.
  String get displayName => switch (this) {
    PermissionType.microphone => 'Microphone',
    PermissionType.notification => 'Notifications',
    PermissionType.background => 'Background Processing',
  };
}

/// Asks the user something about [PermissionRationale] and answers whether
/// they said yes.
typedef PermissionPrompt = Future<bool> Function(PermissionRationale copy);

/// How a request may talk to the user.
///
/// Keeps the wording with the screen that owns it: the service knows when a
/// step is needed and the screen knows how to say it. A step the screen leaves
/// out is skipped, so [silent] asks the platform and never interrupts.
@immutable
final class PermissionContext {
  const new({this.showPrimer, this.showRationale, this.showSettingsPrompt});

  /// Asks nothing of the user beyond the system's own prompt.
  static const PermissionContext silent = PermissionContext();

  /// Explains the ask before the platform's prompt, the first time only.
  ///
  /// Answering no spends nothing — the platform's one-shot prompt stays unused
  /// — and the question is not repeated for a while. Absent: ask straight away.
  final PermissionPrompt? showPrimer;

  /// Explains why, after a refusal, and answers whether to ask again.
  ///
  /// Absent: do not ask again.
  final PermissionPrompt? showRationale;

  /// Offers the app's settings, once only they can help, and answers whether
  /// the user accepted. Absent: say nothing.
  final PermissionPrompt? showSettingsPrompt;
}

/// The copy a permission prompt shows.
@immutable
final class PermissionRationale {
  const new({
    required this.permission,
    required this.title,
    required this.message,
    required this.settingsMessage,
  });

  /// The permission being explained.
  final PermissionType permission;

  /// The prompt's heading.
  final String title;

  /// Why the permission is wanted.
  final String message;

  /// What to say when only the system settings can grant it.
  final String settingsMessage;
}
