import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_shared/services/database.dart';
import 'package:meno_mobile/objectbox.g.dart';

var _storeId = 0;

/// Registers an in-memory ObjectBox store as the app's [Database].
///
/// App-boot tests compose the real locator, whose `Database.create` opens a
/// store in the documents directory and awaits work that the widget test's
/// fake-async zone never completes. Registering this first makes
/// `configureDependencies` reuse it instead, so each test gets an empty store
/// that `di.reset()` closes.
Future<void> installTestDatabase() async {
  // The synchronous constructor is deliberate: `openStore` awaits work that
  // does not progress inside the widget-test fake-async zone, so a `setUp`
  // that awaited it would hang before the test body ran.
  final store = Store(
    getObjectBoxModel(),
    directory: 'memory:meno-test-${_storeId++}',
  );
  di.registerSingleton<Database>(Database.fromStore(store));
}

/// Clears the locator between boot tests without running every `onDispose`.
///
/// The app registers long-lived singletons whose disposal awaits work that
/// the widget-test zone never completes, so a full `di.reset()` hangs in
/// `tearDown`. The store is the one resource that must be closed; the rest are
/// dropped with the test process.
Future<void> resetTestLocator() async {
  if (di.isRegistered<Database>()) {
    await di<Database>().onDispose();
  }
  await di.reset(dispose: false);
}
