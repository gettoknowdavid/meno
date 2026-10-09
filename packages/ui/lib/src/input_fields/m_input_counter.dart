import 'package:flutter/material.dart';
import 'package:meno_ui/meno_ui.dart';

/// A widget that displays a character counter for an input field.
///
/// This widget shows the current length of the input and the maximum allowed
/// length. It is typically used to provide feedback on how many characters the
/// user has entered in relation to the maximum allowed length.
///
/// To use this widget, pass the maximum length and the current length of the
/// input. Optionally, you can also specify if the counter should be enabled
/// or not.
///
/// Example usage:
/// ```dart
/// MInputCounter(
///   maxLength: 100,
///   currentLength: 45,
/// );
/// ```
class MInputCounter extends StatelessWidget {
  /// Creates an instance of [MInputCounter].
  ///
  /// Parameters:
  /// - [maxLength]: The maximum number of characters allowed.
  /// - [currentLength]: The current number of characters entered.
  /// - [key]: An optional key to identify the widget.
  /// - [enabled]: A boolean indicating whether the counter is enabled.
  /// Defaults to true.
  const new({
    required this.maxLength,
    required this.currentLength,
    super.key,
    this.enabled = true,
  });

  /// The maximum number of characters allowed.
  final int maxLength;

  /// The current number of characters entered.
  final int currentLength;

  /// A boolean indicating whether the counter is enabled. Defaults to true.
  final bool enabled;

  @override
  Widget build(BuildContext context) {
    final styles = MTextFieldStyle.of(context);

    final backgroundColor = enabled
        ? styles.counterBgColor
        : styles.counterBgColorDisabled;

    final textColor = enabled
        ? styles.counterTextColor
        : styles.counterTextColorDisabled;

    return Container(
      constraints: const BoxConstraints(minWidth: 50, maxHeight: 24),
      alignment: .center,
      padding: const .symmetric(horizontal: 8, vertical: 4),
      decoration: BoxDecoration(
        color: backgroundColor,
        borderRadius: Corners.sm,
      ),
      child: MText(
        '$currentLength/$maxLength',
        textAlign: .right,
        color: textColor,
        style: styles.counterTextStyle,
      ),
    );
  }
}
