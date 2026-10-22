import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_ui/meno_ui.dart';

/// The transition into a live room.
///
/// A live session's services live in a get_it scope of their own, pushed when
/// the API mints the session that enters the room, and that scope is *not* the
/// room: the LiveKit client, the roster and the on-air clock only exist once
/// the session's own chain has run — connect, announce, settle. This page is
/// what the router shows in between. It
///
/// 1. waits for [broadcastId]'s scope to be open (the create, join or recovery
///    that had the session minted is what pushes it),
/// 2. starts that room's session — nothing else does, and the room's screens
///    read [LiveSessionManager.timer], which only exists afterwards,
/// 3. waits for the room to reach [LiveStatus.live], failing fast when the
///    session reports an error, and
/// 4. replaces itself with the room.
///
/// It is a page rather than a dialog because it is a *destination*: a deep link
/// to a broadcast lands here, and a failure has to leave the user somewhere
/// with a way forward.
///
/// # A room nobody joined
///
/// A scope can exist without a session — a deep link opens one so the
/// broadcast can be *known* — and that room has no token, so this page cannot
/// enter it. That is reported rather than worked around: joining belongs to the
/// pre-stream lobby, and a transition page that joined on its own would be
/// doing it in the wrong place, with nobody's consent.
class LiveSessionInitPage extends WatchingWidget {
  const new({this.broadcastId, super.key});

  /// The room this transition is for.
  ///
  /// The route passes the id it parsed. When it is omitted the open session
  /// says which room that is, which is what the shell's own hand-off needs.
  final Id? broadcastId;

  /// How long to wait for the scope, and for the room, before giving up.
  ///
  /// The first wait is short: by the time this page is shown the caller has
  /// already awaited the scope being pushed. The second is the room's own work
  /// — a connection, not a container — so it gets room to breathe.
  static const Duration _scopeTimeout = Duration(seconds: 10);
  static const Duration _roomTimeout = Duration(seconds: 30);

  @override
  Widget build(BuildContext context) {
    final target = _target();

    final entry = createOnce(
      () => ValueNotifier<Future<void>>(_enterRoom(target)),
    );

    final snapshot = watchFuture<ValueNotifier<Future<void>>, void>(
      (notifier) => notifier.value,
      target: entry,
      initialValue: null,
      allowFutureChange: true,
    );

    if (snapshot.hasError) {
      return _FailureView(
        error: snapshot.error,
        // The wait can be retried; a room whose connection failed is
        // re-entered from the broadcast, not restarted behind the user's back.
        onRetry: () => entry.value = _enterRoom(target),
      );
    }

    if (snapshot.connectionState != .done) return const _LoadingView();

    // Ready: hand the client to the room it was heading for. The room's
    // address names no broadcast; the session it opened is the room.
    callOnceAfterThisBuild(
      (_) => context.replace(MRoutes.liveBroadcast),
    );

    return const _LoadingView();
  }

  /// The room this transition is for, as far as it can be known before the
  /// session's own scope answers.
  Id? _target() {
    if (broadcastId != null && broadcastId!.isValid) return broadcastId;
    if (!di.isRegistered<LiveSession>()) return null;
    return di<LiveSession>().broadcastId;
  }

  /// Waits for the room, then for the room to be worth drawing.
  Future<void> _enterRoom(Id? target) async {
    if (target == null) throw const _RoomNotOpenException();

    final live = di<LiveScopeManager>();

    try {
      await _waitForScope(live, target);
    } on TimeoutException {
      throw const _RoomNotOpenException();
    }

    // A scope opened from a deep link has no token, so the session manager was
    // deliberately not registered in it: there is nothing here to enter.
    if (!di.isRegistered<LiveSessionManager>()) {
      throw const _RoomNotOpenException();
    }

    final session = di<LiveSessionManager>();

    // The chain that turns a session into a room: the timer, the socket
    // subscriptions, then connect → announce → settle. Its state stays
    // `initializing` until one of those settles, so a rebuild or a retry of a
    // previous attempt does not start it a second time.
    if (session.state.value is LiveSessionInitializing) {
      session.setupConfigs.run();
    }

    try {
      await _waitForLive(session);
    } on TimeoutException {
      throw const _RoomTimedOutException();
    }
  }

  /// Resolves when the scope for [broadcastId] is open and its session
  /// registered.
  Future<void> _waitForScope(LiveScopeManager live, Id broadcastId) {
    bool isOpen() => live.isInRoom(broadcastId);
    if (isOpen()) return Future<void>.value();

    final completer = Completer<void>();

    void check() {
      if (completer.isCompleted || !isOpen()) return;
      completer.complete();
    }

    live.activeBroadcastId.addListener(check);
    return completer.future
        .timeout(_scopeTimeout)
        .whenComplete(() => live.activeBroadcastId.removeListener(check));
  }

  /// Resolves when the room is live, or fails when the session reports that it
  /// could not get there.
  Future<void> _waitForLive(LiveSessionManager session) {
    final completer = Completer<void>();

    void check() {
      if (completer.isCompleted) return;
      final status = session.status.value;
      if (status == LiveStatus.live) {
        completer.complete();
      } else if (status == LiveStatus.offAir ||
          status == LiveStatus.disconnected) {
        completer.completeError(const _RoomFailedException());
      }
    }

    // The status may have moved between the caller's guard and this listener.
    check();
    if (completer.isCompleted) return completer.future;

    session.status.addListener(check);
    return completer.future
        .timeout(_roomTimeout)
        .whenComplete(() => session.status.removeListener(check));
  }
}

/// The wait, and what an un-navigated frame shows while it happens.
class _LoadingView extends StatelessWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    return const Scaffold(body: Center(child: MLoadingIndicator(100, 100)));
  }
}

/// What to do when the room cannot be entered.
class _FailureView extends StatelessWidget {
  const new({required this.error, required this.onRetry});

  final Object? error;
  final VoidCallback onRetry;

  /// What the user is told.
  ///
  /// [errorMessage] is for the API's failures and deliberately hides anything
  /// else, so this page's own — no room, no connection — speak for themselves.
  String get _message => error is MenoException
      ? errorMessage(error)
      : error?.toString() ?? 'Something went wrong. Please try again.';

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: Center(
        child: Column(
          crossAxisAlignment: .stretch,
          mainAxisAlignment: .center,
          children: [
            MErrorState(message: _message, onRetry: onRetry),
            Spaces.verticalLarge,
            Row(
              mainAxisAlignment: .center,
              spacing: 8,
              children: [
                MTextButton(
                  label: 'Go Home',
                  onPressed: () => context.go(MRoutes.home),
                ),
                MTextButton(
                  label: 'Back to Broadcast Editor',
                  onPressed: () async {
                    context.go(MRoutes.home);
                    await context.push(MRoutes.broadcastEditor);
                  },
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

/// The room this page was asked to enter is not open on this device.
final class _RoomNotOpenException implements Exception {
  const new();

  @override
  String toString() =>
      'This broadcast is not open on this device. Join it from the broadcast '
      'first.';
}

/// The room opened, but its session could not get into it.
final class _RoomFailedException implements Exception {
  const new();

  @override
  String toString() => 'The broadcast could not be entered.';
}

/// The room neither settled nor failed within the page's room timeout.
final class _RoomTimedOutException implements Exception {
  const new();

  @override
  String toString() =>
      'The broadcast took too long to connect. Please try again.';
}
