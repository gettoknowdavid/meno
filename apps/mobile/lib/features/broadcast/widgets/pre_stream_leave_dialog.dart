import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

/// Asks whether to leave the broadcast this account is in before joining
/// another one.
///
/// It lives in its own file, built only from `material_ui`, because the app's
/// dialogs are drawn by that library's `showDialog`: routing to Flutter's copy
/// would ask for localizations the app never installs.
class PreStreamLeaveDialog extends StatelessWidget {
  const new _() : super(key: null);

  /// True when the person chose to leave the current broadcast.
  static Future<bool?> show(BuildContext context) {
    return showDialog<bool>(
      context: context,
      builder: (_) => const PreStreamLeaveDialog._(),
    );
  }

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return AlertDialog(
      title: MText(
        'You are already in a broadcast',
        style: textTheme.heading2Regular,
      ),
      contentPadding: const .all(24),
      content: MText(
        'Leave the current broadcast before joining another one?',
        style: textTheme.captionRegular,
      ),
      actions: [
        SizedBox.fromSize(
          size: const Size(85, 40),
          child: MTextButton(
            label: 'Cancel',
            onPressed: () => Navigator.pop(context, false),
            style: TextButton.styleFrom(
              foregroundColor: colors.onDisabled.withValues(alpha: 0.5),
            ),
          ),
        ),
        SizedBox(
          height: 40,
          child: MDangerButton(
            label: 'Leave and join',
            onPressed: () => Navigator.pop(context, true),
          ),
        ),
      ],
    );
  }
}
