import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';
import 'package:meno_mobile/features/broadcast/services/_services.dart';

/// The co-host handshake, from the invitee's side.
///
/// An invitation reaches this manager two ways: the `cohostInvitation` frame,
/// and [loadPending], which reads the API's list of the account's pending
/// invitations. The read is what makes an invitation delivered while the app
/// was closed recoverable — the frame was never heard, but the row is still
/// there. Both feed [pending], and an invitation is forgotten as soon as the
/// API says the handshake moved on.
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
      errorFilterFn: menoExceptionFilter,
      debugName: 'cohost.invite',
    );

    loadPending = .createAsyncNoParamNoResult(
      _loadPending,
      errorFilterFn: menoExceptionFilter,
      debugName: 'cohost.loadPending',
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

  /// Reads the API's pending list and makes it the truth for this account.
  late final Command<void, void> loadPending;

  /// Says yes to an invitation, answering with the session it produces.
  late final Command<CohostInvitation, CohostSession?> accept;

  /// Says no to an invitation.
  late final Command<CohostInvitation, void> decline;

  StreamSubscription<CohostEvent>? _subscription;

  /// Invitations this session has already answered or seen resolved.
  ///
  /// A list read that started before an answer can finish after it, and
  /// reinstating an invitation the account just declined is worse than a stale
  /// card. The ids are kept here so the response cannot bring one back.
  final Set<Id> _resolved = {};

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
    // A re-invite reuses the row id, so a new frame makes the id answerable
    // again even if this session saw its previous round resolved.
    _resolved.remove(invitation.id);
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
    _resolved.add(invitationId);
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
    for (final item in pending.value) {
      if (item.broadcastId == broadcastId) _resolved.add(item.id);
    }
    pending.value = next;
  }

  /// Replaces [pending] with the API's list, keeping frames that arrived while
  /// the request was in flight.
  ///
  /// The server list is authoritative for what is still pending, but it is a
  /// snapshot: a `cohostInvitation` that arrived after the request started is
  /// newer and must survive the replace. If the response omits a name the
  /// frame carried, the merge keeps the frame's name.
  Future<void> _loadPending() async {
    final known = {for (final item in pending.value) item.id};
    final current = {for (final item in pending.value) item.id: item};
    final fetched = await _http.pendingCohostInvitations();
    final next = <Id, CohostInvitation>{};

    for (final invitation in fetched) {
      if (_resolved.contains(invitation.id)) continue;
      final existing = current[invitation.id];
      next[invitation.id] = existing?.merge(invitation) ?? invitation;
    }

    for (final item in pending.value) {
      if (!known.contains(item.id)) next.putIfAbsent(item.id, () => item);
    }

    pending.value = next.values.toList()..sort(_newestFirst);
  }

  /// Newest invitation first, with an undated one last.
  static int _newestFirst(CohostInvitation a, CohostInvitation b) {
    final left = a.createdAt;
    final right = b.createdAt;
    if (left == null && right == null) return 0;
    if (left == null) return 1;
    if (right == null) return -1;
    return right.compareTo(left);
  }

  @override
  Future<void> onDispose() async {
    await _subscription?.cancel();
    _subscription = null;
    pending.dispose();
    invite.dispose();
    loadPending.dispose();
    accept.dispose();
    decline.dispose();
  }
}
