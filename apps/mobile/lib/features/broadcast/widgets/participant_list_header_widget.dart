import 'package:flutter/material.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/features/broadcast/manager/participants_manager.dart';
import 'package:meno_mobile/features/broadcast/widgets/participants_modal.dart';
import 'package:meno_ui/meno_ui.dart';

class ParticipantListHeaderWidget extends StatelessWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const .symmetric(horizontal: Insets.lg),
      height: 34,
      child: Row(
        spacing: Insets.sm,
        children: [
          const Expanded(child: _ParticipantCountWidget()),
          ExpandButton(onTap: () => ParticipantsModal.show(context)),
        ],
      ),
    );
  }
}

class _ParticipantCountWidget extends WatchingWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final totalCount = watchValue<ParticipantsManager, int>(
      (m) => m.totalCount,
    );
    return Row(
      children: [
        Icon(MIcons.hearing, size: 18, color: colors.onDisabled),
        const MSpacer.h(Insets.xs),
        MText.caption(totalCount.toString(), color: colors.onDisabled),
      ],
    );
  }
}
