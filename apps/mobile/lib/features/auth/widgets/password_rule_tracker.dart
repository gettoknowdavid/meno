import 'package:flutter/material.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/value_objects/_value_objects.dart';
import 'package:meno_mobile/features/auth/manager/register_form_manager.dart';
import 'package:meno_ui/meno_ui.dart';

class PasswordRulesTracker extends WatchingWidget {
  const new({super.key});

  static const List<ValueException<String>> _allRules = [
    PasswordTooShort(),
    PasswordMissingUppercase(),
    PasswordMissingLowercase(),
    PasswordMissingNumber(),
    PasswordMissingSpecialCharacter(),
  ];

  @override
  Widget build(BuildContext context) {
    final nanoStyle = MTextTheme.of(context).nanoMedium;

    final strength = watchValue<RegisterFormManager, double>(
      (manager) => manager.strength,
    );

    final strengthLabel = watchValue<RegisterFormManager, String>(
      (manager) => manager.passwordStrengthLabel,
    );

    return Column(
      crossAxisAlignment: .stretch,
      children: [
        AnimatedLinearProgressIndicator(value: strength),
        const SizedBox(height: Insets.xs),
        Row(
          mainAxisAlignment: .spaceBetween,
          children: [
            MText('Your password must include:', style: nanoStyle),
            if (strengthLabel.isNotEmpty)
              MText(strengthLabel, style: nanoStyle),
          ],
        ),
        const SizedBox(height: Insets.md),
        Row(
          mainAxisAlignment: .spaceBetween,
          children: _allRules.map(PasswordRuleWidget.new).toList(),
        ),
      ],
    );
  }
}

class PasswordRuleWidget extends WatchingWidget {
  const new(this.rule, {super.key});

  final ValueException<String> rule;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    final brokenRules =
        watchValue<RegisterFormManager, Set<ValueException<String>>?>(
          (manager) => manager.passwordRules,
        );

    final isBroken = brokenRules?.any((r) => r.runtimeType == rule.runtimeType);

    final color = brokenRules == null
        ? colors.onBackground
        : (isBroken ?? false)
        ? colors.error
        : colors.success;

    final title = rule.title;
    final subtitle = rule.subtitle;

    if (title == null || subtitle == null) return const SizedBox.shrink();

    return SizedBox(
      height: 38,
      child: Column(
        mainAxisAlignment: .center,
        children: [
          MText(title, color: color, style: textTheme.subheadingMedium),
          MText(subtitle, color: color, style: textTheme.nanoMedium),
        ],
      ),
    );
  }
}

// Smooth animated progress bar
class AnimatedLinearProgressIndicator extends StatelessWidget {
  const new({required this.value, super.key});

  final double value;

  @override
  Widget build(BuildContext context) {
    return TweenAnimationBuilder<double>(
      tween: Tween(begin: 0, end: value),
      duration: const Duration(milliseconds: 300),
      curve: Curves.easeOut,
      builder: (_, animated, _) => LinearProgressIndicator(value: animated),
    );
  }
}
