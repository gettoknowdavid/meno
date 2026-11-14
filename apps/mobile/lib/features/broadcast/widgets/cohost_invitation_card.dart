import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_ui/meno_ui.dart';

/// A co-host invitation, with the two answers the invitee owes it.
///
/// The card is the same wherever an invitation can arrive — the inbox and the
/// room — because the handshake has one shape: the accept answers with a
/// session, and the caller decides what to do with it. Here that is the lobby:
/// accepting from the inbox is the start of joining the broadcast, not a
/// change to a list.
class CohostInvitationCard extends WatchingWidget {
  const new({required this.invitation, super.key});

  /// The invitation to answer.
  final CohostInvitation invitation;

  @override
  Widget build(BuildContext context) {
    final manager = di<CohostInvitationsManager>();
    final styles = MCardStyles.of(context);
    final textTheme = MTextTheme.of(context);

    final isAccepting = watchValue<CohostInvitationsManager, bool>(
      (manager) => manager.accept.isRunning,
    );
    final isDeclining = watchValue<CohostInvitationsManager, bool>(
      (manager) => manager.decline.isRunning,
    );
    final isBusy = isAccepting || isDeclining;

    return Container(
      padding: styles.nCardContentPadding,
      decoration: BoxDecoration(
        color: styles.backgroundColor,
        borderRadius: styles.nBorderRadius,
      ),
      child: Column(
        crossAxisAlignment: .start,
        mainAxisSize: .min,
        children: [
          MText(
            invitation.prompt,
            style: textTheme.captionMedium,
            maxLines: 3,
            overflow: .ellipsis,
          ),
          Spaces.verticalMedium,
          Row(
            children: [
              Expanded(
                child: MPrimaryButton(
                  label: 'Accept',
                  loading: isAccepting,
                  onPressed: isBusy
                      ? null
                      : () async {
                          final session = await manager.accept.runAsync(
                            invitation,
                          );
                          if (session == null || !context.mounted) return;
                          await PreStreamModal.show(
                            context,
                            invitation.broadcastId,
                          );
                        },
                ),
              ),
              Spaces.horizontalSmall,
              Expanded(
                child: MSecondaryButton(
                  label: 'Decline',
                  loading: isDeclining,
                  onPressed: isBusy
                      ? null
                      : () => manager.decline.run(invitation),
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

/// Every invitation this account can answer, one card each.
///
/// It draws nothing at all when there is none: the inbox is a list of
/// notifications first, and a section header over an empty list would be noise.
class CohostInvitationList extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    if (!di.isRegistered<CohostInvitationsManager>()) {
      return const SizedBox.shrink();
    }

    final invitations =
        watchValue<CohostInvitationsManager, List<CohostInvitation>>(
          (manager) => manager.pending,
        );

    if (invitations.isEmpty) return const SizedBox.shrink();

    return Column(
      mainAxisSize: .min,
      children: [
        for (final invitation in invitations) ...[
          CohostInvitationCard(
            key: ValueKey(invitation.id),
            invitation: invitation,
          ),
          Spaces.verticalSmall,
        ],
      ],
    );
  }
}
