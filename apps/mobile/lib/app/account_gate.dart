import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/app/account_transition_owner.dart';
import 'package:meno_mobile/features/auth/model/_model.dart';
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
      .ready => KeyedSubtree(
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
