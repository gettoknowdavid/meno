import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:isolate';
import 'dart:math';

import 'package:flutter/services.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/database.dart';
import 'package:meno_mobile/features/bible/model/bible_rows.dart';
import 'package:meno_mobile/objectbox.g.dart';

/// The payload layout this app understands.
///
/// Matches `seedPayloadFormat` in `tools/bible_converter`. An asset stamped
/// with anything else is not parsed at all: half-reading a layout that changed
/// would write rows the rest of the app cannot interpret.
const int _supportedSeedFormat = 2;

/// How many verses one write transaction takes.
///
/// Each transaction also updates the seed ledger's running count, so an
/// interruption loses at most this many verses of progress and the next launch
/// knows exactly where it stands.
const int _seedBatchSize = 1000;

/// One translation this build bundles.
final class BundledTranslation {
  const new({required this.code, required this.asset, required this.version});

  /// The code the verses are stored under, for example `kjv`.
  final String code;

  /// The asset the payload is read from.
  final String asset;

  /// The asset's content version.
  ///
  /// Bump it whenever the shipped file changes — a corrected text, a new verse
  /// count — and devices that already hold the old one re-seed just that
  /// translation. It is what makes shipping a fix possible without asking every
  /// install to uninstall.
  final int version;
}

/// What this build bundles, in the order seeding runs.
const List<BundledTranslation> bundledTranslations = [
  BundledTranslation(
    code: 'kjv',
    asset: 'assets/bible/kjv.json.gz',
    version: 1,
  ),
  BundledTranslation(
    code: 'web',
    asset: 'assets/bible/web.json.gz',
    version: 1,
  ),
  BundledTranslation(
    code: 'asv',
    asset: 'assets/bible/asv.json.gz',
    version: 1,
  ),
];

/// Copies the bundled translations into the device's ObjectBox store.
///
/// # Why the app must write these rows itself
///
/// The converter also emits a compiled ObjectBox file per translation, but
/// ObjectBox has no merge or import API: a file produced by the converter's
/// model cannot be folded into the app's store, which already holds notes,
/// drafts and favourites. So the shipped asset is the payload the converter
/// writes alongside that file, and seeding means parsing it and writing rows
/// through the app's own entities.
///
/// # When it runs
///
/// On every launch, in the background, without anybody signed in:
///
/// * It is device data, not account data — a verse carries no owner — so it
///   lives outside the account scope and is not repeated per account.
/// * It never blocks startup. `app.dart` waits on `di.allReady()` before the
///   first frame and this is deliberately not part of that chain; it is started
///   from `bootstrap()` and awaited by nobody.
/// * It is idempotent. [BibleSeedRow] says what is already there, the verse key
///   makes every write a replace, and a crash or a container reset simply means
///   the next call finishes the job.
///
/// [ensureSeeded] is also the await point for anything that needs the verses:
/// it returns immediately when the device is already current.
final class BibleSeedService {
  new({AssetBundle? bundle, List<BundledTranslation>? translations})
    : _bundle = bundle ?? rootBundle,
      _translations = translations ?? bundledTranslations;

  final AssetBundle _bundle;

  /// What this instance seeds.
  ///
  /// Injected so a test can seed one small translation instead of the three
  /// real ones; production always passes [bundledTranslations].
  final List<BundledTranslation> _translations;

  /// The run in flight, so two callers do one seed rather than two.
  Future<void>? _running;

  /// Brings the store up to date with the bundled translations.
  ///
  /// Safe to call on every launch and from more than one place: concurrent
  /// calls share one run, and a finished run leaves nothing to do. A failure is
  /// logged and swallowed — seeding must never take the app down or keep
  /// somebody from signing in.
  Future<void> ensureSeeded() {
    final running = _running;
    if (running != null) return running;

    final future = _seedAll().whenComplete(() => _running = null);
    _running = future;
    return future;
  }

