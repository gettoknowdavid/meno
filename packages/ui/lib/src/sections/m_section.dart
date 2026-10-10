import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/navigation/m_header.dart';
import 'package:meno_ui/src/text/m_text.dart';
import 'package:meno_ui/src/theme/extensions/m_color_scheme.dart';
import 'package:meno_ui/src/tokens/spacing.dart';

/// The default maximum height of a section's content.
const double _kDefaultMaxContentHeight = 184;

/// A titled section of content, with an optional "see all" action.
///
/// Composes an [MHeader] with the content returned by [builder], constraining
/// that content's height so horizontal lists keep a consistent rhythm.
///
/// Example usage:
/// ```dart
/// MSection(
///   title: 'Live for you',
///   onSeeAll: openAll,
///   builder: (context) => const LiveForYouStrip(),
/// )
/// ```
class MSection extends StatelessWidget {
  /// Creates a titled section.
  ///
  /// [builder] receives the build context and returns the section content. The
  /// "see all" action is only shown when [onSeeAll] is provided, labelled with
  /// [seeAllLabel], and [maxContentHeight] caps the content's height.
  const new({
    required this.title,
    required this.builder,
    super.key,
    this.onSeeAll,
    this.seeAllLabel = 'See all',
    this.maxContentHeight = _kDefaultMaxContentHeight,
    this.padding,
  });

  /// The section title.
  final String title;

  /// Builds the section content.
  final Widget Function(BuildContext context) builder;

  /// Called when the "see all" action is tapped.
  final VoidCallback? onSeeAll;

  /// The label of the "see all" action.
  final String seeAllLabel;

  /// The maximum height of the section content.
  final double maxContentHeight;

  /// Optional padding for the header.
  final EdgeInsetsGeometry? padding;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        MHeader(title: title, action: _seeAllAction(context), padding: padding),
        const SizedBox(height: Insets.xl),
        LimitedBox(maxHeight: maxContentHeight, child: builder(context)),
      ],
    );
  }

  /// The "see all" action, or an empty box when [onSeeAll] is null.
  Widget _seeAllAction(BuildContext context) {
    if (onSeeAll == null) return const SizedBox.shrink();

    final colors = MColorScheme.of(context);
    return InkWell(
      onTap: onSeeAll,
      child: MText(seeAllLabel, color: colors.onBackgroundVariant),
    );
  }
}
