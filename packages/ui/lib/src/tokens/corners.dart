import 'package:material_ui/material_ui.dart';

/// The corner-radius scale shared by the whole design system.
///
/// Every rounded surface should draw its radius from this scale rather than
/// introducing a one-off value.
@immutable
final class Corners {
  /// Prevents instantiation: this type is a namespace for the scale.
  const new _();

  /// Micro corner radius (4 units).
  static const BorderRadius xs = .all(.circular(4));

  /// Small corner radius (8 units).
  static const BorderRadius sm = .all(.circular(8));

  /// Medium corner radius (12 units).
  static const BorderRadius md = .all(.circular(12));

  /// Large corner radius (16 units).
  static const BorderRadius lg = .all(.circular(16));

  /// Extra-large corner radius (24 units).
  static const BorderRadius xl = .all(.circular(24));

  /// Extra-extra-large corner radius (32 units).
  static const BorderRadius xxl = .all(.circular(32));

  /// Extra-extra-extra-large corner radius (48 units).
  static const BorderRadius xxxl = .all(.circular(48));

  /// Circular corner radius (555 units, typically used for full circles).
  static const BorderRadius circle = .all(.circular(555));

  /// Superelliptical corner used by live cards and rounded surfaces.
  ///
  /// Uses Flutter's built-in [RoundedSuperellipseBorder] so cards get the
  /// same pill shape as before without an extra dependency.
  static const RoundedSuperellipseBorder squircleLg = RoundedSuperellipseBorder(
    borderRadius: .all(.circular(28)),
  );
}
