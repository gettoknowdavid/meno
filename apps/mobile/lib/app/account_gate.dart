import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/app/account_transition_owner.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_mobile/features/profile/manager/my_profile_manager.dart';
import 'package:meno_ui/meno_ui.dart';

/// Shows the app only while the account lifecycle says it is usable.
///
/// The authenticated subtree is keyed by account id and generation, so every
/// activation tears down the previous subtree's cached watches and feed
/// sources instead of letting them outlive their account. While the owner is
/// preparing, quiescing or blocked, a neutral page replaces the subtree — the
/// old account is gone before its scope is popped.
class AccountGate extends WatchingWidget {
  const new({required this.child, super.key});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    // One watch, first on every build: an unready branch must not resolve any
    // account-typed object.
    final snapshot = watchValue<AccountTransitionOwner, AccountSnapshot>(
      (owner) => owner.snapshot,
    );

    return switch (snapshot.phase) {
      .signedOut => child,
      .preparing || .quiescing => const NeutralTransitionPage(),
      .ready => _AccountScopeHost(
        key: ValueKey(
          'account-${snapshot.context?.accountId.getOrCrash}-'
          '${snapshot.context?.generation}',
        ),
        child: child,
      ),
      .blocked => BlockedAccountPage(
        failure: snapshot.failure,
        onRetry: di<AccountTransitionOwner>().restore,
      ),
    };
  }
}

/// The ready account's subtree, and the one place its background reads start.
///
/// Mounted only by the ready branch and keyed by account and generation, so
/// its work runs once per activation — after `ready`, which is what lets the
/// request carry the credential the owner stored just before publishing it
/// (Step 26). A read that fails here is logged by the manager: nobody asked
/// for it, so it raises nothing.
///
/// It is also where a recovered room is handed back. `ActiveBroadcastManager`
/// re-enters the broadcast the last process was in — as host or as listener —
/// but re-entering is not the same as being there: without a destination the
/// app would come back from a crash to Home, with a live session nobody can
/// see. So the one signal that recovery published becomes one navigation, and
/// the transition page decides what that phase calls for.
class _AccountScopeHost extends StatefulWidget {
  const new({required this.child, super.key});

  final Widget child;

  @override
  State<_AccountScopeHost> createState() => _AccountScopeHostState();
}

class _AccountScopeHostState extends State<_AccountScopeHost> {
  /// Whether the recovered room has already been opened.
  ///
  /// One activation, one hand-off: a rebuild must not push the room again, and
  /// a person who left it is not pulled back into it.
  bool _openedRecoveredRoom = false;

  /// The live owner, when this build has one.
  ///
  /// Null in a widget test that mounts the gate with only the registration it
  /// is about: the refresh and the recovery hand-off are best-effort side
  /// effects, and neither should force every gate test to compose the whole
  /// account scope.
  ActiveBroadcastManager? _live;

  @override
  void initState() {
    super.initState();
    if (di.isRegistered<MyProfileManager>()) {
      unawaited(di<MyProfileManager>().refresh());
    }

    _live = di.isRegistered<ActiveBroadcastManager>()
        ? di<ActiveBroadcastManager>()
        : null;
    _live?.recovery.addListener(_onRecovered);

    // Recovery may have finished before this widget was built — the account
    // becomes ready first — so the signal is read once here as well.
    _onRecovered();
  }

  @override
  Widget build(BuildContext context) => widget.child;

  /// Takes the account back into the room a restart left it in.
  void _onRecovered() {
    final live = _live;
    if (live == null || _openedRecoveredRoom || live.recovery.value == null) {
      return;
    }
    _openedRecoveredRoom = true;

    // After the frame: recovery can complete while the navigator is still being
    // built, and a push before then has nothing to push onto.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || !di.isRegistered<GoRouter>()) return;
      // The transition page is the destination: going there again would stack a
      // second copy of the room's entry over the one being shown.
      if (di<GoRouter>().state.uri.path == MRoutes.liveSessionInit) return;
      unawaited(di<GoRouter>().push<void>(MRoutes.liveSessionInit));
    });
  }

  @override
  void dispose() {
    _live?.recovery.removeListener(_onRecovered);
    super.dispose();
  }
}

/// The page shown while an account is being prepared or taken down.
class NeutralTransitionPage extends StatelessWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) => const MScaffold(body: MLoadingPage());
}

/// The page shown when an account transition cannot continue.
class BlockedAccountPage extends StatelessWidget {
  const new({required this.failure, this.onRetry, super.key});

  final BlockedFailure? failure;
  final Future<void> Function()? onRetry;

  @override
  Widget build(BuildContext context) {
    return MScaffold(
      body: Center(
        child: MErrorState(
          message:
              failure?.message ??
              'This account could not be opened. Please sign in again.',
          onRetry: onRetry == null ? null : () => onRetry!(),
        ),
      ),
    );
  }
}
