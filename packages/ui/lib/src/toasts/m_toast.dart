import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/text/m_text.dart';
import 'package:meno_ui/src/theme/extensions/m_color_scheme.dart';
import 'package:meno_ui/src/theme/extensions/m_text_theme.dart';
import 'package:meno_ui/src/toasts/m_toast_variant.dart';
import 'package:meno_ui/src/tokens/corners.dart';
import 'package:meno_ui/src/tokens/m_icons.dart';
import 'package:meno_ui/src/tokens/shadows.dart';
import 'package:meno_ui/src/tokens/spacing.dart';

/// The default maximum width of a toast, in logical pixels.
///
/// Toasts are meant to be read at a glance, so on wide window they stay a
/// comfortable column instead of stretching edge to edge.
const double _kMaxToastWidth = 420;

/// The content of a toast: a tinted card with an icon, copy and close action.
///
/// Prefer showing one through a `BuildContext` toast extension rather than
/// handing it to an overlay yourself — the extension also owns animation,
/// stacking and auto-dismiss. Build an [MToast] directly only when you want a
/// bespoke layout, then show it with `showToastCard`.
///
/// Example usage:
/// ```dart
/// MToast(
///   'Your broadcast is live',
///   title: 'Success',
///   variant: MToastVariant.success,
/// )
/// ```
class MToast extends StatelessWidget {
  /// Creates a toast card.
  ///
  /// [variant] selects the tone, [icon] overrides the tone's default icon and
  /// [onClose] wires a dismiss action to the close button (which is only shown
  /// while [showCloseButton] is true and an [onClose] callback is set).
  const new(
    this.message, {
    super.key,
    this.title,
    this.variant = .info,
    this.icon,
    this.trailing,
    this.onTap,
    this.onClose,
    this.showCloseButton = true,
    this.maxWidth = _kMaxToastWidth,
    this.closeLabel = 'Dismiss',
    this.semanticLabel,
  });

  /// The message shown to the user.
  final String message;

  /// An optional bold title above [message].
  final String? title;

  /// The semantic tone applied to the card.
  final MToastVariant variant;

  /// Overrides the tone's default icon. Pass `SizedBox.shrink()` to drop it.
  final Widget? icon;

  /// An optional action placed after the copy, before the close button.
  final Widget? trailing;

  /// Called when the card itself is tapped.
  final VoidCallback? onTap;

  /// Called when the close button is tapped.
  final VoidCallback? onClose;

  /// Whether the close button is shown when [onClose] is non-null.
  final bool showCloseButton;

  /// The maximum width of the card.
  final double maxWidth;

  /// The screen-reader label of the close button.
  final String closeLabel;

  /// An optional screen-reader label for the whole card.
  final String? semanticLabel;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);
    final background = variant.background(colors);
    final foreground = variant.foreground(colors);

    return Semantics(
      liveRegion: true,
      container: true,
      label: semanticLabel,
      child: ConstrainedBox(
        constraints: BoxConstraints(maxWidth: maxWidth),
        child: DecoratedBox(
          decoration: BoxDecoration(
            color: background,
            borderRadius: Corners.md,
            boxShadow: Shadows.medium,
          ),
          child: Material(
            type: .transparency,
            child: InkWell(
              onTap: onTap,
              borderRadius: Corners.md,
              child: Padding(
                padding: const .symmetric(
                  horizontal: Insets.lg,
                  vertical: Insets.md,
                ),
                child: Row(
                  mainAxisSize: .min,
                  children: [
                    icon ??
                        Icon(variant.icon, color: foreground, size: Insets.lg),
                    Spaces.horizontalMedium,
                    Expanded(
                      child: Column(
                        mainAxisSize: .min,
                        crossAxisAlignment: .start,
                        children: [
                          if (title != null) ...[
                            MText(
                              title!,
                              style: textTheme.captionBold,
                              color: foreground,
                            ),
                            Spaces.verticalMicro,
                          ],
                          MText(
                            message,
                            style: textTheme.captionRegular,
                            color: foreground,
                          ),
                        ],
                      ),
                    ),
                    if (trailing != null) ...[
                      Spaces.horizontalMedium,
                      trailing!,
                    ],
                    if (showCloseButton && onClose != null) ...[
                      Spaces.horizontalSmall,
                      _MToastCloseButton(
                        color: foreground,
                        label: closeLabel,
                        onPressed: onClose!,
                      ),
                    ],
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// The close action drawn on a toast.
class _MToastCloseButton extends StatelessWidget {
  const new({
    required this.color,
    required this.label,
    required this.onPressed,
  });

  final Color color;
  final String label;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    return Semantics(
      button: true,
      label: label,
      child: InkResponse(
        onTap: onPressed,
        radius: Insets.lg,
        child: Icon(MIcons.x_close, size: Insets.lg, color: color),
      ),
    );
  }
}
