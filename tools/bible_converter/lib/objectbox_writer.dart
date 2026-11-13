import 'dart:io';

import 'package:bible_converter/model.dart';
import 'package:bible_converter/objectbox.g.dart';
// ObjectBox exposes no public API to force/validate the native library load.
// ignore: implementation_imports
import 'package:objectbox/src/native/bindings/bindings.dart' as obx_bindings;

/// Directory (relative to the repo root) that holds `objectbox.dll`.
const _nativeLibDirName = 'native_libs';
const _objectBoxLibFileName = 'objectbox.dll';

/// Opens a throw-away ObjectBox store in a temp directory.
///
/// The store's data file (`data.mdb`) lives inside [Store.directoryPath].
Store openTemporaryStore() {
  ensureObjectBoxLibraryLoaded();

  final tempDir = Directory.systemTemp.createTempSync('bible_');
  return openStore(directory: tempDir.path);
}

/// Returns the path to the compiled `data.mdb` inside [storePath].
String dataFileIn(String storePath) => '$storePath/data.mdb';

/// ObjectBox 6.x loads its native library on its own: it looks for
/// `<cwd>/lib/objectbox.dll` and then falls back to a plain `objectbox.dll`
/// lookup. There is no API to pass an explicit path, so we point the lookup at
/// `native_libs/` (the single shared copy in this repo) for the duration of the
/// load, then restore the working directory so relative CLI paths keep working.
///
/// The load happens lazily on first use and is cached by the package, so this
/// only does real work once per process.
void ensureObjectBoxLibraryLoaded() {
  final libPath = _resolveObjectBoxLibPath();

  if (!File(libPath).existsSync()) {
    throw StateError(
      'ObjectBox native library not found at: $libPath\n'
      'Expected location: $_nativeLibDirName/$_objectBoxLibFileName '
      '(searching upward from the current directory).\n'
      'Copy objectbox.dll there before running the converter.',
    );
  }

  final originalDirectory = Directory.current;
  try {
    // ObjectBox searches for objectbox.dll in the current working directory,
    // so load it while standing inside native_libs/.
    Directory.current = File(libPath).parent.path;
    obx_bindings.C.version_string();
  } finally {
    Directory.current = originalDirectory;
  }
}

String _resolveObjectBoxLibPath() {
  const relativePath = '$_nativeLibDirName/$_objectBoxLibFileName';

  final fromRepoRoot = '${_findRepoRoot().path}/$relativePath';
  if (File(fromRepoRoot).existsSync()) return fromRepoRoot;

  final fromCwd = '${Directory.current.path}/$relativePath';
  if (File(fromCwd).existsSync()) return fromCwd;

  return fromRepoRoot;
}

/// Walks up from the current directory looking for `native_libs/objectbox.dll`,
/// so the converter works when run from the repo root or from its own folder.
Directory _findRepoRoot() {
  var current = Directory.current;

  for (var depth = 0; depth < 10; depth++) {
    if (File('${current.path}/$_nativeLibDirName/$_objectBoxLibFileName')
        .existsSync()) {
      return current;
    }

    final parent = current.parent;
    if (parent.path == current.path) break;
    current = parent;
  }

  return Directory.current;
}

void writeVersionAndVerses(
  Store store,
  BibleVersion version,
  List<Verse> verses,
) {
  store.box<BibleVersion>().put(version);
  store.box<Verse>().putMany(verses);
}
