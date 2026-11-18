import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:dio/dio.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/database.dart';
import 'package:meno_mobile/features/bible/bible.dart';

import '../../_support/stub_api.dart';
import '../../_support/test_database.dart';

/// A gzipped seed payload in the format the converter writes.
Uint8List _payloadBytes(String code) {
  final payload = <String, dynamic>{
    'format': 2,
    'code': code,
    'name': '${code.toUpperCase()} Version',
    'copyrightStatement': 'Public domain',
    'copyright': 0,
    'books': [
      [1, 'Genesis'],
    ],
    'verses': [
      [1, 1, 1, 'In the beginning'],
      [1, 1, 2, 'second'],
    ],
  };
  return Uint8List.fromList(gzip.encode(utf8.encode(jsonEncode(payload))));
}

/// An empty catalogue, so a test never depends on the file server.
ResponseBody _emptyCatalogue() => ResponseBody.fromString(
  jsonEncode({'code': 'OK', 'message': '', 'status': true, 'data': <Object>[]}),
  200,
  headers: {
    Headers.contentTypeHeader: [Headers.jsonContentType],
  },
);

void main() {
  late Directory cache;
  late TranslationsManager manager;
  late BibleLocalService local;
  late BibleDownloadService downloads;

  setUp(() async {
    // The removal command reports through the app's global handler, which no
    // test installs; without this a refusal asserts instead of surfacing.
    Command.globalExceptionHandler = (_, _) {};
    cache = await Directory.systemTemp.createTemp('meno-bible-remove-');
    await installTestDatabase();

    final logger = MockMenoLogger();
    di.registerSingleton<MenoLogger>(logger);

    local = BibleLocalService(di<Database>());
    final client = stubClient(StubApi([() async => _emptyCatalogue()]));

    downloads = BibleDownloadService(
      http: client,
      logger: logger,
      supportDirectory: () async => cache,
    );
    manager = TranslationsManager(
      local: local,
      seed: BibleSeedService(translations: const []),
      catalogue: BibleCatalogueService(client),
      downloads: downloads,
      logger: logger,
    );

    // The removal command asks the reader to move off a translation that is
    // gone, so the manager it talks to has to exist.
    di.registerSingleton<BibleManager>(
      BibleManager(local: local, logger: logger),
    );
  });

  tearDown(() async {
    Command.globalExceptionHandler = null;
    if (cache.existsSync()) await cache.delete(recursive: true);
    await resetTestLocator();
  });

  /// Writes a cached payload and seeds it, the way a download would.
  Future<void> seed(String code) async {
    // The downloader's own cache folder, so `cachedPayloads` sees it and a
    // removal has something real to delete.
    final directory = await downloads.cacheDirectory(code);
    await directory.create(recursive: true);
    final file = File(
      '${directory.path}${Platform.pathSeparator}1.json.gz',
    );
    await file.writeAsBytes(_payloadBytes(code));

    final seeded = await BibleSeedService(translations: const [])
        .seedFromCache(code: code, version: 1, file: file);
    expect(seeded, isTrue);
  }

  test(
    'removing a downloaded translation deletes its rows, ledger and payload',
    () async {
      await seed('tst');
      await seed('tst2');
      await manager.initialize.runAsync();

      expect(manager.downloadedTranslations.value, hasLength(2));
      expect(manager.isBundled('tst'), isFalse);
      expect(manager.canRemove('tst'), isTrue);
      expect(manager.verseTotal('tst'), 2);

      di<BibleManager>().translation.value = 'tst';
      await manager.removeTranslation.runAsync('tst');

      expect(local.version('tst'), isNull);
      expect(local.seedVersion('tst'), 0);
      expect(local.verseTotal('tst'), 0);
      expect(
        manager.downloadedTranslations.value.map((row) => row.code).toList(),
        ['tst2'],
      );

      // The cache is what re-seeds at the next launch, so it goes too.
      final cached = await downloads.cachedPayloads();
      expect(cached.map((payload) => payload.code).toList(), ['tst2']);

      // The reader does not stay on scripture the device no longer holds.
      expect(di<BibleManager>().translation.value, 'tst2');
    },
  );

  test('the last translation left cannot be removed', () async {
    await seed('tst');
    await manager.initialize.runAsync();

    expect(manager.downloadedTranslations.value, hasLength(1));
    expect(manager.canRemove('tst'), isFalse);

    await expectLater(
      manager.removeTranslation.runAsync('tst'),
      throwsA(isA<ConflictException>()),
    );

    expect(local.verseTotal('tst'), 2);
    expect(manager.downloadedTranslations.value, hasLength(1));
  });

  test('a bundled translation cannot be removed', () async {
    await seed('kjv');
    await seed('tst');
    await manager.initialize.runAsync();

    expect(manager.isBundled('kjv'), isTrue);
    expect(manager.canRemove('kjv'), isFalse);

    await expectLater(
      manager.removeTranslation.runAsync('kjv'),
      throwsA(isA<ForbiddenException>()),
    );

    expect(local.verseTotal('kjv'), 2);
    expect(local.seedVersion('kjv'), 1);
  });
}
