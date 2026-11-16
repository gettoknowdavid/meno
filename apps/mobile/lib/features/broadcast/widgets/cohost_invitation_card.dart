import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_ui/meno_ui.dart';

/// A co-host invitation, with the two answers the invitee owes it.
///
/// The card is the same wherever an invitation can arrive — the inbox and the
/// room — because the handshake has one shape: the accept answers with a
/// session, and the caller decides what to do with it. Accepting enters with
/// the token the answer carries: a live broadcast opens its room, and a draft
/// opens its details until the host goes live.
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
                  onPressed: isBusy ? null : () => _accept(context, manager),
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

  /// Answers the invitation, then enters with the session it produced.
  ///
  /// The live endpoint has already joined the invitee's socket and inserted
  /// their participant row, so a second `/join` would answer `ALREADY_JOINED`.
  /// The returned token is therefore the entry credential, used directly by
  /// the live manager (M2). A draft has no room and no token; it opens the
  /// broadcast instead, where the account can wait for go-live.
  Future<void> _accept(
    BuildContext context,
    CohostInvitationsManager manager,
  ) async {
    final accepted = await manager.accept.runAsync(invitation);
    if (accepted == null || !context.mounted) return;

    final token = accepted.livekitToken;
    if (token == null || token.isEmpty) {
      context.showInfoToast('You can co-host once the broadcast goes live.');
      await context.push(MRoutes.broadcastRoute(invitation.broadcastId));
      return;
    }

    try {
      final live = di<ActiveBroadcastManager>();
      final current = live.session;

      // Accepting from the in-room card upgrades the room already open; it
      // must not be closed and replaced by a second one.
      if (current != null && current.broadcast.id == invitation.broadcastId) {
        await current.promoteToCohost(token);
        return;
      }

      final broadcast = await di<BroadcastHttpService>().getBroadcast(
        invitation.broadcastId,
      );
      if (!context.mounted) return;

      final result = await live.enter(
        LiveEnterRequest(
          broadcastId: invitation.broadcastId,
          mode: LiveEnterMode.listen,
          session: accepted.toBroadcastSession(broadcast),
        ),
      );
      if (!context.mounted || result.outcome == EnterOutcome.rejected) return;

      // A failed entry lands on the transition page's failure view, where the
      // attached session can be retried without another `/join`.
      await context.push(MRoutes.liveSessionInit);
    } on Object catch (error) {
      if (context.mounted) context.showErrorToast(errorMessage(error));
    }
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

    // The account seeds this list at start; opening the inbox is the second
    // chance for a read that failed then, without a socket frame to rely on.
    callOnce((_) => di<CohostInvitationsManager>().loadPending.run());

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
