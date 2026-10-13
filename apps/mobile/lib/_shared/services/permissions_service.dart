import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_background/flutter_background.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:permission_handler/permission_handler.dart';

/// Tracks and requests the app's runtime permissions.
///
/// Each permission's state is published through a [ValueNotifier] and every
/// request is a [Command], so a screen can watch a permission and drive it
/// without owning any of the platform detail. Only the microphone matters until
/// somebody broadcasts: permissions are asked for when a feature needs them,
/// never at startup.
final class PermissionsService implements Disposable {
  /// Creates a service over [_logger].
  new(this._logger);

  final MenoLogger _logger;

  /// The microphone permission's last known state.
  final ValueNotifier<PermissionState> microphone = ValueNotifier(
    const PermissionState.unknown(),
  );

  /// The notification permission's last known state.
  final ValueNotifier<PermissionState> notifications = ValueNotifier(
    const PermissionState.unknown(),
  );

  /// The background-execution permission's last known state (Android only).
  final ValueNotifier<PermissionState> background = ValueNotifier(
    const PermissionState.unknown(),
  );

  /// Refreshes every tracked permission.
  late final Command<void, void> checkPermissions =
      Command.createAsyncNoParamNoResult(
        _checkAllPermissions,
        errorFilterFn: menoExceptionFilter,
        debugName: 'permissions.check',
      );

  /// Requests the microphone, explaining a refusal when [PermissionContext]
  /// supplies the copy.
  late final Command<PermissionContext, void> requestMicrophone =
      Command.createAsyncNoResult(
        _requestMicrophone,
        errorFilterFn: menoExceptionFilter,
        debugName: 'permissions.microphone',
      );

  /// Requests notifications, explaining a refusal when one is supplied.
  late final Command<PermissionContext, void> requestNotification =
      Command.createAsyncNoResult(
        _requestNotification,
        errorFilterFn: menoExceptionFilter,
        debugName: 'permissions.notifications',
      );

  /// Requests background execution (Android only).
  late final Command<PermissionContext, void> requestBackground =
      Command.createAsyncNoResult(
        _requestBackground,
        errorFilterFn: menoExceptionFilter,
        debugName: 'permissions.background',
      );

  /// Requests everything a broadcast needs, in the order it needs it.
  ///
  /// The microphone is the gate: without it the rest is moot, so the flow stops
  /// rather than piling up prompts the user cannot act on.
  late final Command<PermissionContext, void> requestBroadcast =
      Command.createAsyncNoResult(
        _requestBroadcast,
        errorFilterFn: menoExceptionFilter,
        debugName: 'permissions.broadcast',
      );

  /// Turns background execution off.
  late final Command<void, bool> disableBackground =
      Command.createAsyncNoParam<bool>(
        _disableBackground,
        initialValue: false,
        errorFilterFn: menoExceptionFilter,
        debugName: 'permissions.disableBackground',
      );

  /// Whether a broadcast could start right now.
  bool get canBroadcast =>
      microphone.value is PermissionGranted &&
      (!Platform.isAndroid || background.value is PermissionGranted);

  /// Whether the platform wants an explanation before asking for [type] again.
  Future<bool> shouldShowRationale(PermissionType type) => switch (type) {
    .microphone => Permission.microphone.shouldShowRequestRationale,
    .notification => Permission.notification.shouldShowRequestRationale,
    .background => Future.value(false),
  };

  /// Refreshes every permission at once.
  Future<void> _checkAllPermissions() async {
    await Future.wait([
      _checkMicrophone(),
      _checkNotification(),
      if (Platform.isAndroid) _checkBackground(),
    ]);
  }

  Future<void> _checkMicrophone() async {
    try {
      _set(microphone, await Permission.microphone.status);
    } on Exception catch (error) {
      _fail(microphone, 'checking microphone permission', error);
    }
  }

  Future<void> _checkNotification() async {
    try {
      _set(notifications, await Permission.notification.status);
    } on Exception catch (error) {
      _fail(notifications, 'checking notification permission', error);
    }
  }

  Future<void> _checkBackground() async {
    try {
      background.value = await FlutterBackground.hasPermissions
          ? const PermissionState.granted()
          : const PermissionState.denied();
    } on Exception catch (error) {
      _fail(background, 'checking background permission', error);
    }
  }

  Future<void> _requestMicrophone(PermissionContext context) async {
    microphone.value = const PermissionState.requesting();
    try {
      _set(microphone, await Permission.microphone.request());
      switch (microphone.value) {
        case PermissionDenied():
          await context.showRationale?.call(
            const PermissionRationale(
              title: 'Microphone access required',
              message:
                  'Microphone access is essential for broadcasting your voice '
                  'to listeners. Without it, you cannot start a broadcast.',
              permission: PermissionType.microphone,
            ),
          );
        case PermissionPermanentlyDenied():
          final shouldOpen = await context.showSettingsPrompt?.call(
            const PermissionRationale(
              title: 'Microphone permission required',
              message:
                  'Microphone permission is permanently denied. Enable it in '
                  'Settings to start broadcasting.',
              permission: PermissionType.microphone,
            ),
          );
          if (shouldOpen ?? false) {
            await openAppSettings();
            await _checkMicrophone();
          }
        case PermissionGranted():
        case PermissionRestricted():
        case PermissionUnknown():
        case PermissionRequesting():
        case PermissionError():
          break;
      }
    } on Exception catch (error) {
      _fail(microphone, 'requesting microphone permission', error);
    }
  }

