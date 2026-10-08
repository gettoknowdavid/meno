import 'package:flutter/material.dart';

/// A class containing predefined spacing values for UI components.
@immutable
class Insets {
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

/// A class containing predefined corner radius values for UI components.
@immutable
class Corners {
  /// Micro corner radius (4 units).
  static const BorderRadius xs = BorderRadius.all(Radius.circular(4));

  /// Small corner radius (8 units).
  static const BorderRadius sm = BorderRadius.all(Radius.circular(8));

  /// Medium corner radius (12 units).
  static const BorderRadius md = BorderRadius.all(Radius.circular(12));

  /// Large corner radius (16 units).
  static const BorderRadius lg = BorderRadius.all(Radius.circular(16));

  /// Extra-large corner radius (24 units).
  static const BorderRadius xl = BorderRadius.all(Radius.circular(24));

  /// Extra-extra-large corner radius (32 units).
  static const BorderRadius xxl = BorderRadius.all(Radius.circular(32));

  /// Extra-extra-extra-large corner radius (48 units).
  static const BorderRadius xxxl = BorderRadius.all(Radius.circular(48));

  /// Circular corner radius (555 units, typically used for full circles).
  static const BorderRadius circle = BorderRadius.all(Radius.circular(555));

  // /// Large squircle corner radius.
  // static final squircleLg = SmoothBorderRadius(
  //   cornerRadius: 16,
  //   cornerSmoothing: 1,
  // );
}

/// A class containing predefined shadow values for UI components.
@immutable
class Shadows {
  /// Soft shadow.
  static const soft = [
    BoxShadow(
      color: Color(0x0C000000),
      offset: Offset(0, 2),
      blurRadius: _blurRadius,
      spreadRadius: _spreadRadius,
    ),
  ];

  /// Medium shadow.
  static const medium = [
    BoxShadow(
      color: Color(0x14000000),
      offset: Offset(0, 6),
      blurRadius: _blurRadius,
      spreadRadius: 4,
    ),
  ];

  /// Strong shadow.
  static const strong = [
    BoxShadow(
      color: Color(0x14000000),
      offset: Offset(0, 8),
      blurRadius: _blurRadius,
      spreadRadius: _spreadRadius,
    ),
    BoxShadow(color: Color(0x14000000), offset: Offset(0, -1), blurRadius: 12),
  ];

  /// Micro shadow.
  static final mic = [
    const BoxShadow(
      color: Color(0x4B360090),
      blurRadius: 8,
      offset: Offset(0, 5),
      spreadRadius: 1,
    ),
  ];

  static const double _blurRadius = 10;
  static const double _spreadRadius = 2;
}

/// A class containing predefined SizedBox widgets for common spacing values.
@immutable
class Spaces {
  // Horizontal Spaces
  /// A SizedBox with a width of 4 logical pixels.
  static const SizedBox horizontalMicro = SizedBox(width: 4);

  /// A SizedBox with a width of 8 logical pixels.
  static const SizedBox horizontalSmall = SizedBox(width: 8);

  /// A SizedBox with a width of 12 logical pixels.
  static const SizedBox horizontalMedium = SizedBox(width: 12);

  /// A SizedBox with a width of 16 logical pixels.
  static const SizedBox horizontalLarge = SizedBox(width: 16);

  /// A SizedBox with a width of 24 logical pixels.
  static const SizedBox horizontalXLarge = SizedBox(width: 24);

  /// A SizedBox with a width of 32 logical pixels.
  static const SizedBox horizontalXXLarge = SizedBox(width: 32);

  /// A SizedBox with a width of 48 logical pixels.
  static const SizedBox horizontalXXXLarge = SizedBox(width: 48);

  // Vertical Spaces
  /// A SizedBox with a height of 4 logical pixels.
  static const SizedBox verticalMicro = SizedBox(height: 4);

  /// A SizedBox with a height of 8 logical pixels.
  static const SizedBox verticalSmall = SizedBox(height: 8);

  /// A SizedBox with a height of 12 logical pixels.
  static const SizedBox verticalMedium = SizedBox(height: 12);

  /// A SizedBox with a height of 16 logical pixels.
  static const SizedBox verticalLarge = SizedBox(height: 16);

  /// A SizedBox with a height of 24 logical pixels.
  static const SizedBox verticalXLarge = SizedBox(height: 24);

  /// A SizedBox with a height of 32 logical pixels.
  static const SizedBox verticalXXLarge = SizedBox(height: 32);

  /// A SizedBox with a height of 48 logical pixels.
  static const SizedBox verticalXXXLarge = SizedBox(height: 48);
}

/// A class containing predefined sizes for toolbars.
@immutable
class ToolBarHeights {
  /// Height of the home toolbar.
  static const Size home = Size.fromHeight(64);

  /// Height of the primary toolbar.
  static const Size primary = Size.fromHeight(120);

  /// Height of the secondary toolbar.
  static const Size secondary = Size.fromHeight(56);
}
