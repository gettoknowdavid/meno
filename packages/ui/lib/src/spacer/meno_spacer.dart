import 'package:flutter/material.dart';

/// {@template meno_spacer}
/// Meno Spacer
/// ```dart
///   const MSpacer.v(20) // gives const SizedBox(height: 20)
///   const MSpacer.h(20) // gives const SizedBox(width: 20)
///
/// ```
/// {@endtemplate}
class MSpacer extends StatelessWidget {
  /// Named private constructor for the vertical spacer factory
  const new _v(double value) : height = value, width = null;

  /// Named private constructor for the horizontal spacer factory
  const new _h(double value) : width = value, height = null;

  /// Builds the vertical space that covers the height of the given `value`.
  const factory v(double value) = MSpacer._v;

  /// Builds the horizontal space that covers the width of the given `value`.
  const factory h(double value) = MSpacer._h;

  /// The height that the space should cover
  final double? height;

  /// The width that the space should cover
  final double? width;

  @override
  Widget build(BuildContext context) => SizedBox(height: height, width: width);
}
