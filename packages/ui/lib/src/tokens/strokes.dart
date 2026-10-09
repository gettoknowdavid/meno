import 'package:flutter/widgets.dart';

/// The stroke-width scale shared by the whole design system.
///
/// Stroke widths are finer than the `Insets` spacing scale, so they get their
/// own scale instead of being spelled out at each call site.
@immutable
class Strokes {
  /// Prevents instantiation: this type is a namespace for the scale.
  const new _();

  /// Thin stroke (1.5 units), used by outlined buttons and rings.
  static const double thin = 1.5;
}
