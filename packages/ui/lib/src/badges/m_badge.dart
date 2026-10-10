import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/gen/assets.gen.dart';
import 'package:meno_ui/src/internal/m_internal.dart';
import 'package:meno_ui/src/text/m_text.dart';
import 'package:meno_ui/src/theme/extensions/m_color_scheme.dart';
import 'package:meno_ui/src/theme/extensions/m_text_theme.dart';
import 'package:meno_ui/src/tokens/corners.dart';
import 'package:meno_ui/src/tokens/m_color.dart';
import 'package:meno_ui/src/tokens/spacing.dart';
import 'package:skeletonizer/skeletonizer.dart';

/// The height of the rectangular badges, which sits between the `lg` and `xl`
/// steps of the spacing scale.
const double _kHeight = 20;

/// The width reserved for the 'Co-host' label.
const double _kCohostWidth = 60;

/// The widest a short badge label may grow before it wraps.
const double _kMaxLabelWidth = 59;

/// The diameter of the dot badge.
const double _kDotSize = 6;

/// The gap between a badge label and its view count.
const double _kCountGap = 6;

/// The side padding of the rectangular badges.
const EdgeInsets _kPadding = EdgeInsets.symmetric(horizontal: Insets.sm);

/// The vertical padding of a badge that wraps its label.
const EdgeInsets _kWrappingPadding = EdgeInsets.symmetric(
  horizontal: Insets.sm,
  vertical: Insets.xs,
);

/// The corner radius of the rectangular badges.
const BorderRadius _kRadius = Corners.xs;

/// The tracking applied to every badge label.
const double _kLetterSpacing = 0.5;

/// A small status pill: 'Co-host', 'Host', 'LIVE', 'OFF-AIR', 'RECONNECTING'
/// or 'NEW', plus a plain dot and a free-form label.
///
/// Each named constructor picks the colours and the type style that fit its
/// meaning, resolving them from the enclosing theme so the badge follows the
/// light and dark schemes. The unnamed sizes stay theme-aware too: a badge
/// without an explicit colour falls back to the error pair.
///
/// Example usage:
/// ```dart
/// MBadge.cohost(context)
/// ```
/// ```dart
/// MBadge.live(count: '10', showLoader: true)
/// ```
class MBadge extends StatelessWidget {
  /// Creates a co-host badge with predefined styles.
  ///
  /// The [context] parameter is used to access the theme for text styling.
  new cohost(BuildContext context, {Key? key})
    : this._(
        key: key,
        value: 'Co-host',
        height: _kHeight,
        width: _kCohostWidth,
        borderRadius: _kRadius,
        color: MColorScheme.of(context).inActiveContainer,
        valueColor: MColor.primary700,
        textStyle: MTextTheme.of(context).microMedium,
      );

  /// Creates a host badge with predefined styles.
  ///
  /// The [context] parameter is used to access the theme for text styling.
  new host(BuildContext context, {Key? key})
    : this._(
        key: key,
        value: 'Host',
        constraints: const BoxConstraints(
          minHeight: _kHeight,
          maxWidth: _kMaxLabelWidth,
        ),
        padding: _kPadding,
        borderRadius: _kRadius,
        color: MColorScheme.of(context).inActiveContainer,
        valueColor: MColor.primary700,
        textStyle: MTextTheme.of(context).microMedium,
      );

  /// Creates a large badge with a customizable value.
  ///
  /// The [value] parameter is required and specifies the text to display on
  /// the badge.
  /// The [key] parameter can be used to uniquely identify this widget in the
  /// widget tree.
  const new large({required String value, Key? key})
    : this._(
        key: key,
        value: value,
        constraints: const BoxConstraints(
          minHeight: Insets.lg,
          minWidth: Insets.lg,
        ),
      );

  /// Creates a live badge with optional loader and border indicators.
  ///
  /// The [count] parameter specifies the view count to display on the badge.
  /// The [showLoader] parameter indicates whether to display a loader.
  /// The [showBorder] parameter indicates whether to display a border around
  /// the badge.
  /// The [key] parameter can be used to uniquely identify this widget in the
  /// widget tree.
  const new live({
    Key? key,
    String? count,
    bool showLoader = false,
    bool showBorder = false,
  }) : this._(
         key: key,
         value: 'LIVE',
         viewCount: count,
         height: _kHeight,
         showLoader: showLoader,
         constraints: const BoxConstraints(minHeight: _kHeight),
         padding: _kPadding,
         showBorder: showBorder,
       );

  /// Creates an off-air badge with predefined styles.
  ///
  /// The [context] parameter is used to access the theme for colours.
  new offAir(BuildContext context, {Key? key})
    : this._(
        key: key,
        value: 'OFF-AIR',
        height: _kHeight,
        constraints: const BoxConstraints(minHeight: _kHeight),
        padding: _kPadding,
        color: MColorScheme.of(context).disabledContainer,
        valueColor: MColorScheme.of(context).onDisabled,
      );

