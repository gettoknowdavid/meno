import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/loading_indicator/m_loading_indicator.dart';
import 'package:meno_ui/src/text/m_text.dart';
import 'package:meno_ui/src/theme/extensions/m_color_scheme.dart';
import 'package:meno_ui/src/theme/extensions/m_text_theme.dart';
import 'package:meno_ui/src/tokens/spacing.dart';

/// The footer for a paginated list.
///
/// Shows a boxed loading indicator while the next page is in flight, and an
/// end-of-list message once the last page has loaded. It renders nothing but
/// spacing when there are more pages and nothing is loading.
///
/// Example usage:
/// ```dart
/// MPagedLoadingIndicator(isLoading: isFetching, hasMore: hasNextPage)
/// ```
class MPagedLoadingIndicator extends StatelessWidget {
  /// Creates a paginated-list footer.
  const new({required this.isLoading, required this.hasMore, super.key});

  /// Whether the next page is currently being fetched.
  final bool isLoading;

  /// Whether more pages remain after the current one.
  final bool hasMore;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return Column(
      children: [
        if (isLoading) const MLoadingIndicator.box(),
        if (!isLoading && !hasMore)
          MText(
            'You have reached the end',
            style: textTheme.captionRegular,
            color: colors.onBackgroundVariant,
            textAlign: .center,
          ),
        const SizedBox(height: Insets.xl + Insets.xs),
      ],
    );
  }
}
