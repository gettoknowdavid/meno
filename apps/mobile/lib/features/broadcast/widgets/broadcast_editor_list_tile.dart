import 'package:flutter/material.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:meno_ui/src/list_tiles/m_info_list_tile.dart';

class BroadcastEditorListTile extends StatelessWidget {
  const new({
    required this.leadingText,
    required this.subtitleText,
    super.key,
    this.trailing,
  });

  final String leadingText;
  final String subtitleText;
  final Widget? trailing;

  @override
  Widget build(BuildContext context) {
    return MInfoListTile(
      leadingText: leadingText,
      subtitleText: subtitleText,
      trailing: trailing,
    );
  }
}
