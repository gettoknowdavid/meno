import 'dart:async';

import 'package:flutter/foundation.dart'
    show
        TargetPlatform,
        ValueListenable,
        ValueNotifier,
        defaultTargetPlatform,
        kIsWeb;
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';
import 'package:meno_mobile/features/broadcast/widgets/broadcast_mini_stage.dart';
import 'package:pip_kit/pip_kit.dart';

/// What the live manager needs from a floating window.
///
/// An interface so the manager can be driven in a test with no platform
/// channel, and so swapping the package (or the mechanism) touches one file.
/// Every method is best effort: a window that will not open is a cosmetic
/// loss, never a broadcast that fails.
abstract interface class LivePip {
  /// Prepares the window for a room that just went live.
  ///
  /// Idempotent: the manager arms on every transition back to live, including a
  /// reconnect, and only the first one does anything.
  Future<void> arm({
    required Broadcast broadcast,
    required bool canPublish,
    required bool isMicrophoneEnabled,
    required ValueListenable<String> elapsed,
  });

  /// Forgets the window, closing it if it is open.
  Future<void> disarm();

  /// Re-renders the muted state after somebody changed the microphone.
  Future<void> updateMicrophone(bool enabled);

  /// Releases the controller's own resources. The manager calls this once, when
  /// the account scope goes away.
  void dispose();
}

/// The Android Picture-in-Picture window for a live broadcast.
///
/// # One controller, one room
///
/// The room already has one owner (the live manager); the window is a property
/// of that room, not an app-wide service, so it is attached to the manager
/// rather than registered in the container. Arming is what makes "leaving the
/// app shows the broadcast" true, and disarming is what makes it false again —
/// every screen outside a live room must leave the app and see nothing.
///
/// # Android only
///
/// iOS gives a PiP window to a video layer, not to an audio room: there is
/// nothing to hand it. The platform check keeps the controller from arming
/// there (and from touching the plugin in a widget test on a desktop host).
///
/// # Best effort
///
/// The package is young, and OEM builds disable PiP. Every call is wrapped:
/// the failure is logged with the rest of the lifecycle's, and the room stays
/// on air with its foreground-service notification as the fallback surface.
final class LivePipController implements LivePip {
  /// Creates the controller over a [PipController] and this room's callbacks.
  new({
    required this._pip,
    required this._logger,
    required this._snapshot,
    required this._accountId,
    required this._onExit,
    required this._onMicrophoneChanged,
    bool Function()? isAndroid,
  }) : _isAndroid = isAndroid ?? _defaultIsAndroid {
    _actions = _pip.actions.listen(_onAction);
  }

  /// The ids the window's buttons report.
  static const String toggleMuteAction = 'toggle-mute';
  static const String leaveAction = 'leave';

  /// The drawables the mute button swaps between, in `android/app/src/main/res`.
  static const String mutedIcon = 'ic_pip_mic_off';
  static const String liveMicrophoneIcon = 'ic_pip_mic_on';

  final PipController _pip;
  final MenoLogger _logger;
  final ValueListenable<LiveSnapshot> _snapshot;
  final Id _accountId;
  final Future<void> Function() _onExit;
  final Future<void> Function(bool enabled) _onMicrophoneChanged;
  final bool Function() _isAndroid;

  StreamSubscription<String>? _actions;

  /// The room currently in the window, while it is armed.
  Broadcast? _broadcast;
  bool _canPublish = false;
  bool _armed = false;

  /// Whether this device has already been told PiP is unavailable.
  ///
  /// Support is asked on every arm — a reconnect arms again — so the log line
  /// is once per controller, not once per transition.
  bool _unsupportedReported = false;

  /// The muted state the window draws.
  ///
  /// One notifier for the controller's life rather than one per arm: the
  /// window's content listens to it, and replacing or disposing it underneath a
  /// window that has not finished closing is a notification into a dead object.
  final ValueNotifier<bool> _microphone = ValueNotifier(false);

  /// Whether the window is armed right now.
  bool get isArmed => _armed;

