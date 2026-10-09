import 'package:flutter/material.dart';
import 'package:meno_ui/src/theme/extensions/m_color_scheme.dart';
import 'package:meno_ui/src/tokens/spacing.dart';
import 'package:skeletonizer/skeletonizer.dart';

/// Sweeps a shimmer over [child], or over a plain placeholder box.
///
/// The effect is painted with the theme's background and surface-shade colours,
/// so a skeleton matches the scheme in light and dark. While [enabled] is false
/// the content is painted untouched.
///
/// When no child is given, a box of the requested [height], [width], [shape]
/// and [borderRadius] is used instead.
///
/// Example usage:
/// ```dart
/// MShimmer(
///   enabled: true,
///   height: 100,
///   width: 100,
///   shape: BoxShape.circle,
/// );
/// ```
class MShimmer extends StatelessWidget {
  /// Creates a shimmering skeleton.
  ///
  /// Parameters:
  /// - [key]: An optional key to identify the widget.
  /// - [enabled]: Whether the shimmer is animating. When false, the child is
  ///   painted as-is. Defaults to true.
  /// - [child]: The widget to shimmer. When null, a placeholder box is drawn.
  /// - [backgroundColor]: The fill of the placeholder box. Defaults to the
  ///   theme's background colour.
  /// - [height]: The height of the placeholder box, ignored when [child] is
  ///   given.
  /// - [width]: The width of the placeholder box, defaulting to the available
  ///   width.
  /// - [borderRadius]: The corner radius of a rectangular placeholder.
  /// - [shape]: The shape of the placeholder. Defaults to a rectangle.
  const new({
    super.key,
    this.enabled = true,
    this.child,
    this.backgroundColor,
    this.height,
    this.width,
    this.borderRadius,
    this.shape = BoxShape.rectangle,
  });

  /// Whether the shimmer is animating. When false, the child is painted as-is.
  final bool enabled;

  /// The widget to shimmer, or null for a placeholder box.
  final Widget? child;

  /// The fill of the placeholder box, defaulting to the theme's background.
  final Color? backgroundColor;

  /// The height of the placeholder box, ignored when [child] is given.
  final double? height;

  /// The width of the placeholder box, defaulting to the available width.
  final double? width;

  /// The corner radius of a rectangular placeholder.
  final double? borderRadius;

  /// The shape of the placeholder.
  final BoxShape shape;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final isCircle = shape == BoxShape.circle;
    final decoration = BoxDecoration(
      shape: shape,
      color: backgroundColor ?? colors.background,
      borderRadius: isCircle
          ? null
          : BorderRadius.circular(borderRadius ?? Insets.xs),
    );

    return Skeletonizer(
      enabled: enabled,
      containersColor: backgroundColor ?? colors.background,
      effect: ShimmerEffect(
        baseColor: colors.background,
        highlightColor: colors.surfaceShade,
      ),
      child: Container(
        decoration: decoration,
        child:
            child ??
            Container(
              height: height,
              width: width ?? double.infinity,
              decoration: decoration,
            ),
      ),
    );
  }
}
