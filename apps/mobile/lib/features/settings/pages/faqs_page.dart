import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/settings/model/_model.dart';
import 'package:meno_mobile/features/settings/pages/support_sheet.dart';
import 'package:meno_ui/meno_ui.dart';

/// The help screen: every bundled question, searchable.
///
/// Lives under the private settings tree, so the router guard already protects
/// it and no route list needs changing.
///
/// Search is local state, not a manager: nothing here is shared with another
/// screen, nothing is fetched, and a manager for one text field would be a
/// lifetime to manage for no reason.
class FaqsPage extends StatefulWidget {
  /// Creates the FAQs page.
  const new({super.key});

  @override
  State<FaqsPage> createState() => _FaqsPageState();
}

class _FaqsPageState extends State<FaqsPage> {
  final _controller = TextEditingController();
  String _query = '';

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final sections = searchFaqSections(_query);
    final textTheme = MTextTheme.of(context);

    return MScaffold(
      appBar: MAppBar.secondary(title: 'FAQs', centerTitle: true),
      body: Column(
        crossAxisAlignment: .stretch,
        children: [
          SizedBox(
            height: 40,
            child: SearchBar(
              elevation: const WidgetStatePropertyAll(0),
              controller: _controller,
              onChanged: (value) => setState(() => _query = value),
              hintText: 'Search questions',
              hintStyle: WidgetStatePropertyAll(textTheme.captionRegular),
              padding: const WidgetStatePropertyAll(
                .symmetric(horizontal: Insets.md),
              ),
              leading: const Icon(MIcons.search, size: Insets.lg),
              shape: const WidgetStatePropertyAll(
                RoundedRectangleBorder(
                  side: BorderSide(color: Color(0xFFC2C7D0)),
                  borderRadius: Corners.sm,
                ),
              ),
              trailing: [
                if (_query.isNotEmpty)
                  IconButton(
                    icon: const Icon(Icons.clear, size: Insets.lg),
                    onPressed: () {
                      _controller.clear();
                      setState(() => _query = '');
                    },
                  ),
              ],
            ),
          ),
          Spaces.verticalLarge,
          Expanded(
            child: sections.isEmpty
                ? const _NoResults()
                : ListView(
                    padding: const .fromLTRB(0, 0, 0, Insets.xxxl),
                    children: [
                      for (final section in sections)
                        _Section(section: section),
                      const _SupportFooter(),
                    ],
                  ),
          ),
        ],
      ),
    );
  }
}

/// One heading and its questions, each collapsible.
class _Section extends StatelessWidget {
  const new({required this.section});

  final FaqSection section;

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);
    final colors = MColorScheme.of(context);

    return Column(
      crossAxisAlignment: .stretch,
      children: [
        Padding(
          padding: const .fromLTRB(0, Insets.lg, 0, Insets.xs),
          child: MText(
            section.title.toUpperCase(),
            style: textTheme.captionMedium,
            color: colors.onBackgroundVariant,
          ),
        ),
        for (final entry in section.entries)
          ExpansionTile(
            // Collapsed by default: a screen of 38 answers is a wall of text,
            // and the question is what someone scans for.
            title: MText(entry.question, style: textTheme.bodyMedium),
            tilePadding: const .symmetric(horizontal: Insets.lg),
            childrenPadding: const .fromLTRB(
              Insets.lg,
              0,
              Insets.lg,
              Insets.md,
            ),
            expandedCrossAxisAlignment: .stretch,
            shape: const Border(),
            collapsedShape: const Border(),
            children: [
              MText(
                entry.answer,
                style: textTheme.bodyRegular,
                color: colors.onBackgroundVariant,
              ),
            ],
          ),
      ],
    );
  }
}

/// What the screen shows when nothing matches.
///
/// A search that returns a blank page looks like a broken screen; naming the
/// query and offering the way out is the difference between "no answer" and
/// "no results".
class _NoResults extends StatelessWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);
    final colors = MColorScheme.of(context);

    return Padding(
      padding: const .all(Insets.lg),
      child: Column(
        mainAxisAlignment: .center,
        crossAxisAlignment: .stretch,
        children: [
          Icon(
            MIcons.help_circle,
            size: Insets.xxl,
            color: colors.onBackgroundVariant,
          ),
          Spaces.verticalMedium,
          MText(
            'No question matches that',
            style: textTheme.bodyBold,
            textAlign: .center,
          ),
          Spaces.verticalSmall,
          MText(
            'Try a shorter phrase — for example "password", "notification" or '
            '"record".',
            style: textTheme.captionRegular,
            color: colors.onBackgroundVariant,
            textAlign: .center,
          ),
        ],
      ),
    );
  }
}

/// The end of the list: the human way out when no answer fits.
class _SupportFooter extends StatelessWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);

    return Padding(
      padding: const .only(top: Insets.lg),
      child: Column(
        crossAxisAlignment: .stretch,
        children: [
          MSettingsSection(
            children: [
              MSettingsListTile(
                title: 'Still need help?',
                leadingIcon: MIcons.mail,
                trailing: MText(
                  'Contact support',
                  style: MTextTheme.of(context).captionMedium,
                  color: colors.primary,
                ),
                showDivider: false,
                onTap: () => showSupportSheet(context),
              ),
            ],
          ),
        ],
      ),
    );
  }
}
