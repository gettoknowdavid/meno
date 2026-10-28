import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_ui/meno_ui.dart';

/// The way back into a broadcast this account is still in.
///
/// It sits in the authenticated chrome rather than inside one page, so it is
/// visible on every tab (audit §1.2). Everything it draws is one snapshot from
/// [ActiveBroadcastManager], which exists for as long as the account does —
/// there is no scope to probe and no second manager to merge.
///
/// The leave/end confirmation captures the values it needs *before* awaiting
/// the dialog and re-checks the snapshot afterwards: a dialog left open across
/// an account switch must not act on the session it was opened for (audit H02).
class LiveSessionBanner extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final snapshot = watchValue<ActiveBroadcastManager, LiveSnapshot>(
      (manager) => manager.snapshot,
    );

    final broadcast = snapshot.broadcast;
    if (broadcast == null ||
        snapshot.phase == .idle ||
        snapshot.phase == .ended) {
      return const SizedBox.shrink();
    }

    return _Banner(
      key: ValueKey('broadcastBanner-${broadcast.id.orEmpty}'),
      badgeTitle: _badgeLabel(snapshot.phase),
      broadcastTitle: broadcast.title.orEmpty,
      broadcastCreatorName: broadcast.hostName.orEmpty,
      actionButtonLabel: snapshot.canEndBroadcast ? 'End' : 'Leave',
      action: () => _handleAction(
        context,
        broadcastId: broadcast.id,
        exitsByEnding: snapshot.canEndBroadcast,
      ),
      onTap: snapshot.isInRoom
          ? () async => await context.push<void>(MRoutes.liveBroadcast)
          : null,
    );
  }

  Future<void> _handleAction(
    BuildContext context, {
    required Id broadcastId,
    required bool exitsByEnding,
  }) async {
    final confirmed = await BroadcastExitAlertDialog.show(
      context,
      exitsByEnding,
    );
    if (!(confirmed ?? false)) return;

    // A switch during the dialog takes the manager away with the account, and
    // a newer session in another broadcast is not the one the dialog asked
    // about.
    if (!di.isRegistered<ActiveBroadcastManager>()) return;
    final manager = di<ActiveBroadcastManager>();
    final snapshot = manager.snapshot.value;
    if (snapshot.broadcast?.id != broadcastId || !snapshot.isInRoom) return;

    await manager.leaveOrEnd.runAsync();
  }

  static String _badgeLabel(LivePhase phase) => switch (phase) {
    LivePhase.connecting => 'Connecting',
    LivePhase.live => 'Live',
    LivePhase.reconnecting => 'Reconnecting',
    LivePhase.ending => 'Ending',
    LivePhase.failed => 'Off air',
    LivePhase.idle || LivePhase.ended => '',
  };
}

class _Banner extends StatelessWidget {
  const new({
    required this.badgeTitle,
    required this.broadcastTitle,
    required this.broadcastCreatorName,
    this.actionButtonLabel,
    this.onTap,
    this.action,
    super.key,
  });

  final String badgeTitle;
  final String broadcastTitle;
  final String broadcastCreatorName;
  final VoidCallback? onTap;
  final String? actionButtonLabel;
  final VoidCallback? action;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return InkWell(
      onTap: onTap,
      borderRadius: .circular(Insets.lg),
      child: Card(
        margin: const .symmetric(horizontal: Insets.lg),
        shape: RoundedSuperellipseBorder(borderRadius: .circular(Insets.lg)),
        child: Padding(
          padding: const .all(14),
          child: Row(
            mainAxisAlignment: .spaceBetween,
            children: [
              Expanded(
                child: Column(
                  mainAxisAlignment: .spaceBetween,
                  crossAxisAlignment: .start,
                  children: [
                    Row(
                      children: [
                        CircleAvatar(
                          radius: 5,
                          backgroundColor: colors.secondaryContainer,
                          child: CircleAvatar(
                            radius: 3,
                            backgroundColor: colors.secondary,
                          ),
                        ),
                        Spaces.horizontalMicro,
                        MText(
                          badgeTitle,
                          style: textTheme.microMedium,
                          color: colors.error,
                        ),
                      ],
                    ),
                    const SizedBox(height: 2),
                    Container(
                      height: 24,
                      alignment: .centerLeft,
                      child: MText(
                        broadcastTitle,
                        style: MTextTheme.of(context).captionMedium,
                        maxLines: 1,
                        overflow: .ellipsis,
                      ),
                    ),
                    MText(
                      broadcastCreatorName,
                      color: colors.onBackgroundVariant,
                      style: textTheme.captionRegular,
                    ),
                  ],
                ),
              ),
              Spaces.horizontalMedium,
              LimitedBox(
                maxHeight: 32,
                maxWidth: 79,
                child: MDangerButton(
                  label: actionButtonLabel ?? '',
                  onPressed: action,
                  style: FilledButton.styleFrom(
                    shape: const RoundedRectangleBorder(
                      borderRadius: Corners.sm,
                    ),
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
