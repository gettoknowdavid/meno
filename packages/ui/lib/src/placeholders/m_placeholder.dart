import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/gen/assets.gen.dart';

/// Draws the Meno logo, scaled to the space it is given.
///
/// The mark is picked from the ambient brightness, so it stays visible on both
/// the light and the dark background.
class MPlaceholder extends StatelessWidget {
  /// Creates a placeholder that draws the logo.
  ///
  /// * `dimension`: The size of the square the logo occupies. When null, the
  ///   placeholder adapts to its parent's constraints.
  const new({super.key, this.dimension});

  /// The size of the placeholder.
  final double? dimension;

  @override
  Widget build(BuildContext context) {
    final isLight = Theme.of(context).brightness == Brightness.light;

    return SizedBox.square(
      dimension: dimension,
      child: isLight
          ? Assets.images.logoDark.svg()
          : Assets.images.logoLight.svg(),
    );
  }
}
