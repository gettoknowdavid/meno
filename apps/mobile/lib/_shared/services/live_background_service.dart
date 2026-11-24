import 'dart:async';
import 'dart:io';

import 'package:flutter_background/flutter_background.dart';
import 'package:meno_mobile/_core/_core.dart';

/// The notification a live room's foreground service shows on Android.
///
/// One definition for the two places that need it: `PermissionsService`
/// initialises the plugin while it is asking for the permission, before a room
/// exists, and [PlatformLiveBackgroundService] starts the service once the room
/// is up. Two copies of the copy would drift the first time either title is
/// edited, and the platform only keeps the channel a title was first created
/// with.
///
/// The icon is the launcher mipmap: a notification icon must exist as a
/// resource, and a name that does not resolve fails the notification (and with
/// it the service start) at runtime rather than at build time.
///
/// [isPublishing] only chooses the wording — a host is on air, a listener is
/// attending — because the two are different things to be told in a
/// notification shade.
FlutterBackgroundAndroidConfig liveBackgroundConfig({
  required bool isPublishing,
}) => FlutterBackgroundAndroidConfig(
  notificationTitle: isPublishing ? 'Menō is live' : 'Menō broadcast',
  notificationText: isPublishing
      ? 'Your broadcast is running in the background'
      : 'You are still listening in the background',
  notificationIcon: const AndroidResource(name: 'ic_launcher'),
);

/// Keeps a live room running while the app is not on screen.
///
/// A room is a WebRTC session owned by the Flutter isolate. Android suspends
/// that isolate — and eventually kills the process — when the app is
/// backgrounded without a foreground service, which is heard as "the broadcast
/// stopped a minute after I locked the phone". This is the seam that stops
/// that: one start when a room comes up, one stop when it closes.
///
/// It is an interface rather than a direct plugin call for the reason the room
/// transport is one: the session that owns the room must be testable without a
/// platform channel, and a build with no background capability wired yet is a
/// silent no-op rather than a missing plugin exception.
///
/// # iOS
///
/// Nothing to start. iOS keeps a call alive through the `audio` background mode
/// (`UIBackgroundModes` in `Info.plist`, already declared) as long as WebRTC
/// holds an active audio session, which it does for the life of a room — the
/// microphone for a publisher, the remote mix for a listener. So the
/// implementation is a no-op there, deliberately, not an oversight.
abstract interface class LiveBackgroundService {
  /// Starts the platform's foreground service for a room that is now open.
  ///
  /// Called after the room has connected and while the app is still on screen —
  /// Android refuses a microphone-type foreground service started from the
  /// background. Best effort: a refused service is a broadcast that may stop
  /// when the screen locks, never a broadcast that fails to start.
  Future<void> start({required bool isPublishing});

  /// Stops it, once the room is gone.
  ///
  /// Safe to call when nothing is running, which is the only thing that lets
  /// the session call it from a single teardown path.
  Future<void> stop();
}

/// The Android foreground service that keeps a room's isolate alive.
///
/// Wraps [`flutter_background`], which runs a foreground service plus a partial
/// wake lock, and asks the system to stop optimising the app's battery use. The
/// manifest declares the service's types (`microphone|connectedDevice|
/// mediaPlayback`) and the matching permissions, so a host's microphone and a
/// listener's playback are both covered by the same service.
///
/// # Why it is started per session, not at launch
///
/// The notification is a promise — "Menō is live" — and a permanent one on a
/// phone that is not in a broadcast is a lie in the shade. It is also what the
/// play store review reads. Start on a live room, stop when the room closes.
final class PlatformLiveBackgroundService implements LiveBackgroundService {
  /// Creates the service over [_logger].
  new(this._logger);

  final MenoLogger _logger;

  /// Whether this object has a service running. Not the platform's own answer,
  /// which is only consulted to avoid starting a second one.
  bool _running = false;

  @override
  Future<void> start({required bool isPublishing}) async {
    if (!Platform.isAndroid || _running) return;

    try {
      final initialized = await FlutterBackground.initialize(
        androidConfig: liveBackgroundConfig(isPublishing: isPublishing),
      );
      if (!initialized) {
        _logger.w(
          'The background service could not be initialised; a locked '
          'screen may end this broadcast',
          tag: MenoLogTag.lifecycle.label,
        );
        return;
      }

      if (!FlutterBackground.isBackgroundExecutionEnabled) {
        final enabled = await FlutterBackground.enableBackgroundExecution();
        if (!enabled) {
          _logger.w(
            'The platform refused background execution; a locked screen '
            'may end this broadcast',
            tag: MenoLogTag.lifecycle.label,
          );
          return;
        }
      }

      _running = true;
      _logger.i(
        'Background execution started for this '
        '${isPublishing ? 'broadcast' : 'room'}',
        tag: MenoLogTag.lifecycle.label,
      );
    } on Object catch (error, stackTrace) {
      // Never rethrown: the room is already live, and a room that is on air but
      // may not survive a locked screen is better than one that is torn down
      // because its notification failed.
      _logger.e(
        'Could not start background execution',
        tag: MenoLogTag.lifecycle.label,
        error: error,
        stackTrace: stackTrace,
      );
    }
  }

  @override
  Future<void> stop() async {
    if (!Platform.isAndroid || !_running) return;
    _running = false;

    try {
      await FlutterBackground.disableBackgroundExecution();
      _logger.i(
        'Background execution stopped',
        tag: MenoLogTag.lifecycle.label,
      );
    } on Object catch (error, stackTrace) {
      _logger.e(
        'Could not stop background execution',
        tag: MenoLogTag.lifecycle.label,
        error: error,
        stackTrace: stackTrace,
      );
    }
  }
}