  @override
  Future<void> arm({
    required Broadcast broadcast,
    required bool canPublish,
    required bool isMicrophoneEnabled,
    required ValueListenable<String> elapsed,
  }) async {
    if (_armed || !_isAndroid()) return;

    try {
      if (!await _pip.isSupported()) {
        if (!_unsupportedReported) {
          _unsupportedReported = true;
          _logger.w(
            'Picture in Picture is not available on this device; the '
            'broadcast continues and the notification is its surface',
            tag: MenoLogTag.lifecycle.label,
          );
        }
        return;
      }

      _broadcast = broadcast;
      _canPublish = canPublish;
      _microphone.value = canPublish && isMicrophoneEnabled;

      final armed = await _pip.arm(
        PipContent.widget(
          (context) => BroadcastMiniStage(
            snapshot: _snapshot,
            elapsed: elapsed,
            microphoneEnabled: _microphone,
          ),
        ),
        options: _options(),
      );

      if (!armed) {
        _clear();
        _logger.w(
          'The floating window could not be prepared; the broadcast '
          'continues in the background',
          tag: MenoLogTag.lifecycle.label,
        );
        return;
      }

      _armed = true;
      _logger.i('Floating window armed', tag: MenoLogTag.lifecycle.label);
    } on Object catch (error, stackTrace) {
      _clear();
      _logger.e(
        'Could not prepare the floating window',
        tag: MenoLogTag.lifecycle.label,
        error: error,
        stackTrace: stackTrace,
      );
    }
  }

  @override
  Future<void> disarm() async {
    if (!_armed) return;
    _armed = false;

    try {
      // `disarm` alone only turns automatic entry off; a window that is already
      // open has to be exited, and on Android that is also the only way the app
      // can close its own.
      if (_pip.isOpen) await _pip.exit();
      await _pip.disarm();
      _logger.i('Floating window disarmed', tag: MenoLogTag.lifecycle.label);
    } on Object catch (error, stackTrace) {
      _logger.e(
        'Could not close the floating window',
        tag: MenoLogTag.lifecycle.label,
        error: error,
        stackTrace: stackTrace,
      );
    } finally {
      _clear();
    }
  }

  @override
  Future<void> updateMicrophone(bool enabled) async {
    if (!_armed) return;

    _microphone.value = enabled;
    await _pushOptions();
  }

  @override
  void dispose() {
    unawaited(_actions?.cancel() ?? Future<void>.value());
    _actions = null;
    _clear();
    _microphone.dispose();
  }

  /// A button on the window was tapped.
  ///
  /// Both actions run the same methods the in-app controls run, so the window
  /// cannot invent a second exit path: a creator's leave ends the broadcast and
  /// everyone else's leaves.
  Future<void> _onAction(String id) async {
    if (!_armed) return;

    if (id == leaveAction) {
      await _guard(_onExit, 'leaving the room from the floating window');
      return;
    }

    if (id != toggleMuteAction || !_canPublish) return;

    final enabled = !_microphone.value;
    _microphone.value = enabled;
    await _pushOptions();
    await _guard(
      () => _onMicrophoneChanged(enabled),
      'changing the microphone from the floating window',
    );
  }

  /// Re-renders the window's buttons after the mute state moved.
  Future<void> _pushOptions() async {
    if (!_armed || _broadcast == null) return;

    await _guard(
      () => _pip.updateOptions(_options()),
      'updating the floating window options',
    );
  }

  /// The window's shape, copy and buttons.
  PipOptions _options() {
    final broadcast = _broadcast;
    return PipOptions(
      aspectRatio: PipAspectRatio.landscape,
      // The whole point: leaving the app opens it without a second gesture.
      autoEnter: true,
      // Text-heavy content does not scale cleanly through a seamless resize.
      seamlessResize: false,
      title: broadcast?.title.orEmpty,
      subtitle: broadcast?.hostName.orEmpty,
      actions: [
        if (_canPublish)
          PipAction(
            id: toggleMuteAction,
            label: _microphone.value ? 'Mute' : 'Unmute',
            icon: PipActionIcon.drawable(
              _microphone.value ? liveMicrophoneIcon : mutedIcon,
            ),
          ),
        PipAction(
          id: leaveAction,
          label: broadcast?.canEndBroadcast(_accountId) ?? false
              ? 'End broadcast'
              : 'Leave',
          icon: PipActionIcon.close,
        ),
      ],
    );
  }

  /// Drops the room's window state, without touching the platform.
  void _clear() {
    _broadcast = null;
    _canPublish = false;
    _microphone.value = false;
  }

  /// Runs [step], logging rather than rethrowing.
  Future<void> _guard(Future<void> Function() step, String what) async {
    try {
      await step();
    } on Object catch (error, stackTrace) {
      _logger.e(
        'Could not finish $what',
        tag: MenoLogTag.lifecycle.label,
        error: error,
        stackTrace: stackTrace,
      );
    }
  }

  static bool _defaultIsAndroid() =>
      !kIsWeb && defaultTargetPlatform == TargetPlatform.android;
}
