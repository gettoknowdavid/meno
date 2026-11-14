import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';
import 'package:meno_mobile/features/broadcast/services/_services.dart';

/// The co-host handshake, from the invitee's side.
///
/// The API has no endpoint that lists an account's invitations: the invitee
/// learns about one from the `cohostInvitation` frame, and answers it with the
/// invitation id the frame carried. That makes this manager the client's only
/// record of them — it keeps the frames it has seen in [pending] so the inbox
/// and the room can both draw the same list, and forgets an invitation as soon
/// as the API says the handshake moved on.
///
/// The frames also travel the other way: a host is told when an invitation is
/// accepted, and an invitee's own acceptance answers with a `newCohost` frame.
/// Every one of those means the invitation this side asked about is gone, so
/// they all drop it.
final class CohostInvitationsManager implements Disposable {
  /// Creates the manager over the HTTP service and the account's frames.
  new({
    required this._http,
    required Stream<CohostEvent> events,
    required this._logger,
  }) {
    _events = events;
    invite = .createAsync<({Id broadcastId, Id userId}), CohostInvitation?>(
      (request) async {
        final invitation = await _http.inviteCohost(
          request.broadcastId,
          request.userId,
        );
        _logger.i(
          'Invited ${request.userId} to co-host '
          '${request.broadcastId} as ${invitation.id}',
        );
        return invitation;
      },
      initialValue: null,
      errorFilter: null,
      errorFilterFn: menoExceptionFilter,
      debugName: 'cohost.invite',
    );

    accept = .createAsync<CohostInvitation, CohostSession?>(
      (invitation) async {
        final session = await _http.acceptCohostInvitation(
          invitation.broadcastId,
          invitation.id,
        );
        _remove(invitation.id);
        return session;
      },
      initialValue: null,
      errorFilter: null,
      errorFilterFn: menoExceptionFilter,
      debugName: 'cohost.accept',
    );

    decline = .createAsyncNoResult<CohostInvitation>(
      (invitation) async {
        await _http.declineCohostInvitation(
          invitation.broadcastId,
          invitation.id,
        );
        _remove(invitation.id);
      },
      errorFilter: null,
      errorFilterFn: menoExceptionFilter,
      debugName: 'cohost.decline',
    );
  }

  final BroadcastHttpService _http;

  /// The account's co-host frames, kept for the one subscription [init] makes.
  late final Stream<CohostEvent> _events;

  final MenoLogger _logger;

  /// The invitations this account can answer, newest first.
  final pending = ValueNotifier<List<CohostInvitation>>([]);

  /// Asks the account and the broadcast the request names to meet.
  late final Command<({Id broadcastId, Id userId}), CohostInvitation?> invite;

  /// Says yes to an invitation, answering with the session it produces.
  late final Command<CohostInvitation, CohostSession?> accept;

  /// Says no to an invitation.
  late final Command<CohostInvitation, void> decline;

  StreamSubscription<CohostEvent>? _subscription;

  /// Starts listening to the account's socket frames.
  ///
  /// Idempotent: the account scope calls it once as the manager is built, and
  /// a second call would be two prompts for one invitation.
  void init() {
    _subscription ??= _events.listen(_onEvent);
  }

  void _onEvent(CohostEvent event) {
    switch (event) {
      case CohostInvitedEvent(
        :final invitationId,
        :final broadcastId,
        :final inviterName,
        :final broadcastTitle,
      ):
        _upsert(
          CohostInvitation(
            id: invitationId,
            broadcastId: broadcastId,
            inviterName: inviterName,
            broadcastTitle: broadcastTitle,
            createdAt: DateTime.now(),
          ),
        );
      case CohostAcceptedEvent() ||
          CohostDeclinedEvent() ||
          NewCohostEvent() ||
          RemovedCohostEvent():
        _removeBroadcast(event.broadcastId);
      case CohostDemotionEvent():
        // The invitation was answered long ago; being demoted changes what the
        // account may publish, not what it has to decide.
        break;
    }
  }

  void _upsert(CohostInvitation invitation) {
    if (!invitation.isPending) return;
    final existing = {
      for (final item in pending.value) item.id: item,
    }[invitation.id];
    final next = [
      if (existing != null) existing.merge(invitation) else invitation,
      for (final item in pending.value)
        if (item.id != invitation.id) item,
    ];
    pending.value = next;
  }

  void _remove(Id invitationId) {
    pending.value = [
      for (final item in pending.value)
        if (item.id != invitationId) item,
    ];
  }

  void _removeBroadcast(Id broadcastId) {
    final next = [
      for (final item in pending.value)
        if (item.broadcastId != broadcastId) item,
    ];
    if (next.length == pending.value.length) return;
    pending.value = next;
  }

  @override
  Future<void> onDispose() async {
    await _subscription?.cancel();
    _subscription = null;
    pending.dispose();
    invite.dispose();
    accept.dispose();
    decline.dispose();
  }
}
