import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

/// Answers `path_provider`'s platform channel with a throwaway directory.
///
/// The app opens its ObjectBox store in the documents directory, and a test has
/// no plugin registrant to answer that call. Without this the database
/// registration fails, the startup gate paints its error state, and a test that
/// expected a screen finds a message about the app not starting instead.
///
/// Call from `setUp`, after the binding exists (this makes sure it does). The
/// directory is left behind on purpose: the store is still open while
/// teardown callbacks run, and Windows refuses to delete a directory a handle
/// is open in.
void installFakePathProvider() {
  TestWidgetsFlutterBinding.ensureInitialized();

  final directory = Directory.systemTemp.createTempSync('meno_test_');

  TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
      .setMockMethodCallHandler(
        const MethodChannel('plugins.flutter.io/path_provider'),
        (call) async => directory.path,
      );
}
