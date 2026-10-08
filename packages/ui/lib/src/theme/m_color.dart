// ignore_for_file: public_member_api_docs

import 'package:flutter/material.dart';

/// Standard colors for the Meno app
class MColor extends Color {
  const MColor._(super.value);

  // Basic
  static const MColor n0 = MColor._(0xFF000000);
  static const MColor black = MColor._(0xFF020D1C);
  static const MColor white = MColor._(0xFFFFFFFF);
  static const MColor transparent = MColor._(0x00000000);
  static const MColor shadow = MColor._(0x4B360090);
  static const MColor counter = MColor._(0xFF2A213C);
  static const MColor newBadgeLight = MColor._(0xFFF4EEFF);
  static const MColor newBadgeDark = MColor._(0xFF2A213C);
  static const MColor tint = MColor._(0xFFF6F7FB);

  // Primary
  static const MColor primary50 = MColor._(0xFFF4EEFF);
  static const MColor primary60 = MColor._(0xFFE9DDFF);
  static const MColor primary75 = MColor._(0xFFCEB2FF);
  static const MColor primary100 = MColor._(0xFF792BFF);
  static const MColor primary200 = MColor._(0xFF4D00CE);
  static const MColor primary300 = MColor._(0xFF360090);
  static const MColor primary400 = MColor._(0xFF260065);
  static const MColor primary500 = MColor._(0xFF200058);
  static const MColor primary600 = MColor._(0xFF16003D);
  static const MColor primary700 = MColor._(0xFF0B001F);

  // Primary - Alt
  static const MColor primaryAlt = MColor._(0xFF2B213C);

  // Secondary
  static const MColor secondary50 = MColor._(0xFFFFEAE6);
  static const MColor secondary75 = MColor._(0xFFFFA896);
  static const MColor secondary100 = MColor._(0xFFFF836B);
  static const MColor secondary200 = MColor._(0xFFFF4E2B);
  static const MColor secondary300 = MColor._(0xFFFF2A00);
  static const MColor secondary400 = MColor._(0xFFB31D00);
  static const MColor secondary500 = MColor._(0xFF9C1A00);
  static const MColor secondary600 = MColor._(0xFF661100);

  // Success
  static const MColor success50 = MColor._(0xFFEBF6EE);
  static const MColor success75 = MColor._(0xFFADDBB8);
  static const MColor success100 = MColor._(0xFF8CCD9A);
  static const MColor success200 = MColor._(0xFF5AB76F);
  static const MColor success300 = MColor._(0xFF38A851);
  static const MColor success400 = MColor._(0xFF277639);
  static const MColor success500 = MColor._(0xFF226631);
  static const MColor success600 = MColor._(0xFF13391C);

  // Yellow
  static const MColor yellow50 = MColor._(0xFFFCF6E8);
  static const MColor yellow75 = MColor._(0xFFF8E2B9);
  static const MColor yellow100 = MColor._(0xFFF6D69C);
  static const MColor yellow200 = MColor._(0xFFF2C471);
  static const MColor yellow300 = MColor._(0xFFEFB854);
  static const MColor yellow400 = MColor._(0xFFA7813B);
  static const MColor yellow500 = MColor._(0xFF927033);
  static const MColor yellow600 = MColor._(0xFF4C3A1A);

  // Blue
  static const MColor blue50 = MColor._(0xFFEAF2FD);
  static const MColor blue75 = MColor._(0xFFA8C8F7);
  static const MColor blue100 = MColor._(0xFF85B1F4);
  static const MColor blue200 = MColor._(0xFF5090EF);
  static const MColor blue300 = MColor._(0xFF2C79EC);
  static const MColor blue400 = MColor._(0xFF1F55AF);
  static const MColor blue500 = MColor._(0xFF1B4A90);
  static const MColor blue600 = MColor._(0xFF102C56);
  static const MColor blue700 = MColor._(0xFF0A1A34);

