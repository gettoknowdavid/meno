import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/features/broadcast/manager/live_scope_manager.dart';
import 'package:meno_mobile/features/broadcast/manager/live_session_manager.dart';
import 'package:meno_mobile/features/broadcast/widgets/broadcast_exit_alert_dialog.dart';
import 'package:meno_ui/meno_ui.dart';

/// The way back into a broadcast this account is still in.
///
/// # The banner is drawn outside the room, so it cannot live in it
///
/// The home page is not inside the live scope, so the room's own services —
/// [LiveSessionManager], [LiveSession] — are *not registered* while this widget
/// builds. Resolving one here would be a get_it error, and there is nothing to
/// resolve in the first place: the banner has to answer "am I on air, and in
/// which broadcast" for a device that may not be in a room at all.
///
/// So the two halves are taken from where each one lives. Which broadcast is
/// open comes from [LiveScopeManager], registered for as long as the account
/// is; the broadcast itself, its status and who is hosting come from the
/// room's own [LiveSessionManager], read through `maybeGet` because its absence
/// *is* the answer to "is there a room to draw".
///
/// The rebuild that turns the one into the other is
/// [LiveScopeManager.activeBroadcastId]: it changes once the scope has been
/// pushed, which is exactly when the room's manager becomes resolvable — so a
/// room that opens while this banner is on screen paints itself, and one that
/// closes takes the card away again.
class LiveSessionBanner extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final activeId = watchValue<LiveScopeManager, Id?>(
      (manager) => manager.activeBroadcastId,
    );

    // Nothing of a room exists before the scope above was pushed, and a room a
    // deep link opened has no session manager at all: it can be listed but not
    // entered, and a card offering "End" for one would be offering an action
    // with nothing behind it.
    final manager = activeId == null ? null : di.maybeGet<LiveSessionManager>();
    if (manager == null) return const SizedBox.shrink();

    final status = watch(manager.status).value;
    final broadcast = watch(manager.broadcast).value;
    final isHost = watch(manager.isHost).value;

    if (status.isNotLive) return const SizedBox.shrink();

    return _Banner(
      key: ValueKey('broadcastBanner-${broadcast.id.orEmpty}'),
      badgeTitle: status.label,
      broadcastTitle: broadcast.title.orEmpty,
      broadcastCreatorName: broadcast.hostName.orEmpty,
      actionButtonLabel: isHost ? 'End' : 'Leave',
      action: () => _handleAction(context),
      onTap: () async {
        await context.push<void>(MRoutes.liveBroadcast);
      },
    );
  }

  Future<void> _handleAction(BuildContext context) async {
    final manager = di<LiveSessionManager>();
    final isHost = manager.isHost.value;

    final result = await BroadcastExitAlertDialog.show(context, isHost);
    if (result ?? false) manager.endSession.run();
  }
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
                      alignment: Alignment.centerLeft,
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
