import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_ui/meno_ui.dart';

/// The transition into a live room.
///
/// A live session's services are registered in its own scope, and signing into
/// a room is asynchronous — the scope exists before it is ready. This page is
/// what the router shows in between: it waits for that scope (and everything
/// else registered with `init()`) and then replaces itself with the room, or
/// offers the two ways out when the wait fails.
///
/// It is a page rather than a dialog because it is a *destination*: a deep link
/// to a broadcast lands here, and a failure has to leave the user somewhere
/// with a way forward.
class LiveSessionInitPage extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final futureNotifier = createOnce(
      () => ValueNotifier<Future<void>>(
        di.allReady(timeout: const Duration(seconds: 30)),
      ),
    );

    final snapshot = watchFuture<ValueNotifier<Future<void>>, void>(
      (notifier) => notifier.value,
      target: futureNotifier,
      initialValue: null,
      allowFutureChange: true,
    );

    if (snapshot.hasError) {
      return Scaffold(
        body: Center(
          child: Column(
            crossAxisAlignment: .stretch,
            mainAxisAlignment: .center,
            children: [
              MErrorState(
                message: errorMessage(snapshot.error),
                onRetry: () {
                  futureNotifier.value = di.allReady(
                    timeout: const Duration(seconds: 30),
                  );
                },
              ),
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

    if (snapshot.connectionState == .waiting) return const _LoadingView();

    // Ready: hand the client to the room it was heading for. The scope knows
    // which broadcast that is; a page opened without one goes home rather than
    // guessing an id.
    final path = di.isRegistered<LiveSession>()
        ? '${MRoutes.broadcasts}/${di<LiveSession>().broadcastId}/live'
        : MRoutes.home;
        
    callOnceAfterThisBuild((_) => context.replace(path));

    return const _LoadingView();
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
