import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/settings/model/_model.dart';
import 'package:meno_ui/meno_ui.dart';

/// Shows the support details, next to what a useful report contains.
///
/// Reached from About (a general "report an app problem") and from the
/// bottom of the FAQs (a question the list did not answer). Both need the same
/// address, the same advice about screenshots and the same selectable text, so
/// both open this — a second copy would drift from the first.
Future<void> showSupportSheet(BuildContext context) {
  final colors = MColorScheme.of(context);
  final textTheme = MTextTheme.of(context);

  return showModalBottomSheet<void>(
    context: context,
    showDragHandle: true,
    builder: (_) => SafeArea(
      child: Padding(
        padding: const .fromLTRB(
          Insets.lg,
          0,
          Insets.lg,
          Insets.lg,
        ),
        child: Column(
          mainAxisSize: .min,
          crossAxisAlignment: .stretch,
          children: [
            MText('Report an app problem', style: textTheme.subheadingBold),
            Spaces.verticalSmall,
            MText(
              'Tell us what happened, what you expected, and which device '
              'you are on. A screenshot helps us reproduce it.',
              style: textTheme.captionRegular,
              color: colors.onBackgroundVariant,
            ),
            Spaces.verticalLarge,
            MText('Email', style: textTheme.captionMedium),
            SelectableText(legalSupportEmail, style: textTheme.bodyRegular),
            Spaces.verticalMedium,
            MText('Phone', style: textTheme.captionMedium),
            SelectableText(legalSupportPhone, style: textTheme.bodyRegular),
          ],
        ),
      ),
    ),
  );
}
