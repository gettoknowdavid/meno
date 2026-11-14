import 'dart:async';

import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/toasts/m_toast.dart';
import 'package:meno_ui/src/toasts/m_toast_controller.dart';
import 'package:meno_ui/src/toasts/m_toast_position.dart';
import 'package:meno_ui/src/toasts/m_toast_variant.dart';

/// Shows design-system toasts from any [BuildContext] under an overlay.
///
/// The extension resolves the toast stack for the root overlay, so a toast
/// survives navigation and is never clipped by the page that asked for it.
///
/// Example usage:
/// ```dart
/// context.showSuccessToast('Broadcast published');
/// context.showErrorToast('Upload failed', position: MToastPosition.topCenter);
/// ```
extension MToastBuildContextX on BuildContext {
  /// The toast stack for this context's root overlay.
  ///
  /// Throws when the context sits outside an overlay — for example above the
  /// `MaterialApp` that provides `Navigator` — because a toast has nowhere to
  /// render without one.
  MToastController get mToasts {
    final overlay = Overlay.maybeOf(this, rootOverlay: true);
    assert(
      overlay != null,
      'No Overlay in scope. Show toasts from a context below a MaterialApp '
      '(or any widget that creates a Navigator/Overlay).',
    );
    return MToastController.of(overlay!);
  }

  /// Shows [message] as a toast and returns its id.
  ///
  /// [variant] selects the tone, [position] the edge or corner it enters from
  /// and [duration] how long it stays (use [Duration.zero] to keep it until
  /// dismissed). Set [dismissible] to false to hide the close button.
  int showToast(
    String message, {
    String? title,
    MToastVariant variant = .info,
    MToastPosition position = .bottomCenter,
    Duration duration = const Duration(seconds: 3),
    bool dismissible = true,
    Widget? icon,
    Widget? trailing,
    VoidCallback? onTap,
    VoidCallback? onDismiss,
    double maxWidth = 420,
    String? semanticLabel,
  }) {
    return mToasts.show(
      MToastRequest(
        position: position,
        duration: duration,
        onDismiss: onDismiss,
        builder: (_, dismiss) => MToast(
          message,
          title: title,
          variant: variant,
          icon: icon,
          trailing: trailing,
          onTap: onTap,
          maxWidth: maxWidth,
          semanticLabel: semanticLabel,
          onClose: dismissible ? dismiss : null,
        ),
      ),
    );
  }

  /// Shows a success toast.
  int showSuccessToast(
    String message, {
    String? title,
    MToastPosition position = .bottomCenter,
    Duration duration = const Duration(seconds: 3),
    bool dismissible = true,
  }) {
    return showToast(
      message,
      title: title,
      variant: .success,
      position: position,
      duration: duration,
      dismissible: dismissible,
    );
  }

  /// Shows an error toast.
  int showErrorToast(
    String message, {
    String? title,
    MToastPosition position = .bottomCenter,
    Duration duration = const Duration(seconds: 3),
    bool dismissible = true,
  }) {
    return showToast(
      message,
      title: title,
      variant: .error,
      position: position,
      duration: duration,
      dismissible: dismissible,
    );
  }

  /// Shows a warning toast.
  int showWarningToast(
    String message, {
    String? title,
    MToastPosition position = .bottomCenter,
    Duration duration = const Duration(seconds: 3),
    bool dismissible = true,
  }) {
    return showToast(
      message,
      title: title,
      variant: .warning,
      position: position,
      duration: duration,
      dismissible: dismissible,
    );
  }

  /// Shows an informational toast.
  int showInfoToast(
    String message, {
    String? title,
    MToastPosition position = .bottomCenter,
    Duration duration = const Duration(seconds: 3),
    bool dismissible = true,
  }) {
    return showToast(
      message,
      title: title,
      position: position,
      duration: duration,
      dismissible: dismissible,
    );
  }

  /// Shows an already-built [MToast], for bespoke layouts and actions.
  int showToastCard(
    MToast toast, {
    MToastPosition position = .bottomCenter,
    Duration duration = const Duration(seconds: 3),
    VoidCallback? onDismiss,
  }) {
    return mToasts.show(
      MToastRequest(
        position: position,
        duration: duration,
        onDismiss: onDismiss,
        builder: (_, _) => toast,
      ),
    );
  }

  /// Animates the toast with [id] away.
  void dismissToast(int id) => mToasts.dismiss(id);

  /// Animates every visible toast away and drops the queue.
  void clearToasts() => unawaited(mToasts.clear());
}
