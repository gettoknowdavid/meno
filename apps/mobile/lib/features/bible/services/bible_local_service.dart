import 'package:meno_mobile/_shared/services/database.dart';
import 'package:meno_mobile/features/bible/model/bible_rows.dart';
import 'package:meno_mobile/objectbox.g.dart';

/// Reads the seeded translations out of the device's store.
///
/// The store is the only source: every translation ships in the app and the
/// seeder writes it on first launch, so a read is a local query that answers
/// without a network.
///
/// Nothing here is scoped to an account — verses carry no owner — which is why
/// this service belongs in the root scope rather than the account's.
final class BibleLocalService {
  new(this._database);

  final Database _database;

  /// Every translation this device holds, ordered by code.
  List<BibleVersionRow> versions() {
    final query = _database.bibleVersionBox
        .query()
        .order(BibleVersionRow_.code)
        .build();
    try {
      return query.find();
    } finally {
      query.close();
    }
  }

  /// How many verses this device holds for [code].
  int verseTotal(String code) {
    final query = _database.bibleVerseBox
        .query(BibleVerseRow_.translationCode.equals(code))
        .build();
    try {
      return query.count();
    } finally {
      query.close();
    }
  }

  /// Removes [code] from the device: its verses, its version row and its seed
  /// ledger entry, in one transaction.
  ///
  /// The ledger goes with the verses on purpose. It is what `ensureSeeded` and
  /// the translations list read, so a ledger left behind would claim scripture
  /// that is no longer there and hide the translation from the catalogue —
  /// leaving neither something to read nor something to download.
  Future<void> remove(String code) async {
    await _database.runWriteTxAsync(_removeTranslation, code);
  }

  /// One translation's record, or null when it is not seeded.
  BibleVersionRow? version(String code) {
    final query = _database.bibleVersionBox
        .query(BibleVersionRow_.code.equals(code))
        .build();
    try {
      return query.findFirst();
    } finally {
      query.close();
    }
  }

  /// The version this device has seeded for [code], or 0 when it has none.
  ///
  /// Read from the seed ledger rather than the version row: the ledger is what
  /// the seeder writes in the same transaction as the verses, so it describes
  /// rows that are actually there. It is what makes "an update is available" a
  /// comparison instead of a download on every launch.
  int seedVersion(String code) {
    final query = _database.bibleSeedBox
        .query(BibleSeedRow_.translationCode.equals(code))
        .build();
    try {
      return query.findFirst()?.seedVersion ?? 0;
    } finally {
      query.close();
    }
  }

  /// The verses of one chapter, in verse order.
  List<BibleVerseRow> verses({
    required String translation,
    required int book,
    required int chapter,
  }) {
    final query = _database.bibleVerseBox
        .query(
          BibleVerseRow_.translationCode
              .equals(translation)
              .and(BibleVerseRow_.book.equals(book))
              .and(BibleVerseRow_.chapter.equals(chapter)),
        )
        .order(BibleVerseRow_.verse)
        .build();
    try {
      return query.find();
    } finally {
      query.close();
    }
  }

  /// The book numbers one translation holds, ascending.
  List<int> bookIds(String translation) {
    final query = _database.bibleVerseBox
        .query(BibleVerseRow_.translationCode.equals(translation))
        .build();
    try {
      final ids = <int>{for (final row in query.find()) row.book};
      return ids.toList()..sort();
    } finally {
      query.close();
    }
  }

  /// The book names of one translation, in book order.
  List<String> bookNames(String translation) {
    final query = _database.bibleVerseBox
        .query(BibleVerseRow_.translationCode.equals(translation))
        .order(BibleVerseRow_.book)
        .build();
    try {
      // One row per book is enough: the name is the same on every verse of it.
      final seen = <int>{};
      final names = <String>[];
      for (final row in query.find()) {
        if (!seen.add(row.book)) continue;
        names.add(row.bookName);
      }
      return names;
    } finally {
      query.close();
    }
  }

  /// How many chapters [book] has in this translation.
  int chapterCount(String translation, int book) {
    final query = _database.bibleVerseBox
        .query(
          BibleVerseRow_.translationCode
              .equals(translation)
              .and(BibleVerseRow_.book.equals(book)),
        )
        .build();
    try {
      var chapters = 0;
      for (final row in query.find()) {
        if (row.chapter > chapters) chapters = row.chapter;
      }
      return chapters;
    } finally {
      query.close();
    }
  }

  /// How many verses [chapter] of [book] holds.
  int verseCount(String translation, int book, int chapter) {
    final query = _database.bibleVerseBox
        .query(
          BibleVerseRow_.translationCode
              .equals(translation)
              .and(BibleVerseRow_.book.equals(book))
              .and(BibleVerseRow_.chapter.equals(chapter)),
        )
        .build();
    try {
      return query.count();
    } finally {
      query.close();
    }
  }

  /// Whether the device holds no verses at all yet.
  bool get isEmpty => _database.bibleVerseBox.count() == 0;
}

/// Deletes one translation's rows.
///
/// Top-level and capturing nothing, so it can run on ObjectBox's background
/// isolate. All three boxes go in one transaction: a store that lost the verses
/// but kept the ledger would look seeded forever and never download them again.
void _removeTranslation(Store store, String code) {
  final verses = store
      .box<BibleVerseRow>()
      .query(BibleVerseRow_.translationCode.equals(code))
      .build();
  try {
    verses.remove();
  } finally {
    verses.close();
  }

  final versions = store
      .box<BibleVersionRow>()
      .query(BibleVersionRow_.code.equals(code))
      .build();
  try {
    versions.remove();
  } finally {
    versions.close();
  }

  final seeds = store
      .box<BibleSeedRow>()
      .query(BibleSeedRow_.translationCode.equals(code))
      .build();
  try {
    seeds.remove();
  } finally {
    seeds.close();
  }
}