  Future<void> _seedAll() async {
    final logger = di<MenoLogger>();

    final Database database;
    try {
      database = await _resolveDatabase();
    } on Object catch (error, stackTrace) {
      logger.w(
        'Bible seeding skipped: the store is not available',
        tag: MenoLogTag.lifecycle.label,
        error: error,
        stackTrace: stackTrace,
      );
      return;
    }

    for (final translation in _translations) {
      try {
        await _seedTranslation(database, translation, logger);
      } on Object catch (error, stackTrace) {
        // One bad translation does not stop the others, and a store that was
        // closed underneath this run (a container reset) is retried
        // next launch.
        logger.e(
          'Bible seeding failed for ${translation.code}',
          tag: MenoLogTag.lifecycle.label,
          error: error,
          stackTrace: stackTrace,
        );
      }
    }
  }

  Future<void> _seedTranslation(
    Database database,
    BundledTranslation translation,
    MenoLogger logger,
  ) async {
    final code = translation.code;
    final ledger = _ledgerFor(database, code);

    if (ledger != null &&
        ledger.seedVersion >= translation.version &&
        ledger.verseCount == _storedVerseCount(database, code)) {
      return;
    }

    final payload = await _loadPayload(translation.asset);

    final format = payload['format'];
    if (format != _supportedSeedFormat) {
      logger.w(
        'Skipped ${translation.code}: asset format $format is not '
        '$_supportedSeedFormat',
        tag: MenoLogTag.lifecycle.label,
      );
      return;
    }

    final assetCode = payload['code'] as String?;
    if (assetCode != code) {
      logger.w(
        'Skipped ${translation.code}: the asset names itself "$assetCode"',
        tag: MenoLogTag.lifecycle.label,
      );
      return;
    }

    final verses = payload['verses'] as List<dynamic>;
    if (verses.isEmpty) {
      logger.w(
        'Skipped $code: the asset holds no verses',
        tag: MenoLogTag.lifecycle.label,
      );
      return;
    }

    final bookNames = _bookNames(payload);
    final versionRow = BibleVersionRow(
      code: code,
      name: payload['name'] as String? ?? code,
      copyrightStatement: payload['copyrightStatement'] as String? ?? '',
      copyright: (payload['copyright'] as num?)?.toInt() ?? 0,
    );

    var written = 0;
    var replaceExisting = true;

    for (var start = 0; start < verses.length; start += _seedBatchSize) {
      final end = min(start + _seedBatchSize, verses.length);
      final rows = <BibleVerseRow>[
        for (var index = start; index < end; index++)
          _rowFrom(code, verses[index], bookNames),
      ];

      final isLastBatch = end == verses.length;
      written = end;

      await database.runWriteTxAsync<void, _SeedBatch>(
        _writeSeedBatch,
        _SeedBatch(
          rows: rows,
          code: code,
          versionRow: versionRow,
          verseCount: written,
          version: translation.version,
          replaceExisting: replaceExisting,
          isLastBatch: isLastBatch,
        ),
      );
      replaceExisting = false;
    }

    logger.i('Seeded $code ($written verses)', tag: MenoLogTag.lifecycle.label);
  }

  /// The store to seed into.
  ///
  /// The app registers it as an async singleton because opening it awaits the
  /// documents directory, so this runs before that opening may have finished
  /// (it is started from `bootstrap`, not from a dependency chain). A test
  /// registers a plain singleton and gets it immediately.
  Future<Database> _resolveDatabase() {
    if (di.isRegistered<Database>()) {
      try {
        return Future.value(di<Database>());
      } on Object {
        // Registered asynchronously and still opening: fall through and wait.
      }
    }
    return di.getAsync<Database>();
  }

  /// The ledger row for [code], or null when this device has never seeded it.
  BibleSeedRow? _ledgerFor(Database database, String code) {
    final query = database.bibleSeedBox
        .query(BibleSeedRow_.translationCode.equals(code))
        .build();
    try {
      return query.findFirst();
    } finally {
      query.close();
    }
  }

