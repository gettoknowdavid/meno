import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/settings/model/_model.dart';
import 'package:meno_ui/meno_ui.dart';

/// Shows a legal document that ships with the app.
///
/// The text lives in `model/legal_documents.dart`; this is only the reading
/// surface, so the same document can be a screen of its own (About) or a sheet
/// over a form (the register screen's terms link).
class LegalDocumentPage extends StatelessWidget {
  /// Creates a page showing [document].
  const new({required this.document, super.key});

  /// The document to render.
  final LegalDocument document;

  @override
  Widget build(BuildContext context) {
    return MScaffold(
      appBar: MAppBar.secondary(title: document.title, centerTitle: true),
      body: _LegalDocumentBody(document: document),
    );
  }
}

/// Opens [document] over the current screen and completes when it is closed.
///
/// This is how the register form reaches the terms: that screen runs before
/// there is an account, and the router's guard sends a private address to the
/// login page instead of showing it.
Future<void> showLegalDocumentSheet(
  BuildContext context,
  LegalDocument document,
) {
  final height = MediaQuery.sizeOf(context).height;

  return showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    useRootNavigator: true,
    builder: (_) => MModal(
      title: document.title,
      builder: (_) => SizedBox(
        // Tall enough to read a policy on a phone, short enough to leave the
        // screen it was opened from visible above it.
        height: height * 0.75,
        child: _LegalDocumentBody(document: document),
      ),
    ),
  );
}

/// The sections of [document], in order, with room to breathe between them.
class _LegalDocumentBody extends StatelessWidget {
  const new({required this.document});

  final LegalDocument document;

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);

    return ListView.separated(
      padding: const EdgeInsets.fromLTRB(
        Insets.lg,
        Insets.sm,
        Insets.lg,
        Insets.xxxl,
      ),
      itemCount: document.sections.length,
      separatorBuilder: (_, _) => Spaces.verticalXLarge,
      itemBuilder: (_, index) {
        final section = document.sections[index];
        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            MText(section.title, style: textTheme.bodyBold),
            Spaces.verticalSmall,
            MText(section.body, style: textTheme.bodyRegular),
          ],
        );
      },
    );
  }
}