  /// Creates a reconnecting badge with predefined styles.
  ///
  /// The [context] parameter is used to access the theme for colours.
  new reconnecting(BuildContext context, {Key? key})
    : this._(
        key: key,
        value: 'RECONNECTING',
        height: _kHeight,
        constraints: const BoxConstraints(minHeight: _kHeight),
        padding: _kPadding,
        color: MColorScheme.of(context).errorContainer,
        valueColor: MColorScheme.of(context).onErrorContainer,
      );

  /// Creates a new badge with a customizable value.
  ///
  /// The [key] parameter can be used to uniquely identify this widget in the
  /// widget tree.
  new newBadge(BuildContext context, {Key? key})
    : this._(
        key: key,
        value: 'NEW',
        constraints: const BoxConstraints(minHeight: Insets.lg),
        padding: _kWrappingPadding,
        borderRadius: _kRadius,
        valueColor: MColorScheme.of(context).primary,
        color: MInternal.resolve(
          MColorScheme.of(context).brightness == Brightness.light,
          MColor.newBadgeLight,
          MColor.newBadgeDark,
        ),
      );

  /// Creates a small badge, a bare dot.
  ///
  /// The [key] parameter can be used to uniquely identify this widget in the
  /// widget tree.
  const new small({Key? key})
    : this._(
        key: key,
        height: _kDotSize,
        width: _kDotSize,
        constraints: const BoxConstraints(
          minHeight: _kDotSize,
          minWidth: _kDotSize,
        ),
      );

  /// Creates a badge with custom styles.
  ///
  /// The [color] parameter specifies the background color of the badge.
  /// The [value] parameter specifies the text to display on the badge.
  /// The [viewCount] parameter specifies an optional view count.
  /// The [textStyle] parameter specifies the style for the badge text.
  /// The [valueColor] parameter specifies the color of the badge text.
  /// The [borderRadius] parameter specifies the border radius of the badge.
  /// The [height] and [width] parameters specify the size of the badge.
  /// The [padding] parameter specifies the padding around the badge content.
  /// The [constraints] parameter specifies the constraints on the badge size.
  /// The [showLoader] parameter indicates whether to show a loader.
  /// The [showBorder] parameter indicates whether to show a border around the
  /// badge.
  const new _({
    super.key,
    this.color,
    this.value,
    this.viewCount,
    this.textStyle,
    this.valueColor,
    this.borderRadius = Corners.circle,
    this.height,
    this.width,
    this.padding,
    this.constraints,
    this.showLoader = false,
    this.showBorder = false,
  });

  /// The background colour of the badge, defaulting to the error colour.
  final Color? color;

  /// The text to display on the badge.
  final String? value;

  /// An optional view count to display on the badge.
  final String? viewCount;

  /// The style for the badge text, defaulting to the theme's micro style.
  final TextStyle? textStyle;

  /// The colour of the badge text, defaulting to the on-error colour.
  final Color? valueColor;

  /// The border radius of the badge.
  final BorderRadius? borderRadius;

  /// The height of the badge.
  final double? height;

  /// The width of the badge.
  final double? width;

  /// The padding around the badge content.
  final EdgeInsets? padding;

  /// The constraints on the badge size.
  final BoxConstraints? constraints;

  /// Whether to show a loader on the badge.
  final bool showLoader;

  /// Whether to show a border around the badge.
  final bool showBorder;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final effectiveValueColor = valueColor ?? colors.onError;

    Widget? child;
    if (value != null) {
      final effectiveTextStyle =
          (textStyle ?? MTextTheme.of(context).microMedium).copyWith(
            letterSpacing: _kLetterSpacing,
          );

      child = Row(
        mainAxisAlignment: MainAxisAlignment.center,
        mainAxisSize: MainAxisSize.min,
        children: [
          if (showLoader)
            Assets.images.loading.image(
              color: effectiveValueColor,
              package: 'meno_ui',
            ),
          _buildText(
            content: value!,
            style: effectiveTextStyle,
            color: effectiveValueColor,
          ),
          if (viewCount != null) ...[
            const SizedBox(width: _kCountGap),
            _buildText(
              content: viewCount!,
              style: effectiveTextStyle,
              color: effectiveValueColor,
            ),
          ],
        ],
      );
    }

    return Skeleton.leaf(
      child: Container(
        height: height,
        width: width,
        padding: padding,
        constraints: constraints,
        decoration: ShapeDecoration(
          color: color ?? colors.error,
          shape: RoundedRectangleBorder(
            borderRadius: borderRadius ?? BorderRadius.zero,
            side: showBorder
                ? BorderSide(color: valueColor ?? Colors.white)
                : BorderSide.none,
          ),
        ),
        child: child,
      ),
    );
  }

  /// Draws one run of badge text with the already-resolved style and colour.
  Widget _buildText({
    required String content,
    required TextStyle style,
    required Color color,
  }) {
    return MText(
      content,
      textAlign: TextAlign.center,
      style: style,
      color: color,
    );
  }
}
