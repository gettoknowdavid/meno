import 'dart:async';

import 'package:flutter/services.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/bible/model/bible_rows.dart';
import 'package:meno_ui/meno_ui.dart';

class VerseOptionsModal extends StatelessWidget {
  const new _({required this.verse}) : super(key: null);

  final BibleVerseRow verse;

  static Future<void> show(BuildContext context, BibleVerseRow verse) {
    return showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      useRootNavigator: true,
      builder: (_) => VerseOptionsModal._(verse: verse),
    );
  }

  // "Genesis 1:1"
  String get _reference => '${verse.bookName} ${verse.chapter}:${verse.verse}';

  // "Genesis 1:1\nIn the beginning God created…"
  String get _fullText => '$_reference\n${verse.text}';

  @override
  Widget build(BuildContext context) {
    return MModal(
      title: _reference,
      builder: (context) => Column(
        mainAxisSize: .min,
        children: [
          MModalListTile(
            leading: const Icon(MIcons.send),
            title: 'Send to chat',
            onTap: () {
              Navigator.of(context, rootNavigator: true).pop();
              _sendToChat(context);
            },
          ),
          Spaces.verticalLarge,
          MModalListTile(
            leading: const Icon(MIcons.copy_06),
            title: 'Copy verse',
            onTap: () {
              Navigator.of(context, rootNavigator: true).pop();
              _copy(context);
            },
          ),
          Spaces.verticalLarge,
        ],
      ),
    );
  }

  void _sendToChat(BuildContext context) {
    // try {
    //   final chat = di<ChatManager>();
    //   chat.onContentChange(_fullText);
    //   chat.sendMessage.run();
    //   if (context.mounted) context.showSnackBar('Sent: $_reference');
    // } catch (_) {
    //   // TODO(gettoknowdavid): Handle sending to chat.
    //   _copy(context);
    // }
  }

  void _copy(BuildContext context) {
    unawaited(Clipboard.setData(ClipboardData(text: _fullText)));
    // if (context.mounted) context.showSnackBar('Copied: $_reference');
  }
}
