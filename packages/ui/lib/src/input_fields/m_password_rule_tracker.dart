import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/text/m_text.dart';
import 'package:meno_ui/src/theme/extensions/m_color_scheme.dart';
import 'package:meno_ui/src/theme/extensions/m_text_theme.dart';
import 'package:meno_ui/src/tokens/spacing.dart';

/// One rule a [MPasswordRuleTracker] reports on.
///
/// The design system deliberately does not know what the rules *are* — that is
/// a product policy that changes, and it differs per app. A caller supplies
/// the label, the one-word caption and whether the rule is satisfied.
@immutable
final class MPasswordRule {
  /// Creates a rule.
  ///
  /// [title] is the short form shown in the chip (e.g. `8+`), [caption] the
  /// word under it (e.g. `Characters`), and [isMet] whether the current input
  /// satisfies it.
  const new({
    required this.title,
    required this.caption,
    this.isMet = true,
  });

  /// The short form shown in the chip.
  final String title;

  /// The word under the title.
  final String caption;

  /// Whether the current input satisfies this rule.
  final bool isMet;
}

/// A password strength meter with one chip per rule.
///
/// The bar shows how much of the policy the current input satisfies and the
/// chips show which parts are still missing, so a user sees what is left to do
/// rather than only that something is wrong.
///
/// Set [touched] once anything has been typed. Until then the chips stay
/// neutral: colouring them red before the first keystroke reads as an error the
/// user did not make.
///
/// Example usage:
/// ```dart
/// MPasswordRuleTracker(
///   strength: 0.66,
///   touched: true,
///   rules: [
///     MPasswordRule(title: '8+', caption: 'Characters'),
///     MPasswordRule(title: 'A-Z', caption: 'Uppercase', isMet: false),
///   ],
/// )
/// ```
class MPasswordRuleTracker extends StatelessWidget {
  /// Creates a rule tracker.
  ///
  /// [strength] is 0–1, [rules] the chips to show, and [hint] the line above
  /// them.
  const new({
    required this.strength,
    required this.rules,
    super.key,
    this.touched = false,
    this.hint = 'Your password must include:',
  });

  /// How much of the policy is satisfied, from 0 to 1.
  final double strength;

  /// The rules to show, in the order they should appear.
  final List<MPasswordRule> rules;

  /// Whether anything has been typed yet.
  final bool touched;

  /// The line above the chips.
  final String hint;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);
    final label = touched ? _labelFor(strength) : '';

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        TweenAnimationBuilder<double>(
          tween: Tween<double>(begin: 0, end: strength.clamp(0, 1)),
          duration: const Duration(milliseconds: 300),
          curve: Curves.easeOut,
          builder: (context, value, _) => LinearProgressIndicator(value: value),
        ),
        Spaces.verticalMicro,
        Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: [
            MText(hint, style: textTheme.nanoMedium),
            if (label.isNotEmpty)
              MText(label, style: textTheme.nanoMedium, color: colors.primary),
          ],
        ),
        Spaces.verticalMedium,
        Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: [
            for (final rule in rules)
              _RuleChip(rule: rule, touched: touched),
          ],
        ),
      ],
    );
  }

  /// The word for [strength].
  static String _labelFor(double strength) {
    if (strength <= 0) return '';
    if (strength <= 0.4) return 'Weak';
    if (strength <= 0.8) return 'Good';
    return 'Strong';
  }
}

/// One rule chip: its short form over its caption, coloured by whether it is
/// satisfied.
class _RuleChip extends StatelessWidget {
  const new({required this.rule, required this.touched});

  final MPasswordRule rule;
  final bool touched;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    final color = !touched
        ? colors.onBackgroundVariant
        : rule.isMet
        ? colors.success
        : colors.error;

    return SizedBox(
      height: 38,
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          MText(rule.title, color: color, style: textTheme.subheadingMedium),
          MText(rule.caption, color: color, style: textTheme.nanoMedium),
        ],
      ),
    );
  }
}
