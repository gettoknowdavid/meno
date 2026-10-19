import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/_shared/services/permissions_service.dart';
import 'package:meno_ui/meno_ui.dart';

/// Where every permission dialog is worded and wired.
///
/// [PermissionsService] decides *when* a step is needed; this decides what it
/// looks like, so the same permission reads the same on every screen. Each
/// entry point is one moment in the app, named for the moment.
abstract final class PermissionPrompts {
  /// The host tapped "Start Broadcast".
  ///
  /// Answers whether the broadcast may go ahead, which is whether the
  /// microphone is granted. Asked here, before anything is created on the
  /// server, so a refusal never leaves a live room nobody can speak in.
  static Future<bool> broadcast(BuildContext context) async {
    final permissions = di<PermissionsService>();
    if (!permissions.requestBroadcast.isRunning.value) {
      await _run(
        () => permissions.requestBroadcast.runAsync(_blocking(context)),
      );
    }
    return permissions.microphone.value is PermissionGranted;
  }

  /// Somebody in a room is about to unmute.
  ///
  /// Covers whoever joined without publishing rights and was given them
  /// later (a co-host), and anyone who turned the microphone off in Settings
  /// since their last broadcast.
  static Future<bool> microphone(BuildContext context) async {
    final permissions = di<PermissionsService>();
    if (!permissions.requestMicrophone.isRunning.value) {
      await _run(
        () => permissions.requestMicrophone.runAsync(_blocking(context)),
      );
    }
    return permissions.microphone.value is PermissionGranted;
  }

  /// The user flipped a notifications switch.
  ///
  /// An explicit ask, so the system prompt comes straight away and a refusal
  /// for good offers Settings. Answers whether notifications are now on.
  static Future<bool> notificationsRequested(BuildContext context) async {
    final permissions = di<PermissionsService>();
    if (!permissions.requestNotification.isRunning.value) {
      await _run(
        () => permissions.requestNotification.runAsync(
          PermissionContext(
            showSettingsPrompt: (copy) => _confirm(
              context,
              title: copy.title,
              message: copy.settingsMessage,
              confirmLabel: 'Settings',
            ),
          ),
        ),
      );
    }
    return permissions.notifications.value is PermissionGranted;
  }

  /// The user just subscribed to somebody: the first moment they have a reason
  /// to want a notification.
  ///
  /// An implicit ask, so it is gentle: nothing is shown when notifications are
  /// already on or were refused for good, the explanation comes before the
  /// system prompt (which iOS only ever shows once), and "not now" is
  /// remembered for the device, whichever account is signed in.
  static Future<void> notificationsOffered(BuildContext context) async {
    final permissions = di<PermissionsService>();
    if (permissions.requestNotification.isRunning.value) return;

    await _run(
      () => permissions.requestNotification.runAsync(
        PermissionContext(
          showPrimer: (copy) => _confirm(
            context,
            title: copy.title,
            message: copy.message,
            confirmLabel: 'Turn on',
          ),
        ),
      ),
    );
  }

  /// Runs [pick], and explains a camera or photo library the user turned off.
  ///
  /// Nothing is asked before the picker opens: the system camera and photo
  /// picker need no permission. Only a refusal is handled, with a way to
  /// Settings; anything else is the picker failing, and is said plainly.
  static Future<void> guardMedia(
    BuildContext context,
    Future<void> Function() pick,
  ) async {
    try {
      await pick();
    } on MediaAccessDenied catch (denied) {
      if (!context.mounted) return;

      final what = denied.camera ? 'camera' : 'photos';
      final open = await _confirm(
        context,
        title: denied.camera ? 'Camera access is off' : 'Photo access is off',
        message: 'Turn on $what access for Menō in Settings to add a picture.',
        confirmLabel: 'Settings',
      );
      if (open) await di<PermissionsService>().openSettings();
    } on Exception catch (_) {
      if (context.mounted) {
        context.showErrorToast('Could not open your photos.');
      }
    }
  }

  /// For an action the user cannot do without the permission: explains a
  /// refusal and offers a second try, then Settings.
  static PermissionContext _blocking(BuildContext context) => PermissionContext(
    showRationale: (copy) => _confirm(
      context,
      title: copy.title,
      message: copy.message,
      confirmLabel: 'Retry',
    ),
    showSettingsPrompt: (copy) => _confirm(
      context,
      title: copy.title,
      message: copy.settingsMessage,
      confirmLabel: 'Settings',
    ),
  );

  static Future<bool> _confirm(
    BuildContext context, {
    required String title,
    required String message,
    required String confirmLabel,
  }) async {
    // The platform's own prompt has just been on screen; the page behind it
    // may have gone in the meantime.
    if (!context.mounted) return false;
    return await MConfirmationDialog.show(
      context,
      title: title,
      description: message,
      confirmLabel: confirmLabel,
      cancelLabel: 'Not now',
    );
  }

  /// A failed request has already been logged and reported by the command's
  /// own error handling; the caller only needs the permission's final state.
  static Future<void> _run(Future<void> Function() request) async {
    try {
      await request();
    } on Object {
      // Reported by the global handler.
    }
  }
}
