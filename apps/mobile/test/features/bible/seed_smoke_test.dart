import 'dart:convert';
import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/database.dart';
import 'package:meno_mobile/features/bible/bible.dart';
import 'package:mocktail/mocktail.dart';

import '../../_support/test_database.dart';

class _MockLogger extends Mock implements MenoLogger;

class _MemoryBundle extends CachingAssetBundle {
  new(this.bytes);

  final Uint8List bytes;

  @override
  Future<ByteData> load(String key) async => ByteData.sublistView(bytes);
}

Uint8List _payloadBytes(List<List<Object>> verses, {int copyright = 0}) {
  final payload = <String, dynamic>{
    'format': 2,
    'code': 'tst',
    'name': 'Test Version',
    'copyrightStatement': 'free',
    'copyright': copyright,
    'books': [
      [1, 'Genesis'],
    ],
    'verses': verses,
  };
  return Uint8List.fromList(gzip.encode(utf8.encode(jsonEncode(payload))));
}

const _translation = BundledTranslation(
  code: 'tst',
  asset: 'assets/bible/tst.json.gz',
  version: 1,
);

void main() {
  setUp(() async {
    await installTestDatabase();
    di.registerSingleton<MenoLogger>(_MockLogger());
  });

  tearDown(resetTestLocator);

  test('seeds, then skips, then re-seeds on a version bump', () async {
    final database = di<Database>();
    final service = BibleSeedService(
      bundle: _MemoryBundle(
        _payloadBytes([
          [1, 1, 1, 'In the beginning'],
          [1, 1, 2, 'second'],
          [1, 2, 1, 'third'],
        ]),
      ),
      translations: const [_translation],
    );

    await service.ensureSeeded();

    expect(database.bibleVerseBox.count(), 3);
    expect(
      database.bibleVersionBox.query().build().findFirst()!.name,
      'Test Version',
    );
    expect(database.bibleVerseBox.getAll().first.bookName, 'Genesis');
    final ledger = database.bibleSeedBox.getAll().single;
    expect(ledger.seedVersion, 1);
    expect(ledger.verseCount, 3);

    // A second run has nothing to do.
    await service.ensureSeeded();
    expect(database.bibleVerseBox.count(), 3);
    expect(database.bibleSeedBox.count(), 1);

    // A bumped version replaces the rows instead of duplicating them.
    final bumped = BibleSeedService(
      bundle: _MemoryBundle(
        _payloadBytes([
          [1, 1, 1, 'corrected'],
        ], copyright: 1),
      ),
      translations: const [
        BundledTranslation(
          code: 'tst',
          asset: 'assets/bible/tst.json.gz',
          version: 2,
        ),
      ],
    );
    await bumped.ensureSeeded();

    expect(database.bibleVerseBox.count(), 1);
    expect(database.bibleVerseBox.getAll().single.text, 'corrected');
    expect(database.bibleSeedBox.getAll().single.seedVersion, 2);
    expect(database.bibleVersionBox.getAll().single.copyright, 1);
  });
}