  Future<void> _requestNotification(PermissionContext context) async {
    notifications.value = const PermissionState.requesting();
    try {
      _set(notifications, await Permission.notification.request());
      if (notifications.value is! PermissionPermanentlyDenied) return;

      final shouldOpen = await context.showSettingsPrompt?.call(
        const PermissionRationale(
          title: 'Notification permission',
          message:
              'Notifications keep you updated about your broadcasts and '
              'interactions. Enable them in Settings.',
          permission: PermissionType.notification,
        ),
      );
      if (shouldOpen ?? false) {
        await openAppSettings();
        await _checkNotification();
      }
    } on Exception catch (error) {
      _fail(notifications, 'requesting notification permission', error);
    }
  }

  Future<void> _requestBackground(PermissionContext context) async {
    if (!Platform.isAndroid) {
      background.value = const PermissionState.granted();
      return;
    }

    background.value = const PermissionState.requesting();
    try {
      var granted = await FlutterBackground.hasPermissions;
      if (!granted) {
        granted = await FlutterBackground.initialize(
          androidConfig: const FlutterBackgroundAndroidConfig(
            notificationTitle: 'Menō Broadcasting',
            notificationText: 'Broadcast is running in background',
            notificationImportance: AndroidNotificationImportance.high,
            notificationIcon: AndroidResource(name: 'ic_stat_ic_notification'),
          ),
        );
      }

      background.value = granted
          ? const PermissionState.granted()
          : const PermissionState.denied();

      if (!granted) {
        await context.showRationale?.call(
          const PermissionRationale(
            title: 'Background processing',
            message:
                'Background processing lets your broadcast continue while you '
                'switch apps or lock your screen.',
            permission: PermissionType.background,
          ),
        );
      }
    } on Exception catch (error) {
      _fail(background, 'requesting background permission', error);
    }
  }

  /// Requests the broadcast set in the order the platform expects.
  ///
  /// Each request publishes its own result, so there is no re-read at the end:
  /// the notifiers already hold the answer.
  Future<void> _requestBroadcast(PermissionContext context) async {
    await _checkAllPermissions();

    if (microphone.value is! PermissionGranted) {
      await _requestMicrophone(context);
    }
    if (microphone.value is! PermissionGranted) {
      _logger.w(
        'Broadcast permissions abandoned: the microphone was not granted',
        tag: MenoLogTag.permissions.label,
      );
      return;
    }

    if (notifications.value is! PermissionGranted) {
      await _requestNotification(context);
    }
    if (Platform.isAndroid && background.value is! PermissionGranted) {
      await _requestBackground(context);
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

  /// Publishes [status] as [notifier]'s new state.
  void _set(ValueNotifier<PermissionState> notifier, PermissionStatus status) {
    notifier.value = PermissionState.fromStatus(status);
    _logger.d(
      '${_name(notifier)} is now ${notifier.value.runtimeType}',
      tag: MenoLogTag.permissions.label,
    );
  }

  /// Records a failed platform call, surfacing it as a [Command] error.
  void _fail(
    ValueNotifier<PermissionState> notifier,
    String action,
    Object error,
  ) {
    _logger.e(
      'Error $action',
      tag: MenoLogTag.permissions.label,
      error: error,
    );
    notifier.value = PermissionState.error(error.toString());
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
    PermissionStatus.provisional => const PermissionState.granted(),
    PermissionStatus.denied => const PermissionState.denied(),
    PermissionStatus.permanentlyDenied =>
      const PermissionState.permanentlyDenied(),
    PermissionStatus.restricted => const PermissionState.restricted(),
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

/// What a request should say when it needs to explain itself.
///
/// Keeps the copy with the screen that owns it: the service knows when to ask
/// and the screen knows how to say it.
@immutable
final class PermissionContext {
  const new({this.showRationale, this.showSettingsPrompt});

  /// Explains why the permission is wanted before the platform asks again.
  final Future<void> Function(PermissionRationale)? showRationale;

  /// Offers to open the app's settings. Answers whether the user accepted.
  final Future<bool> Function(PermissionRationale)? showSettingsPrompt;
}

/// The copy a rationale prompt shows.
@immutable
final class PermissionRationale {
  const new({
    required this.title,
    required this.message,
    required this.permission,
  });

  /// The prompt's heading.
  final String title;

  /// The prompt's body.
  final String message;

  /// The permission being explained.
  final PermissionType permission;
}
