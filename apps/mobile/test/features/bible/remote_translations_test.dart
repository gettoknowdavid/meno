import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';
import 'package:dio/dio.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/database.dart';
import 'package:meno_mobile/features/bible/bible.dart';

import '../../_support/stub_api.dart';
import '../../_support/test_database.dart';

/// A gzipped seed payload in the format the converter writes.
Uint8List _seedPayloadBytes() {
  final payload = <String, dynamic>{
    'format': 2,
    'code': 'tst',
    'name': 'Test Version',
    'copyrightStatement': 'free',
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

/// One catalogue row, as `GET /bible/translations` writes it.
Map<String, Object> _row({required int bytes, required String sha256}) => {
  'code': 'tst',
  'name': 'Test Version',
  'version': 1,
  'bytes': bytes,
  'sha256': sha256,
  'payloadUrl': 'https://cdn.test/bible/tst/1.json.gz',
};

/// An envelope whose `data` is a list, which [envelope] in the shared support
/// cannot express.
ResponseBody _listEnvelope(List<Map<String, Object>> rows) {
  return ResponseBody.fromString(
    jsonEncode({
      'code': 'OK',
      'message': '',
      'status': true,
      'data': rows,
    }),
    200,
    headers: {
      Headers.contentTypeHeader: [Headers.jsonContentType],
    },
  );
}

void main() {
  late Directory cache;

  setUp(() async {
    cache = await Directory.systemTemp.createTemp('meno-bible-cache-');
  });

  tearDown(() async {
    if (cache.existsSync()) await cache.delete(recursive: true);
  });

  test('a download is verified, cached, and not fetched twice', () async {
    final payload = _seedPayloadBytes();
    final api = StubApi([
      () async => _listEnvelope([
        _row(bytes: payload.length, sha256: sha256.convert(payload).toString()),
      ]),
      () async => ResponseBody.fromBytes(payload, 200),
    ]);
    final client = stubClient(api);

    final releases = await BibleCatalogueService(client).fetch();
    expect(releases, hasLength(1));
    expect(releases.single.code, 'tst');
    expect(releases.single.version, 1);

    final downloads = BibleDownloadService(
      http: client,
      logger: MockMenoLogger(),
      supportDirectory: () async => cache,
    );

    final file = await downloads.ensureDownloaded(releases.single);
    expect(file.lengthSync(), payload.length);
    expect(file.path, endsWith('tst${Platform.pathSeparator}1.json.gz'));

    // The cache is the record: the second call answers from disk.
    await downloads.ensureDownloaded(releases.single);
    expect(api.requests, hasLength(2));

    final cached = await downloads.cachedPayloads();
    expect(cached, hasLength(1));
    expect(cached.single.code, 'tst');
    expect(cached.single.version, 1);
  });

  test('a payload that does not match the catalogue is refused', () async {
    final payload = _seedPayloadBytes();
    // Right length, wrong bytes — the case a length check alone would pass.
    final wrongDigest = sha256
        .convert(List<int>.filled(payload.length, 0))
        .toString();

    final api = StubApi([
      () async => _listEnvelope([
        _row(bytes: payload.length, sha256: wrongDigest),
      ]),
      () async => ResponseBody.fromBytes(payload, 200),
    ]);
    final client = stubClient(api);

    final release = (await BibleCatalogueService(client).fetch()).single;
    final downloads = BibleDownloadService(
      http: client,
      logger: MockMenoLogger(),
      supportDirectory: () async => cache,
    );

    await expectLater(
      downloads.ensureDownloaded(release),
      throwsA(isA<StorageException>()),
    );

    // Nothing was cached and no partial file was left behind.
    expect(await downloads.cachedPayloads(), isEmpty);
    final directory = await downloads.cacheDirectory('tst');
    if (directory.existsSync()) {
      expect(await directory.list().isEmpty, isTrue);
    }
  });

  test(
    'a cached payload seeds the store; an equal version is skipped',
    () async {
    await installTestDatabase();
    di.registerSingleton<MenoLogger>(MockMenoLogger());

    try {
      final file = File('${cache.path}/tst/1.json.gz');
      await file.parent.create(recursive: true);
      await file.writeAsBytes(_seedPayloadBytes());

      final service = BibleSeedService(translations: const []);
      final database = di<Database>();

      expect(
        await service.seedFromCache(code: 'tst', version: 1, file: file),
        isTrue,
      );
      expect(database.bibleVerseBox.count(), 2);
      expect(database.bibleSeedBox.getAll().single.seedVersion, 1);
      expect(database.bibleVerseBox.getAll().first.bookName, 'Genesis');

      // The ledger says this version is in: no rows are rewritten.
      expect(
        await service.seedFromCache(code: 'tst', version: 1, file: file),
        isFalse,
      );
      expect(database.bibleVerseBox.count(), 2);
    } finally {
      await resetTestLocator();
    }
  });
}
