import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/theme/extensions/m_color_scheme.dart';
import 'package:meno_ui/src/tokens/spacing.dart';

/// A theme extension for customizing the modal elements in the
/// Meno design system.
///
/// This class defines various styles for modal elements such as
/// BottomSheets and modals.
///
/// Example usage:
/// ```dart
/// final modalStyles = MModalStyles.of(context);
/// ```

class MModalStyles extends ThemeExtension<MModalStyles> {
  /// Creates a new instance of [MModalStyles].
  ///
  /// The constructor allows you to specify custom styles for modal elements.
  ///
  /// - [dragHandleColor]: The color of the drag handle.
  /// - [backgroundColor]: The background color of the modal.
  /// - [modalBackgroundColor]: The background color of the modal when it is in
  /// a modal state.
  const new({
    required this.dragHandleColor,
    required this.backgroundColor,
    required this.modalBackgroundColor,
  });

  /// Creates a default [MModalStyles] based on the given [MColorScheme].
  ///
  /// This factory constructor initializes the modal styles using the
  /// provided color scheme.
  ///
  /// - [colors]: The color scheme to use for the modal styles.
  ///
  /// Returns a new [MModalStyles] instance with the default styles applied.
  factory $default(MColorScheme colors) {
    return MModalStyles(
      dragHandleColor: colors.outlineVariant1,
      backgroundColor: colors.background,
      modalBackgroundColor: colors.background,
    );
  }

  /// The color of the drag handle.
  final Color dragHandleColor;

  /// The background color of the modal.
  final Color backgroundColor;

  /// The background color of the modal when it is in a modal state.
  final Color modalBackgroundColor;

  /// Returns the [MModalStyles] extension from the closest [Theme].
  static MModalStyles of(BuildContext context) {
    return Theme.of(context).extension<MModalStyles>()!;
  }

  /// Provides the [BottomSheetThemeData] based on the current modal styles.
  ///
  /// This getter constructs a [BottomSheetThemeData] using the properties
  /// defined in the current instance of [MModalStyles].
  ///
  /// Returns a [BottomSheetThemeData] instance with the styles applied.
  BottomSheetThemeData get bottomSheetTheme {
    return BottomSheetThemeData(
      dragHandleSize: const Size(Insets.xxl, Insets.xs),
      showDragHandle: true,
      dragHandleColor: dragHandleColor,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(32)),
      ),
      backgroundColor: backgroundColor,
      modalBackgroundColor: modalBackgroundColor,
    );
  }

  @override
  ThemeExtension<MModalStyles> copyWith({
    Color? dragHandleColor,
    Color? backgroundColor,
    Color? modalBackgroundColor,
  }) {
    return MModalStyles(
      dragHandleColor: dragHandleColor ?? this.dragHandleColor,
      backgroundColor: backgroundColor ?? this.backgroundColor,
      modalBackgroundColor: modalBackgroundColor ?? this.modalBackgroundColor,
    );
  }

  @override
  ThemeExtension<MModalStyles> lerp(
    ThemeExtension<MModalStyles>? other,
    double t,
  ) {
    if (other is! MModalStyles) return this;
    return MModalStyles(
      dragHandleColor: Color.lerp(dragHandleColor, other.dragHandleColor, t)!,
      backgroundColor: Color.lerp(backgroundColor, other.backgroundColor, t)!,
      modalBackgroundColor: Color.lerp(
        modalBackgroundColor,
        other.modalBackgroundColor,
        t,
      )!,
    );
  }
}
