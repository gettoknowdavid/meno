import 'package:flutter_it/flutter_it.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:responsive_framework/responsive_framework.dart';

/// How long the app waits for its services before surfacing the error state.
const Duration _kInitTimeout = Duration(seconds: 30);

/// The Meno app.
///
/// The router is held back until get_it reports every registration ready, so no
/// route renders against half-initialised services. While the container starts
/// the design system's [MLoadingPage] is shown; if a service fails to start, an
/// [MErrorState] offers a retry that re-runs the wait.
///
/// The gate uses `watchFuture` rather than watch_it's `allReady()` helper
/// because only the former hands the widget the error itself (to render) and
/// lets a retry swap in a fresh future. Replacing the notifier's value,
/// together with `allowFutureChange: true`, is what makes `watchFuture`
/// re-subscribe instead of caching the first future.
class MenoApp extends WatchingWidget {
  /// Creates the app.
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final readiness = createOnce(
      () => ValueNotifier<Future<void>>(di.allReady(timeout: _kInitTimeout)),
    );

    final snapshot = watchFuture<ValueNotifier<Future<void>>, void>(
      (notifier) => notifier.value,
      target: readiness,
      initialValue: null,
      allowFutureChange: true,
    );

    if (snapshot.hasError) {
      return _AppFrame(
        child: MScaffold(
          body: Center(
            child: MErrorState(
              message:
                  "We couldn't start the app. Please check your "
                  'connection, then try again.',
              onRetry: () => _retry(readiness),
            ),
          ),
        ),
      );
    }

    if (snapshot.connectionState != .done) {
      return const _AppFrame(child: MLoadingPage());
    }

    return MaterialApp.router(
      debugShowCheckedModeBanner: false,
      theme: MTheme.light,
      darkTheme: MTheme.dark,
      routerConfig: di<GoRouter>(),
      builder: (context, child) => ResponsiveBreakpoints.builder(
        breakpoints: menoBreakpoints,
        child: child ?? const SizedBox.shrink(),
      ),
    );
  }

  /// Re-runs the readiness wait by swapping in a fresh future.
  void _retry(ValueNotifier<Future<void>> readiness) {
    readiness.value = di.allReady(timeout: _kInitTimeout);
  }
}

/// A themed frame for the states shown before the router exists.
class _AppFrame extends StatelessWidget {
  const new({required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) => MaterialApp(
    debugShowCheckedModeBanner: false,
    theme: MTheme.light,
    darkTheme: MTheme.dark,
    home: child,
  );
}
