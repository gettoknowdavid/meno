import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// A customizable text form field widget with extensive styling and
/// functionality.
///
/// This widget wraps a [TextFormField] and provides additional customization
/// options such as label display,
/// icons, validation, and more.
///
/// To use this widget, provide the required parameters and customize as needed.
/// For example:
/// ```dart
/// MTextFormField(
///   label: 'Username',
///   hint: 'Enter your username',
///   prefixIcon: Icons.person,
///   isPassword: false,
///   onChanged: (value) {
///     // Handle text change
///   },
///   validator: (value) {
///     // Validate input
///     if (value == null || value.isEmpty) {
///       return 'Username is required';
///     }
///     return null;
///   },
/// );
/// ```
class MTextFormField extends StatefulWidget {
  /// Creates an instance of [MTextFormField].
  ///
  /// Parameters:
  /// - [label]: The label for the text form field.
  /// - [key]: An optional key to identify the widget.
  /// - [showLabel]: A boolean to control whether the label is shown. Defaults
  /// to true.
  /// - [autoFocus]: A boolean to control whether the text field should
  /// auto-focus. Defaults to false.
  /// - [labelIcon]: An optional icon to display with the label.
  /// - [hint]: An optional hint text to display when the field is empty.
  /// - [prefixIcon]: An optional icon to display before the input field.
  /// - [suffixIcon]: An optional icon to display after the input field.
  /// - [enabled]: A boolean to control whether the text field is enabled.
  /// Defaults to true.
  /// - [maxLines]: The maximum number of lines to display. Defaults to 1.
  /// - [maxLength]: An optional maximum number of characters.
  /// - [controller]: An optional controller for managing the text.
  /// - [onChanged]: A callback function that is called when the text changes.
  /// - [onFieldSubmitted]: A callback function that is called when the field
  /// is submitted.
  /// - [initialValue]: An optional initial value for the field.
  /// - [focusNode]: An optional focus node for managing focus.
  /// - [keyboardType]: The type of keyboard to display. Defaults to
  /// [TextInputType.text].
  /// - [isPassword]: A boolean to control whether the field should obscure
  /// text for passwords. Defaults to false.
  /// - [validator]: An optional function for validating the input.
  /// - [autovalidateMode]: An optional mode to control when validation should
  /// occur.
  /// - [required]: A boolean to indicate if the field is required. Defaults to
  /// false.
  /// - [textInputAction]: An optional action to display on the keyboard.
  const new({
    required this.label,
    super.key,
    this.showLabel = true,
    this.autoFocus = false,
    this.labelIcon,
    this.hint,
    this.prefixIcon,
    this.suffixIcon,
    this.enabled = true,
    this.maxLines = 1,
    this.maxLength,
    this.controller,
    this.onChanged,
    this.onFieldSubmitted,
    this.initialValue,
    this.focusNode,
    this.keyboardType = .text,
    this.isPassword = false,
    this.validator,
    this.autovalidateMode,
    this.required = false,
    this.textInputAction,
  });

  /// The label for the text form field.
  final String label;

  /// A boolean to control whether the label is shown. Defaults to true.
  final bool showLabel;

  /// A boolean to control whether the text field should auto-focus. Defaults
  /// to false.
  final bool autoFocus;

  /// An optional icon to display with the label.
  final IconData? labelIcon;

  /// An optional hint text to display when the field is empty.
  final String? hint;

  /// An optional icon to display before the input field.
  final IconData? prefixIcon;

  /// An optional icon to display after the input field.
  final IconData? suffixIcon;

  /// A boolean to control whether the text field is enabled. Defaults to true.
  final bool enabled;

  /// The maximum number of lines to display. Defaults to 1.
  final int maxLines;

  /// An optional maximum number of characters.
  final int? maxLength;

  /// An optional controller for managing the text.
  final TextEditingController? controller;

  /// A callback function that is called when the text changes.
  final ValueChanged<String>? onChanged;

  /// A callback function that is called when the field is submitted.
  final ValueChanged<String>? onFieldSubmitted;

  /// An optional initial value for the field.
  final String? initialValue;

  /// An optional focus node for managing focus.
  final FocusNode? focusNode;

  /// The type of keyboard to display. Defaults to [TextInputType.text].
  final TextInputType keyboardType;

  /// A boolean to control whether the field should obscure text for passwords.
  /// Defaults to false.
  final bool isPassword;

  /// An optional function for validating the input.
  final FormFieldValidator<String>? validator;

  /// An optional mode to control when validation should occur.
  final AutovalidateMode? autovalidateMode;

  /// A boolean to indicate if the field is required. Defaults to false.
  final bool required;

  /// An optional action to display on the keyboard.
  final TextInputAction? textInputAction;

  @override
  State<MTextFormField> createState() => _MTextFormFieldState();
}

class _MTextFormFieldState extends State<MTextFormField> {
  final _controllerNotifier = ValueNotifier<TextEditingController?>(null);

  late FocusNode _focus;
  bool _hasFocus = false;

  int? currentLength;

  late bool obscureText;

  Widget? suffixWidget;
  Widget? prefixWidget;

