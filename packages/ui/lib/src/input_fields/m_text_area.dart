import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// A customizable text area widget with extensive styling and functionality.
///
/// This widget wraps a [TextFormField] and provides additional customization
/// options such as label display, icons, and more suitable for multiline text
/// input.
///
/// To use this widget, provide the required parameters and customize as
/// needed. For example:
/// ```dart
/// MTextArea(
///   label: 'Description',
///   hint: 'Enter your description here',
///   maxLines: 5,
///   onChanged: (value) {
///     // Handle text change
///   },
///   validator: (value) {
///     // Validate input
///     if (value == null || value.isEmpty) {
///       return 'Description is required';
///     }
///     return null;
///   },
/// );
/// ```
class MTextArea extends StatefulWidget {
  /// Creates an instance of [MTextArea].
  ///
  /// Parameters:
  /// - [label]: The label for the text area.
  /// - [key]: An optional key to identify the widget.
  /// - [labelIcon]: An optional icon to display with the label.
  /// - [hint]: An optional hint text to display when the field is empty.
  /// - [enabled]: A boolean to control whether the text area is enabled.
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
  /// - [validator]: An optional function for validating the input.
  /// - [autovalidateMode]: An optional mode to control when validation should
  /// occur.
  /// - [required]: A boolean to indicate if the field is required. Defaults to
  /// false.
  /// - [textInputAction]: An optional action to display on the keyboard.
  const new({
    required this.label,
    super.key,
    this.labelIcon,
    this.hint,
    this.enabled = true,
    this.maxLines = 1,
    this.maxLength,
    this.controller,
    this.onChanged,
    this.onFieldSubmitted,
    this.initialValue,
    this.focusNode,
    this.keyboardType = TextInputType.text,
    this.validator,
    this.autovalidateMode,
    this.required = false,
    this.textInputAction,
  });

  /// The label for the text area.
  final String label;

  /// An optional icon to display with the label.
  final IconData? labelIcon;

  /// An optional hint text to display when the field is empty.
  final String? hint;

  /// A boolean to control whether the text area is enabled. Defaults to true.
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

  /// An optional function for validating the input.
  final FormFieldValidator<String>? validator;

  /// An optional mode to control when validation should occur.
  final AutovalidateMode? autovalidateMode;

  /// A boolean to indicate if the field is required. Defaults to false.
  final bool required;

  /// An optional action to display on the keyboard.
  final TextInputAction? textInputAction;

  @override
  State<MTextArea> createState() => _MTextAreaState();
}

class _MTextAreaState extends State<MTextArea> {
  final _controllerNotifier = ValueNotifier<TextEditingController?>(null);

  late FocusNode _focus;
  bool hasFocus = false;

  @override
  Widget build(BuildContext context) {
    final styles = MTextFieldStyle.of(context);

    return ConstrainedBox(
      constraints: const BoxConstraints(maxHeight: 168),
      child: FormField<String?>(
        validator: widget.validator,
        autovalidateMode: widget.autovalidateMode,
        builder: (field) => Column(
          mainAxisSize: .min,
          crossAxisAlignment: .start,
          children: [
            Row(
              mainAxisAlignment: .spaceBetween,
              children: [
                MInputLabel(
                  widget.label,
                  icon: widget.labelIcon,
                  required: widget.required,
                ),
                MInputCounter(
                  maxLength: widget.maxLength ?? 244,
                  currentLength: _controllerNotifier.value?.text.length ?? 0,
                  enabled: widget.enabled || field.hasError,
                ),
              ],
            ),
            Spaces.verticalSmall,
            LimitedBox(
              maxHeight: 136,
              child: _buildTextFormField(styles, field),
            ),
            if (field.errorText != null)
              Container(
                alignment: .centerLeft,
                height: 18,
                child: _buildErrorText(field, styles),
              ),
          ],
        ),
      ),
    );
  }

  @override
  void didUpdateWidget(covariant MTextArea oldWidget) {
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
    _controllerNotifier.value = widget.controller;
    _focus = widget.focusNode ?? FocusNode();
    _focus.addListener(_onFocusChange);
  }

  void _onFocusChange() => setState(() => hasFocus = _focus.hasFocus);

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
    FormFieldState<String?> field,
  ) {
    return TextFormField(
      autovalidateMode: widget.autovalidateMode,
      style: styles.textStyle,
      onFieldSubmitted: widget.onFieldSubmitted,
      initialValue: widget.initialValue,
      controller: widget.controller,
      focusNode: _focus,
      keyboardType: widget.keyboardType,
      textInputAction: widget.textInputAction,
      maxLength: 244,
      maxLengthEnforcement: .enforced,
      onChanged: widget.onChanged,
      obscuringCharacter: '*',
      maxLines: widget.maxLines,
      enabled: widget.enabled,
      cursorWidth: 1,
      cursorHeight: 18,
      decoration: InputDecoration(
        enabled: widget.enabled,
        hintText: widget.hint,
        hintStyle: styles.hintTextStyle,
        contentPadding: const .all(Insets.md),
        counter: const SizedBox(),
        fillColor: widget.enabled ? styles.fillColor : styles.fillColorDisabled,
        filled: true,
        border: field.hasError ? styles.borderError : styles.border,
        enabledBorder: field.hasError ? styles.borderError : styles.border,
        focusedBorder: field.hasError
            ? styles.borderError
            : styles.borderFocused,
        disabledBorder: styles.borderDisabled,
      ),
    );
  }
}
