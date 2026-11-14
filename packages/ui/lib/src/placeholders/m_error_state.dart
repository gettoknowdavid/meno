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

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return Container(
      margin: margin,
      padding: padding,
      alignment: .center,
      decoration: ShapeDecoration(
        color: colors.primaryContainer,
        shape: const RoundedSuperellipseBorder(borderRadius: Corners.lg),
      ),
      child: Column(
        mainAxisAlignment: .center,
        children: [
          Icon(Icons.error_outline, color: colors.error),
          Spaces.verticalMedium,
          MText(
            message,
            style: textTheme.captionMedium,
            textAlign: .center,
            maxLines: 5,
          ),
          if (showRetry) ...[
            Spaces.verticalMedium,
            SizedBox(
              height: Insets.xxl,
              child: MTextButton(label: 'Retry', onPressed: onRetry),
            ),
          ],
        ],
      ),
    );
  }
}
