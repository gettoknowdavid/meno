import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_ui/meno_ui.dart';

/// A failure, said once and in place.
///
/// What a sheet or a modal shows instead of its content when the read behind it
/// failed: the message the API gave — or a plain one when the failure was not
/// an API error — and, when the caller has somewhere to retry, a button that
/// does it. It is deliberately small: a modal that has failed should still look
/// like the modal it was going to be.
class MenoErrorWidget extends StatelessWidget {
  /// Shows [error], with an optional retry.
  const new({
    required this.error,
    super.key,
    this.margin,
    this.showRetryButton = true,
    this.onRetry,
  });

  /// What went wrong. Rendered through [errorMessage] so an API failure reads
  /// as its message rather than as its runtime type.
  final Object? error;

  /// The space around the widget. Zero inside a sheet that already pads.
  final EdgeInsetsGeometry? margin;

  /// Whether to offer a retry.
  ///
  /// False where the caller cannot retry — a read that failed during a
  /// destructive flow, say, needs a different way forward than a button.
  final bool showRetryButton;

  /// What the retry button does. When null, the button is not drawn either.
  final VoidCallback? onRetry;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return Container(
      margin: margin ?? const EdgeInsets.all(Insets.lg),
      padding: const EdgeInsets.all(Insets.lg),
      decoration: BoxDecoration(
        color: colors.surfaceTint,
        borderRadius: Corners.lg,
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(MIcons.info_circle, size: 20, color: colors.error),
              Spaces.horizontalSmall,
              Expanded(
                child: MText(
                  'Something went wrong',
                  style: textTheme.bodyMedium,
                ),
              ),
            ],
          ),
          Spaces.verticalSmall,
          MText(
            errorMessage(error),
            style: textTheme.captionRegular,
            color: colors.onBackgroundVariant,
          ),
          if (showRetryButton && onRetry != null) ...[
            Spaces.verticalLarge,
            MTextButton(label: 'Try again', onPressed: onRetry),
          ],
        ],
      ),
    );
  }
}
