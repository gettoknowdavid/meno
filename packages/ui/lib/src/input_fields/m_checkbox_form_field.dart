import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/text/m_text.dart';
import 'package:meno_ui/src/theme/extensions/m_color_scheme.dart';
import 'package:meno_ui/src/theme/extensions/m_text_field_style.dart';
import 'package:meno_ui/src/theme/extensions/m_text_theme.dart';
import 'package:meno_ui/src/tokens/spacing.dart';

/// A checkbox and its label as one field.
///
/// The whole row is the tap target, not just the 20-pixel box: "By continuing
/// you agree to our Terms of Service" is a sentence a user taps, and a control
/// that only responds to the square next to it is a control users miss. The
/// label wraps across lines rather than overflowing, so a long statement stays
/// readable at any width.
///
/// [actionLabel] is the phrase inside the sentence that does something else —
/// opening a document, usually. It keeps its own tap target, so tapping it does
/// not also toggle the box; leave [onAction] null to render it as plain text.
///
/// The box itself carries no colour of its own: it reads the ambient
/// `CheckboxThemeData` that `MGlobalStyles` installs.
///
/// Example usage:
/// ```dart
/// MCheckboxFormField(
///   label: 'By continuing you agree to our ',
///   actionLabel: 'Terms of Service',
///   onAction: openTerms,
///   value: accepted,
///   onChanged: (value) => accepted = value,
/// )
/// ```
class MCheckboxFormField extends StatelessWidget {
  /// Creates a checkbox field.
  ///
  /// [label] is the statement beside the box and [value] the current state,
  /// owned by the caller: this widget never keeps a selection of its own.
  /// [onChanged] is called with the *next* value, not the current one.
  const new({
    required this.label,
    required this.value,
    required this.onChanged,
    super.key,
    this.actionLabel,
    this.onAction,
    this.enabled = true,
    this.validator,
    this.autovalidateMode,
    this.spacing = Insets.sm,
  });

  /// The statement beside the box.
  final String label;

  /// The tappable phrase inside [label], if the statement has an action.
  final String? actionLabel;

  /// Called when [actionLabel] is tapped. A null callback renders the phrase as
  /// plain text, which is how the sentence is shown while the action is
  /// unavailable.
  final VoidCallback? onAction;

  /// Whether the box is selected.
  final bool value;

  /// Called with the next value when the row — or the box — is tapped.
  final ValueChanged<bool> onChanged;

  /// Whether the field responds to taps.
  final bool enabled;

  /// Validates the selection, as a [FormField] does.
  final FormFieldValidator<bool>? validator;

  /// When [validator] runs.
  final AutovalidateMode? autovalidateMode;

  /// The gap between the box and the statement.
  final double spacing;

  @override
  Widget build(BuildContext context) {
    final styles = MTextFieldStyle.of(context);

    return FormField<bool>(
      initialValue: value,
      validator: validator,
      autovalidateMode: autovalidateMode,
      builder: (field) {
        void toggle() {
          if (!enabled) return;
          // The field keeps its own copy so a `validator` runs on the tap that
          // changed it; the caller's `value` is what is rendered.
          field.didChange(!value);
          onChanged(!value);
        }

        return Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            InkWell(
              onTap: enabled ? toggle : null,
              child: Row(
                children: [
                  // The theme owns the size and the mark; this only keeps the
                  // box from stretching to the row's full height.
                  Checkbox(
                    value: value,
                    onChanged: enabled ? (_) => toggle() : null,
                  ),
                  SizedBox(width: spacing),
                  Expanded(child: _label(context)),
                ],
              ),
            ),
            if (field.errorText != null) ...[
              Spaces.verticalMicro,
              MText(
                field.errorText!,
                style: styles.errorTextStyle,
                color: styles.errorColor,
              ),
            ],
          ],
        );
      },
    );
  }

  /// The statement, with [actionLabel] as its own tap target when it has one.
  ///
  /// A `Wrap` rather than a `Row`: a statement that does not fit moves the
  /// action phrase to the next line instead of clipping it.
  Widget _label(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);
    final style = textTheme.captionMedium;

    return Wrap(
      crossAxisAlignment: WrapCrossAlignment.center,
      children: [
        MText(label, style: style),
        if (actionLabel case final action?) ...[
          Spaces.horizontalMicro,
          InkWell(
            onTap: enabled ? onAction : null,
            child: MText(action, style: style, color: colors.primary),
          ),
        ],
      ],
    );
  }
}
