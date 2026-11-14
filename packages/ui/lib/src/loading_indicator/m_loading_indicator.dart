import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/gen/assets.gen.dart';
import 'package:meno_ui/src/theme/extensions/m_color_scheme.dart';
import 'package:meno_ui/src/tokens/corners.dart';

/// The fraction of the screen width the adaptive box takes up.
const double _kBoxWidthFraction = 0.2;

/// An animated loading indicator, in a handful of preset sizes.
///
/// The named constructors cover the sizes the design system uses, and
/// [MLoadingIndicator.box] fits a square to the screen for full-screen waits.
///
/// Example usage:
/// ```dart
/// const MLoadingIndicator.five();
/// ```
/// ```dart
/// const MLoadingIndicator.box(height: 50, width: 50);
/// ```
class MLoadingIndicator extends StatelessWidget {
  /// Creates a loading indicator at the given size.
  ///
  /// Parameters:
  /// - [height]: The height of the indicator.
  /// - [width]: The width of the indicator.
  /// - [key]: An optional key to identify the widget.
  const new(this.height, this.width, {super.key}) : _adaptive = false;

  /// The constructor every named constructor delegates to.
  const new _(this.height, this.width, {required this._adaptive, super.key});

  /// Creates a loading indicator sized 48x48.
  const new five({Key? key, double height = 48, double width = 48})
    : this._(height, width, adaptive: false, key: key);

  /// Creates a loading indicator sized 40x40.
  const new four({Key? key, double height = 40, double width = 40})
    : this._(height, width, adaptive: false, key: key);

  /// Creates a loading indicator sized 1200x1200.
  const new one({Key? key, double height = 1200, double width = 1200})
    : this._(height, width, adaptive: false, key: key);

  /// Creates a loading indicator sized 32x32.
  const new three({Key? key, double height = 32, double width = 32})
    : this._(height, width, adaptive: false, key: key);

  /// Creates a loading indicator sized 24x24.
  const new two({Key? key, double height = 24, double width = 24})
    : this._(height, width, adaptive: false, key: key);

  /// Creates a square indicator that adapts to the screen width.
  ///
  /// Parameters:
  /// - [key]: An optional key to identify the widget.
  /// - [height]: The height of the indicator, defaulting to a fifth of the
  ///   screen width.
  /// - [width]: The width of the indicator, defaulting to a fifth of the
  ///   screen width.
  const new box({Key? key, double? height, double? width})
    : this._(height, width, adaptive: true, key: key);

  /// The height of the loading indicator.
  final double? height;

  /// The width of the loading indicator.
  final double? width;

  /// Whether the indicator sizes itself from the screen.
  final bool _adaptive;

  @override
  Widget build(BuildContext context) {
    if (!_adaptive) {
      return SizedBox(
        height: height,
        width: width,
        child: Assets.images.loading.image(
          fit: BoxFit.cover,
          package: 'meno_ui',
        ),
      );
    }

    final colors = MColorScheme.of(context);
    final dimension = MediaQuery.sizeOf(context).width * _kBoxWidthFraction;

    return Center(
      child: Container(
        height: height ?? dimension,
        width: width ?? dimension,
        decoration: BoxDecoration(
          color: colors.background,
          borderRadius: Corners.md,
        ),
        child: Assets.images.loading.image(package: 'meno_ui'),
      ),
    );
  }
}
