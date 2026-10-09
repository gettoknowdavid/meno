import 'package:flutter/material.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:pinput/pinput.dart';

/// A customizable OTP (One-Time Password) input field.
///
/// This widget creates a field for entering OTP codes, which is typically used
/// for authentication and verification processes. It supports features such as
/// a specific length for the OTP, obscure text for privacy, and custom
/// validation.
///
/// To use this widget, configure the parameters according to your requirements.
/// For example:
/// ```dart
/// MOtpField(
///   length: 6,
///   onChanged: (value) {
///     // Handle OTP changes
///   },
///   validator: (value) {
///     // Validate OTP input
///     if (value == null || value.length < 6) {
///       return 'Enter a valid OTP';
///     }
///     return null;
///   },
/// );
/// ```
class MOtpField extends StatelessWidget {
  /// Creates an instance of [MOtpField].
  ///
  /// Parameters:
  /// - [key]: An optional key to identify the widget.
  /// - [controller]: An optional controller for managing the text input.
  /// - [enabled]: A boolean to control whether the field is enabled.
  /// Defaults to true.
  /// - [focusNode]: An optional focus node for managing focus.
  /// - [length]: The number of OTP fields to display. Defaults to 4.
  /// - [obscureText]: A boolean to control whether the text should be
  /// obscured. Defaults to false.
  /// - [onChanged]: A callback function that is called when the text changes.
  /// - [validator]: An optional function for validating the OTP input.
  const new({
    super.key,
    this.controller,
    this.enabled = true,
    this.focusNode,
    this.length = 4,
    this.obscureText = false,
    this.onChanged,
    this.validator,
  });

  /// An optional controller for managing the text input.
  final TextEditingController? controller;

  /// A boolean to control whether the field is enabled. Defaults to true.
  final bool enabled;

  /// An optional focus node for managing focus.
  final FocusNode? focusNode;

  /// The number of OTP fields to display. Defaults to 4.
  final int length;

  /// A boolean to control whether the text should be obscured.
  /// Defaults to false.
  final bool obscureText;

  /// A callback function that is called when the text changes.
  final ValueChanged<String?>? onChanged;

  /// An optional function for validating the OTP input.
  final FormFieldValidator<String>? validator;

  @override
  Widget build(BuildContext context) {
    final styles = MOtpFieldStyles.of(context);
    final textTheme = MTextTheme.of(context);
    final defaultPinTheme = PinTheme(
      constraints: const BoxConstraints(maxHeight: 88, maxWidth: 88),
      padding: const .all(24),
      decoration: BoxDecoration(
        color: styles.fillColor,
        border: styles.border,
        borderRadius: Corners.md,
      ),
      textStyle: textTheme.heading1Medium.copyWith(
        color: styles.textStyle.color,
      ),
    );

    return Pinput(
      autofocus: true,
      controller: controller,
      enabled: enabled,
      focusNode: focusNode,
      length: length,
      obscureText: obscureText,
      obscuringCharacter: '*',
      onChanged: onChanged,
      validator: validator,
      defaultPinTheme: defaultPinTheme,
      focusedPinTheme: defaultPinTheme.copyDecorationWith(
        color: styles.fillColor,
        border: styles.borderFocused,
        borderRadius: Corners.md,
      ),
      errorPinTheme: defaultPinTheme.copyDecorationWith(
        color: styles.fillColor,
        border: styles.borderError,
        borderRadius: Corners.md,
      ),
    );
  }
}
