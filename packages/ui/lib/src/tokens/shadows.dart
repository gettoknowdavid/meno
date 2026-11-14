import 'package:flutter/widgets.dart';

/// Elevation shadows shared by the whole design system.
///
/// Only three neutral elevations plus the brand-tinted [mic] shadow exist, so
/// surfaces differ by intent rather than by ad-hoc blur values.
@immutable
class Shadows {
  /// Prevents instantiation: this type is a namespace for the shadows.
  const new _();

  static const double _blurRadius = 10;
  static const double _spreadRadius = 2;

  /// Soft shadow.
  static const List<BoxShadow> soft = [
    BoxShadow(
      color: Color(0x0C000000),
      offset: Offset(0, 2),
      blurRadius: _blurRadius,
      spreadRadius: _spreadRadius,
    ),
  ];

  /// Medium shadow.
  static const List<BoxShadow> medium = [
    BoxShadow(
      color: Color(0x14000000),
      offset: Offset(0, 6),
      blurRadius: _blurRadius,
      spreadRadius: 4,
    ),
  ];

  /// Strong shadow.
  static const List<BoxShadow> strong = [
    BoxShadow(
      color: Color(0x14000000),
      offset: Offset(0, 8),
      blurRadius: _blurRadius,
      spreadRadius: _spreadRadius,
    ),
    BoxShadow(color: Color(0x14000000), offset: Offset(0, -1), blurRadius: 12),
  ];

  /// Brand-tinted micro shadow, used to lift small accent surfaces.
  static const List<BoxShadow> mic = [
    BoxShadow(
      color: Color(0x4B360090),
      blurRadius: 8,
      offset: Offset(0, 5),
      spreadRadius: 1,
    ),
  ];
}
