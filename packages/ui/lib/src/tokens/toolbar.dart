import 'package:flutter/widgets.dart';

/// The standard toolbar heights used by app shells.
@immutable
class ToolBarHeights {
  /// Prevents instantiation: this type is a namespace for the sizes.
  const new _();

  /// Height of the home toolbar.
  static const Size home = Size.fromHeight(64);

  /// Height of the primary toolbar.
  static const Size primary = Size.fromHeight(120);

  /// Height of the secondary toolbar.
  static const Size secondary = Size.fromHeight(56);
}