  @override
  Widget build(BuildContext context) {
    final styles = MTextFieldStyle.of(context);

    final iconColor = _hasFocus ? styles.iconColor : MColor.grey80;

    if (widget.prefixIcon != null && widget.maxLines == 1) {
      prefixWidget = _PrefixIcon(icon: widget.prefixIcon!, color: iconColor);
    }

    if (widget.suffixIcon != null && widget.maxLines == 1) {
      suffixWidget = Icon(widget.suffixIcon, size: Insets.xl, color: iconColor);
    }

    if (widget.isPassword) {
      suffixWidget = InkWell(
        onTap: () => setState(() => obscureText = !obscureText),
        child: _EyeIcon(obscureText: obscureText),
      );
    }

    return ConstrainedBox(
      constraints: const BoxConstraints(maxHeight: 100),
      child: LayoutBuilder(
        builder: (context, constraints) => FormField<String?>(
          validator: widget.validator,
          autovalidateMode: widget.autovalidateMode,
          builder: (field) => Column(
            mainAxisSize: .min,
            crossAxisAlignment: .start,
            children: [
              if (widget.showLabel) ...[
                MInputLabel(
                  widget.label,
                  icon: widget.labelIcon,
                  required: widget.required,
                ),
                Spaces.verticalSmall,
              ],
              SizedBox(
                height: constraints.maxHeight * 0.48,
                child: _buildTextFormField(styles, iconColor, field),
              ),
              if (field.errorText != null) ...[
                Spaces.verticalSmall,
                Container(
                  alignment: .centerLeft,
                  height: constraints.maxHeight * 0.18,
                  child: _buildErrorText(field, styles),
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }

  @override
  void didUpdateWidget(covariant MTextFormField oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.controller != oldWidget.controller) {
      _controllerNotifier.value = widget.controller;
    }
  }

  @override
  void dispose() {
    if (widget.focusNode == null) {
      _focus.dispose();
    }
    super.dispose();
  }

  @override
  void initState() {
    super.initState();
    obscureText = widget.isPassword;
    _controllerNotifier.value = widget.controller;
    _focus = widget.focusNode ?? FocusNode();
    _focus.addListener(_onFocusChange);
  }

  void _onFocusChange() {
    setState(() => _hasFocus = _focus.hasFocus);
  }

  //
  // ignore: avoid_positional_boolean_parameters
  T resolveBorder<T>(bool isBoxed, T boxedValue, T underlinedValue) {
    return isBoxed ? boxedValue : underlinedValue;
  }

  MText _buildErrorText(FormFieldState<String?> field, MTextFieldStyle styles) {
    return MText(
      field.errorText!,
      style: styles.errorTextStyle,
      color: styles.errorColor,
    );
  }

  TextFormField _buildTextFormField(
    MTextFieldStyle styles,
    Color iconColor,
    FormFieldState<String?> field,
  ) {
    return TextFormField(
      autofocus: widget.autoFocus,
      autovalidateMode: widget.autovalidateMode,
      style: styles.textStyle,
      onFieldSubmitted: widget.onFieldSubmitted,
      initialValue: widget.initialValue,
      controller: widget.controller,
      focusNode: _focus,
      keyboardType: widget.keyboardType,
      obscureText: obscureText && widget.isPassword,
      textInputAction: widget.textInputAction,
      maxLength: widget.maxLength,
      maxLengthEnforcement: .enforced,
      onChanged: widget.onChanged,
      obscuringCharacter: '*',
      maxLines: widget.maxLines,
      enabled: widget.enabled,
      decoration: InputDecoration(
        enabled: widget.enabled,
        hintText: widget.hint,
        hintStyle: styles.hintTextStyle,
        contentPadding: const .symmetric(horizontal: Insets.md),
        fillColor: widget.enabled ? styles.fillColor : styles.fillColorDisabled,
        filled: true,
        iconColor: iconColor,
        prefixIconColor: iconColor,
        suffixIconColor: iconColor,
        border: field.hasError ? styles.borderError : styles.border,
        enabledBorder: field.hasError ? styles.borderError : styles.border,
        focusedBorder: field.hasError
            ? styles.borderError
            : styles.borderFocused,
        disabledBorder: styles.borderDisabled,
        prefixIconConstraints: .tight(const Size(44, 48)),
        prefixIcon: prefixWidget,
        suffixIconConstraints: .tight(const Size(44, 48)),
        suffixIcon: suffixWidget,
      ),
    );
  }
}

class _EyeIcon extends StatelessWidget {
  const new({required this.obscureText});
  final bool obscureText;

  @override
  Widget build(BuildContext context) {
    const iconSize = Insets.xl;
    return Container(
      height: iconSize,
      width: iconSize,
      margin: const .only(right: Insets.md),
      child: obscureText
          ? const Icon(MIcons.eye, size: iconSize)
          : const Icon(MIcons.eye_off, size: iconSize),
    );
  }
}

class _PrefixIcon extends StatelessWidget {
  const new({required this.icon, this.color});
  final IconData icon;
  final Color? color;

  @override
  Widget build(BuildContext context) {
    const iconSize = Insets.xl;
    return Container(
      width: iconSize,
      alignment: .centerRight,
      margin: const .only(right: 6),
      child: Icon(icon, size: iconSize, color: color),
    );
  }
}
