import 'package:flutter/widgets.dart';

/// The spacing scale, in logical pixels, shared by the whole design system.
///
/// These are raw scale values: compose them into `EdgeInsets`, or pick a
/// ready-made gap from [Spaces]. Keeping the scale in one place is what lets
/// layout code stay consistent across features.
@immutable
final class Insets {
  /// Prevents instantiation: this type is a namespace for the scale.
  const Insets._();

  /// Micro spacing (4 units).
  static const double xs = 4;

  /// Small spacing (8 units).
  static const double sm = 8;

  /// Medium spacing (12 units).
  static const double md = 12;

  /// Large spacing (16 units).
  static const double lg = 16;

  /// Extra-large spacing (24 units).
  static const double xl = 24;

  /// Extra-extra-large spacing (32 units).
  static const double xxl = 32;

  /// Extra-extra-extra-large spacing (48 units).
  static const double xxxl = 48;

  /// Circular spacing (555 units, typically used for full circles).
  static const double circle = 555;
}

/// A [SizedBox] gap for every step of the [Insets] scale.
///
/// Use these instead of hand-written `SizedBox` values so the spacing rhythm
/// stays aligned with the scale.
@immutable
final class Spaces {
  /// Prevents instantiation: this type is a namespace for the gaps.
  const Spaces._();

  // Horizontal Spaces
  /// A [SizedBox] with a width of [Insets.xs] logical pixels.
  static const SizedBox horizontalMicro = SizedBox(width: Insets.xs);

  /// A [SizedBox] with a width of [Insets.sm] logical pixels.
  static const SizedBox horizontalSmall = SizedBox(width: Insets.sm);

  /// A [SizedBox] with a width of [Insets.md] logical pixels.
  static const SizedBox horizontalMedium = SizedBox(width: Insets.md);

  /// A [SizedBox] with a width of [Insets.lg] logical pixels.
  static const SizedBox horizontalLarge = SizedBox(width: Insets.lg);

  /// A [SizedBox] with a width of [Insets.xl] logical pixels.
  static const SizedBox horizontalXLarge = SizedBox(width: Insets.xl);

  /// A [SizedBox] with a width of [Insets.xxl] logical pixels.
  static const SizedBox horizontalXXLarge = SizedBox(width: Insets.xxl);

  /// A [SizedBox] with a width of [Insets.xxxl] logical pixels.
  static const SizedBox horizontalXXXLarge = SizedBox(width: Insets.xxxl);

  // Vertical Spaces
  /// A [SizedBox] with a height of [Insets.xs] logical pixels.
  static const SizedBox verticalMicro = SizedBox(height: Insets.xs);

  /// A [SizedBox] with a height of [Insets.sm] logical pixels.
  static const SizedBox verticalSmall = SizedBox(height: Insets.sm);

  /// A [SizedBox] with a height of [Insets.md] logical pixels.
  static const SizedBox verticalMedium = SizedBox(height: Insets.md);

  /// A [SizedBox] with a height of [Insets.lg] logical pixels.
  static const SizedBox verticalLarge = SizedBox(height: Insets.lg);

  /// A [SizedBox] with a height of [Insets.xl] logical pixels.
  static const SizedBox verticalXLarge = SizedBox(height: Insets.xl);

  /// A [SizedBox] with a height of [Insets.xxl] logical pixels.
  static const SizedBox verticalXXLarge = SizedBox(height: Insets.xxl);

  /// A [SizedBox] with a height of [Insets.xxxl] logical pixels.
  static const SizedBox verticalXXXLarge = SizedBox(height: Insets.xxxl);
}
