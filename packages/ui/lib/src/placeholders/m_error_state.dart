import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/buttons/m_text_button.dart';
import 'package:meno_ui/src/text/m_text.dart';
import 'package:meno_ui/src/theme/extensions/m_color_scheme.dart';
import 'package:meno_ui/src/theme/extensions/m_text_theme.dart';
import 'package:meno_ui/src/tokens/corners.dart';
import 'package:meno_ui/src/tokens/spacing.dart';

/// A centred error-state placeholder with an optional retry action.
///
/// Pairs with `MEmptyState`: use this when a request failed rather than when a
/// list is simply empty.
///
/// The card fits the space it is given. A short slot — a feed preview strip
/// caps its content — gets the icon and one line of message, scaled down to
/// fit, instead of a card that overflows it.
///
/// Example usage:
/// ```dart
/// MErrorState(
///   message: 'Could not load broadcasts',
///   onRetry: loadBroadcasts,
/// )
/// ```
class MErrorState extends StatelessWidget {
  /// Creates an error state.
  ///
  /// [message] is the text shown to the user. Set [showRetry] to false to hide
  /// the retry button even when [onRetry] is provided.
  const new({
    required this.message,
    super.key,
    this.onRetry,
    this.showRetry = true,
    this.margin = const .all(Insets.lg),
    this.padding = const .all(Insets.lg),
  });

  /// The message shown to the user.
  final String message;

  /// Called when the retry button is tapped.
  final VoidCallback? onRetry;

  /// Whether the retry button is shown.
  final bool showRetry;

  /// The margin around the error card.
  final EdgeInsetsGeometry? margin;

  /// The padding inside the error card.
  final EdgeInsetsGeometry? padding;

  /// Below this height the card is drawn without its margins and action.
  static const double _compactHeight = 140;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return LayoutBuilder(
      builder: (context, constraints) {
        // What the card can actually use is the slot minus its own chrome.
        final innerHeight =
            constraints.maxHeight -
            (margin?.vertical ?? 0) -
            (padding?.vertical ?? 0);
        final compact =
            constraints.hasBoundedHeight && innerHeight < _compactHeight;
        final contentWidth = constraints.maxWidth - 2 * Insets.lg;

        final content = Column(
          mainAxisAlignment: .center,
          mainAxisSize: .min,
          children: [
            Icon(
              Icons.error_outline,
              color: colors.error,
              size: compact ? 20 : null,
            ),
            if (compact) Spaces.verticalSmall else Spaces.verticalMedium,
            MText(
              message,
              style: textTheme.captionMedium,
              textAlign: .center,
              maxLines: compact ? 1 : 5,
              overflow: .ellipsis,
            ),
            // A compact slot has no room for the action as well; the retry
            // stays on the screen this card is a preview of.
            if (showRetry && !compact) ...[
              Spaces.verticalMedium,
              SizedBox(
                height: Insets.xxl,
                child: MTextButton(label: 'Retry', onPressed: onRetry),
              ),
            ],
          ],
        );

        return Container(
          margin: compact ? EdgeInsets.zero : margin,
          padding: compact
              ? const .symmetric(horizontal: Insets.lg, vertical: Insets.sm)
              : padding,
          alignment: .center,
          decoration: ShapeDecoration(
            color: colors.primaryContainer,
            shape: const RoundedSuperellipseBorder(borderRadius: Corners.lg),
          ),
          child: compact
              ? FittedBox(
                  fit: .scaleDown,
                  child: SizedBox(
                    width: contentWidth > 0
                        ? contentWidth
                        : constraints.maxWidth,
                    child: content,
                  ),
                )
              : content,
        );
      },
    );
  }
}
