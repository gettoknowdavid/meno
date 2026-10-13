import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/auth/manager/auth_manager.dart';
import 'package:meno_mobile/features/settings/settings.dart';
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
///
/// The app is also where failures with no local handler become visible: it
/// listens to `Command.globalErrors` and reports them.
class MenoApp extends WatchingWidget {
  /// Creates the app.
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    // The types are spelled out because watch_it cannot infer the element type
    // of a stream it is handed, and a snapshot of `Object` would hide the
    // failure's type from the reporter.
    registerStreamHandler<Stream<CommandError<dynamic>>, CommandError<dynamic>>(
      target: Command.globalErrors,
      handler: (context, snapshot, cancel) {
        final error = snapshot.data?.error;
        if (error != null) _report(error);
      },
    );

    // Registered eagerly, so it is watchable on every build — including the
    // ones that render before the container is ready.
    final themeMode = watchValue<ThemeManager, ThemeMode>(
      (manager) => manager.themeMode,
    );

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
        themeMode: themeMode,
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
      return _AppFrame(themeMode: themeMode, child: const MLoadingPage());
    }

    return MaterialApp.router(
      debugShowCheckedModeBanner: false,
      theme: MTheme.light,
      darkTheme: MTheme.dark,
      themeMode: themeMode,
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

  /// Tells the user a command failed, and ends a session that has expired.
  ///
  /// Everything that reaches here has already been logged by the global
  /// exception handler; this is only the part a person can see. The one failure
  /// the app acts on itself is a rejected session, which is unrecoverable from
  /// inside the screen that hit it.
  void _report(Object error) {
    final overlay = rootNavigatorKey.currentState?.overlay;
    // Before the router is up there is nothing to attach a toast to, and the
    // failure is on its way to the log either way.
    if (overlay == null) return;

    if (error is UnauthenticatedException &&
        error.code != MenoErrorCode.invalidCredentials) {
      unawaited(di<AuthManager>().signOut());
    }

    MToastController.of(overlay).show(
      MToastRequest(
        builder: (_, dismiss) => MToast(
          errorMessage(error),
          variant: MToastVariant.error,
          onClose: dismiss,
        ),
      ),
    );
  }
}

/// A themed frame for the states shown before the router exists.
class _AppFrame extends StatelessWidget {
  const new({required this.themeMode, required this.child});

  /// The theme the account's settings ask for.
  final ThemeMode themeMode;

  final Widget child;

  @override
  Widget build(BuildContext context) => MaterialApp(
    debugShowCheckedModeBanner: false,
    theme: MTheme.light,
    darkTheme: MTheme.dark,
    themeMode: themeMode,
    home: child,
  );
}