  // Error
  static const MColor error50 = MColor._(0xFFFDEAE7);
  static const MColor error75 = MColor._(0xFFF6A89B);
  static const MColor error100 = MColor._(0xFFF28472);
  static const MColor error200 = MColor._(0xFFED4F35);
  static const MColor error300 = MColor._(0xFFE92B0C);
  static const MColor error400 = MColor._(0xFFA31E08);
  static const MColor error500 = MColor._(0xFF8E1A07);
  static const MColor error600 = MColor._(0xFF5C1105);

  // Grey
  static const MColor grey10 = MColor._(0xFFFAFBFB);
  static const MColor grey20 = MColor._(0xFFF5F6F7);
  static const MColor grey30 = MColor._(0xFFEBEDF0);
  static const MColor grey40 = MColor._(0xFFDFE2E6);
  static const MColor grey50 = MColor._(0xFFC2C7D0);
  static const MColor grey60 = MColor._(0xFFB3B9C4);
  static const MColor grey70 = MColor._(0xFFA6AEBB);
  static const MColor grey80 = MColor._(0xFF98A1B0);
  static const MColor grey90 = MColor._(0xFF8993A4);
  static const MColor grey100 = MColor._(0xFF7A8699);
  static const MColor grey200 = MColor._(0xFF6B788E);
  static const MColor grey300 = MColor._(0xFF5D6B82);
  static const MColor grey400 = MColor._(0xFF505F79);
  static const MColor grey500 = MColor._(0xFF42526D);
  static const MColor grey600 = MColor._(0xFF354764);
  static const MColor grey700 = MColor._(0xFF243757);
  static const MColor grey800 = MColor._(0xFF15294B);
  static const MColor grey900 = MColor._(0xFF091E42);

  // Decorative/Yellow
  static const MColor decorativeYellow50 = MColor._(0xFFFFFFDF);
  static const MColor decorativeYellow75 = MColor._(0xFFFFFF2C);
  static const MColor decorativeYellow100 = MColor._(0xFFFFFF01);
  static const MColor decorativeYellow200 = MColor._(0xFFFFDC4D);

  /// Linearly interpolate between two MColor values.
  ///
  /// The [t] parameter represents the interpolation factor. A value of 0.0
  /// returns the [a] color, a value of 1.0 returns the [b] color, and values
  /// in between represent a linear interpolation between the two colors.
  static MColor? lerp(MColor? a, MColor? b, double t) {
    if (b == null) {
      if (a == null) {
        return null;
      } else {
        return _scaleAlpha(a, 1.0 - t);
      }
    } else {
      if (a == null) {
        return _scaleAlpha(b, t);
      } else {
        return MColor._(
          Color.from(
            alpha: _clampInt(_lerpInt(a.a, b.a, t), 0, 255),
            red: _clampInt(_lerpInt(a.r, b.r, t), 0, 255),
            green: _clampInt(_lerpInt(a.g, b.g, t), 0, 255),
            blue: _clampInt(_lerpInt(a.b, b.b, t), 0, 255),
          ).toARGB32(),
        );
      }
    }
  }
}

/// Linearly interpolate between two integers.
///
/// Same as lerpDouble but specialized for non-null `int` type.
double _lerpInt(num a, num b, double t) {
  return a + (b - a) * t;
}

/// Same as [num.clamp] but specialized for non-null [int].
double _clampInt(double value, double min, double max) {
  assert(min <= max, 'max must be greater or equals to the min');
  if (value < min) {
    return min;
  } else if (value > max) {
    return max;
  } else {
    return value;
  }
}

MColor _scaleAlpha(MColor a, double factor) {
  return MColor._(
    a.withAlpha((a.a * factor).round().clamp(0, 255)).toARGB32(),
  );
}

extension ColorEx on Color {
  int get intAlpha => _floatToInt8(a);
  int get intRed => _floatToInt8(r);
  int get intGreen => _floatToInt8(g);
  int get intBlue => _floatToInt8(b);

  int _floatToInt8(double x) {
    return (x * 255.0).round() & 0xff;
  }

  int toARGB32() {
    return _floatToInt8(a) << 24 |
        _floatToInt8(r) << 16 |
        _floatToInt8(g) << 8 |
        _floatToInt8(b) << 0;
  }
}
