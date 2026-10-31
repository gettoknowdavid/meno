import 'package:flutter/material.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/features/broadcast/manager/active_broadcast_manager.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';
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
    // The roster belongs to the open session, not to the container: only
    // ActiveBroadcastManager is registered (audit Step 31), and it hands out
    // the room it owns. Asking GetIt for a ParticipantsManager threw here,
    // because nothing registers one — the session creates it and disposes it
    // with itself (Step 29).
    //
    // Watched so a session that becomes available after this first build is
    // picked up: the count lives in the session, and the phase is what changes
    // when there is one to read.
    watchValue<ActiveBroadcastManager, LiveSnapshot>(
      (manager) => manager.snapshot,
    );
    final roster = di<ActiveBroadcastManager>().session?.participants;
    if (roster == null) return const SizedBox.shrink();

    final colors = MColorScheme.of(context);
    final totalCount = watch(roster.totalCount).value;

    return Row(
      children: [
        Icon(MIcons.hearing, size: 18, color: colors.onDisabled),
        const MSpacer.h(Insets.xs),
        MText.caption(totalCount.toString(), color: colors.onDisabled),
      ],
    );
  }
}
