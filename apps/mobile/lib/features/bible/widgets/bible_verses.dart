import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/bible/manager/bible_manager.dart';
import 'package:meno_mobile/features/bible/model/bible_rows.dart';
import 'package:meno_mobile/features/broadcast/manager/active_broadcast_manager.dart';
import 'package:meno_mobile/features/bible/widgets/verse_options_modal.dart';
import 'package:meno_mobile/features/bible/widgets/verse_widget.dart';
import 'package:meno_ui/meno_ui.dart';

class BibleVerses extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final manager = di<BibleManager>();
    final verses = watchValue<BibleManager, List<BibleVerseRow>>(
      (m) => m.verses,
    );
    final selected = watchValue<BibleManager, List<BibleVerseRow>>(
      (m) => m.selectedVerses,
    );

    return Stack(
      children: [
        ListView.separated(
          padding: const .only(bottom: 88),
          addAutomaticKeepAlives: false,
          addRepaintBoundaries: false,
          itemCount: verses.length,
          separatorBuilder: (_, _) => Spaces.verticalMedium,
          itemBuilder: (context, i) {
            final v = verses[i];
            final isSelected = selected.any((s) => s.verse == v.verse);
            return VerseWidget(
              verse: v,
              isSelected: isSelected,
              onTap: selected.isEmpty
                  ? () => VerseOptionsModal.show(context, v)
                  : () => manager.toggleVerseSelection(v),
              onLongPress: () => manager.toggleVerseSelection(v),
            );
          },
        ),
        if (selected.isNotEmpty)
          Positioned(
            left: 0,
            right: 0,
            bottom: 0,
            child: _SelectionActionBar(manager: manager),
          ),
      ],
    );
  }
}

class _SelectionActionBar extends StatelessWidget {
  const new({required this.manager});

  final BibleManager manager;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    final reference = manager.selectionReference;
    final text = manager.selectionText;

    return Container(
      padding: const .fromLTRB(16, 12, 8, 12),
      decoration: BoxDecoration(
        color: colors.surface,
        border: Border(top: BorderSide(color: colors.outlineVariant1)),
        boxShadow: [
          BoxShadow(
            color: colors.shadow.withValues(alpha: 0.08),
            blurRadius: 12,
            offset: const Offset(0, -4),
          ),
        ],
      ),
      child: Row(
        children: [
          Expanded(
            child: MText(
              reference,
              style: textTheme.captionMedium,
              maxLines: 1,
              overflow: .ellipsis,
            ),
          ),
          IconButton(
            icon: const Icon(MIcons.copy_06),
            tooltip: 'Copy',
            onPressed: () => _copy(context, reference, text),
          ),
          IconButton(
            icon: const Icon(MIcons.send),
            tooltip: 'Send to chat',
            onPressed: () => _sendToChat(context, reference, text),
          ),
          IconButton(
            icon: const Icon(MIcons.x_close),
            tooltip: 'Clear selection',
            onPressed: manager.clearSelection,
          ),
        ],
      ),
    );
  }

  Future<void> _copy(BuildContext ctx, String reference, String text) async {
    await Clipboard.setData(ClipboardData(text: text));
    manager.clearSelection();
    if (ctx.mounted) {
      ScaffoldMessenger.of(ctx)
          .showSnackBar(SnackBar(content: Text('Copied: $reference')));
    }
  }

  /// Puts the selection in the room's composer and sends it.
  ///
  /// The chat belongs to the live broadcast, not to the container: outside a
  /// room there is no composer to write into, so the selection goes to the
  /// clipboard instead of disappearing.
  void _sendToChat(BuildContext context, String reference, String text) {
    final chat = di.isRegistered<ActiveBroadcastManager>()
        ? di<ActiveBroadcastManager>().session?.chat
        : null;

    if (chat == null) {
      unawaited(_copy(context, reference, text));
      return;
    }

    chat.onContentChange(text);
    chat.sendMessage.run();
    manager.clearSelection();

    if (context.mounted) {
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text('Sent: $reference')));
    }
  }
}
