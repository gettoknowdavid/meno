import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/theme/extensions/m_color_scheme.dart';

/// Shows a design-system modal bottom sheet from any [BuildContext].
///
/// Wraps `showModalBottomSheet` with the design system's surface colour, drag
/// handle and safe-area handling so callers get a consistent sheet without
/// repeating the options. The `child` is wrapped in a [Material] so text and
/// ink in the sheet resolve against a themed surface.
extension MBottomSheetBuildContextX on BuildContext {
  /// Shows [child] in a modal bottom sheet and resolves to its result.
  ///
  /// [constraints] limits the sheet's size, [isScrollControlled] lets it grow
  /// past half the screen, [useRootNavigator] targets the root navigator and
  /// [isDismissible] / [enableDrag] control how the user closes it.
  Future<T?> showModal<T>(
    Widget child, {
    BoxConstraints? constraints,
    bool? isScrollControlled,
    bool? useRootNavigator,
    bool? isDismissible,
    bool? enableDrag,
  }) {
    return showModalBottomSheet<T?>(
      context: this,
      builder: (_) => Material(child: child),
      constraints: constraints,
      backgroundColor: MColorScheme.of(this).background,
      isScrollControlled: isScrollControlled ?? false,
      useRootNavigator: useRootNavigator ?? false,
      isDismissible: isDismissible ?? true,
      enableDrag: enableDrag ?? true,
      showDragHandle: true,
      useSafeArea: true,
    );
  }
}
