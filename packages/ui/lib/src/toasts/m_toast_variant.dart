import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/theme/extensions/m_color_scheme.dart';
import 'package:meno_ui/src/tokens/m_icons.dart';

/// The semantic tone of a toast.
///
/// The tone decides the toast's background, the colour of its content and the
/// icon shown before the message, so a caller only has to say *what kind* of
/// message it is, not how it should look.
///
/// Example usage:
/// ```dart
/// context.showErrorToast('Could not save your changes');
/// ```
enum MToastVariant {
  /// Neutral information.
  info,

  /// A successful outcome.
  success,

  /// A recoverable problem the user should notice.
  warning,

  /// A failure.
  error;

  /// The background colour for [colors].
  Color background(MColorScheme colors) => switch (this) {
    .info => colors.informational,
    .success => colors.success,
    .warning => colors.warning,
    .error => colors.error,
  };

  /// The colour of the content drawn on [background].
  Color foreground(MColorScheme colors) => switch (this) {
    .info => colors.onInformational,
    .success => colors.onSuccess,
    .warning => colors.onWarning,
    .error => colors.onError,
  };

  /// The icon shown before the message.
  IconData get icon => switch (this) {
    .info => MIcons.info_circle,
    .success => MIcons.check_circle,
    .warning => MIcons.announcement,
    .error => MIcons.x_circle,
  };
}