  /// How many verse rows the store actually holds for [code].
  int _storedVerseCount(Database database, String code) {
    final query = database.bibleVerseBox
        .query(BibleVerseRow_.translationCode.equals(code))
        .build();
    try {
      return query.count();
    } finally {
      query.close();
    }
  }

  /// Builds one verse row from a positional payload entry.
  BibleVerseRow _rowFrom(
    String code,
    dynamic entry,
    Map<int, String> bookNames,
  ) {
    final verse = entry as List<dynamic>;
    final book = (verse[0] as num).toInt();

    return BibleVerseRow(
      translationCode: code,
      book: book,
      chapter: (verse[1] as num).toInt(),
      verse: (verse[2] as num).toInt(),
      text: verse[3] as String,
      bookName: bookNames[book] ?? '',
    );
  }

  /// Expands the payload's book list into a lookup by book number.
  Map<int, String> _bookNames(Map<String, dynamic> payload) {
    final books = payload['books'] as List<dynamic>;
    return {
      for (final entry in books)
        ((entry as List<dynamic>)[0] as num).toInt(): entry[1] as String,
    };
  }

  /// Loads and decodes one bundled asset.
  Future<Map<String, dynamic>> _loadPayload(String asset) async {
    final data = await _bundle.load(asset);
    final bytes = data.buffer.asUint8List(
      data.offsetInBytes,
      data.lengthInBytes,
    );

    // Off the UI isolate: gunzipping and parsing a few megabytes of JSON is the
    // most expensive thing this service does, and it happens while somebody is
    // signing in.
    return await Isolate.run(() => decodeSeedPayload(bytes));
  }
}

/// One batch of verses, with everything the write transaction needs.
///
/// The transaction body runs in its own isolate, so this value has to carry
/// all of it: a closure over the service would try to send the service — and
/// the `Future` it holds — which cannot cross an isolate boundary.
final class _SeedBatch {
  const new({
    required this.rows,
    required this.code,
    required this.versionRow,
    required this.verseCount,
    required this.version,
    required this.replaceExisting,
    required this.isLastBatch,
  });

  final List<BibleVerseRow> rows;
  final String code;
  final BibleVersionRow versionRow;

  /// How many verses of this translation exist once this batch is written.
  final int verseCount;

  /// The asset version the seed is working towards.
  final int version;

  /// Whether this is the first batch of a re-seed, so the old rows go first.
  final bool replaceExisting;

  /// Whether this batch completes the translation.
  final bool isLastBatch;
}

/// Writes one batch, the version record and the ledger in one transaction.
///
/// Top-level on purpose: `runInTransactionAsync` runs this off the UI isolate,
/// and only a function that captures nothing can be sent there.
void _writeSeedBatch(Store store, _SeedBatch batch) {
  // The old rows go in the same transaction as the first new ones, so a re-seed
  // never leaves a chapter half old and half new.
  if (batch.replaceExisting) {
    final stale = store
        .box<BibleVerseRow>()
        .query(BibleVerseRow_.translationCode.equals(batch.code))
        .build();
    try {
      stale.remove();
    } finally {
      stale.close();
    }
  }

  store.box<BibleVersionRow>().put(batch.versionRow);
  store.box<BibleVerseRow>().putMany(batch.rows);
  // The version reaches its target only on the last batch: an interrupted seed
  // stays below it and is therefore retried.
  store.box<BibleSeedRow>().put(
    BibleSeedRow(
      translationCode: batch.code,
      seedVersion: batch.isLastBatch ? batch.version : 0,
      verseCount: batch.verseCount,
    ),
  );
}

/// Gunzips and parses one seed payload.
///
/// Top-level and synchronous so it can be handed to [Isolate.run].
Map<String, dynamic> decodeSeedPayload(Uint8List bytes) =>
    jsonDecode(utf8.decode(gzip.decode(bytes))) as Map<String, dynamic>;
