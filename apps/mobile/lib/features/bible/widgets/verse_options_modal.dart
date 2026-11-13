import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/bible/model/bible_rows.dart';
import 'package:meno_mobile/features/broadcast/manager/active_broadcast_manager.dart';
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
              _sendToChat(context);
              Navigator.of(context, rootNavigator: true).pop();
            },
          ),
          Spaces.verticalLarge,
          MModalListTile(
            leading: const Icon(MIcons.copy_06),
            title: 'Copy verse',
            onTap: () {
              _copy(context);
              Navigator.of(context, rootNavigator: true).pop();
            },
          ),
          Spaces.verticalLarge,
        ],
      ),
    );
  }

  /// Puts the verse in the room's composer and sends it.
  ///
  /// The chat belongs to the live broadcast, not to the container: outside a
  /// room there is no composer to write into, so the verse goes to the
  /// clipboard instead of disappearing.
  void _sendToChat(BuildContext context) {
    // The live owner is registered inside the account scope, so it is absent
    // both signed out and between rooms.
    final chat = di.isRegistered<ActiveBroadcastManager>()
        ? di<ActiveBroadcastManager>().session?.chat
        : null;

    if (chat == null) {
      _copy(context);
      return;
    }

    chat.onContentChange(_fullText);
    chat.sendMessage.run();

    context.showSuccessToast('Sent: $_reference');
  }

  void _copy(BuildContext context) {
    unawaited(Clipboard.setData(ClipboardData(text: _fullText)));
    context.showSuccessToast('Copied: $_reference');
  }
}
