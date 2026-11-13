import 'package:objectbox/objectbox.dart';

/// The `<translation>/<book>/<chapter>/<verse>` key a verse row is unique by.
///
/// The translation code is encoded as well as separated, the same way
/// `logicalKey()` guards the broadcast and notes rows, so a code that somehow
/// contains the separator cannot name a key belonging to another translation.
String bibleVerseKey(
  String translationCode,
  int book,
  int chapter,
  int verse,
) => '${Uri.encodeComponent(translationCode)}/$book/$chapter/$verse';

/// One row of the versions box: a translation's identity and attribution.
///
/// Deliberately small. The source JSON carries about twenty metadata fields
/// (description, publisher, module version, typography flags, ...) and the
/// design shows none of them, so they are not stored — four fields that are
/// read beat twenty that are not. The JSON stays the source of record; bringing
/// one back is a field here plus a bumped [BibleSeedRow.seedVersion].
///
/// A storage row, not a domain entity: ObjectBox needs a mutable identity and
/// its own key, which a `@freezed` entity cannot carry.
@Entity()
class BibleVersionRow {
  new({
    required this.code,
    required this.name,
    this.copyrightStatement = '',
    this.copyright = 0,
  });

  @Id()
  int id = 0;

  /// `kjv`, `web`, `asv` — the same code every verse of this translation holds.
  ///
  /// Unique with replace-on-conflict, so re-seeding a translation updates this
  /// row rather than leaving a second copy of it behind.
  @Unique(onConflict: .replace)
  final String code;

  final String name;

  /// The translation's copyright line, shown in the attribution.
  final String copyrightStatement;

  /// The source JSON's `copyright` flag, kept as the number it is there.
  final int copyright;
}

/// One verse of one translation.
///
/// Reference data, not user data: nothing in the app edits a verse, and the
/// seeder is the only writer. That is also why these rows carry no `ownerId` —
/// unlike drafts, favourites and notes, a verse does not belong to the account
/// that happens to be signed in.
@Entity()
class BibleVerseRow {
  new({
    required this.translationCode,
    required this.book,
    required this.chapter,
    required this.verse,
    this.text = '',
    this.bookName = '',
  }) : uniqueKey = bibleVerseKey(translationCode, book, chapter, verse);

  @Id()
  int id = 0;

  /// The translation this verse belongs to.
  @Index()
  final String translationCode;

  /// The book number the source JSON uses, where 1 is Genesis.
  @Index()
  final int book;

  @Index()
  final int chapter;

  final int verse;

  final String text;

  /// The book's name, expanded from the payload's book list.
  ///
  /// Denormalized on purpose: the payload carries 66 names once, the seeder
  /// repeats them onto ~31k rows, and every chapter read then answers without a
  /// second lookup. The cost is a few hundred kilobytes against a table that is
  /// already megabytes.
  final String bookName;

  /// One row per verse per translation.
  ///
  /// A unique index that replaces on conflict is what makes the seed
  /// re-runnable — a crash mid-seed, an app update that ships a corrected text,
  /// or a retry after the container was reset all rewrite rows that are already
  /// there instead of duplicating them, so a translation's count is always its
  /// real verse count.
  @Unique(onConflict: ConflictStrategy.replace)
  String uniqueKey;
}

/// What this device has already seeded, one row per translation.
///
/// The ledger lives in the store rather than in `SharedPreferences` so it can
/// be written in the same transaction as the rows it describes: there is no
/// window where the app believes it has a translation it does not, or the
/// reverse, and an interrupted seed is visible rather than silently trusted.
@Entity()
class BibleSeedRow {
  new({
    required this.translationCode,
    required this.seedVersion,
    this.verseCount = 0,
    int? seededAtMillis,
  }) : seededAtMillis =
           seededAtMillis ?? DateTime.now().millisecondsSinceEpoch;

  @Id()
  int id = 0;

  @Unique(onConflict: ConflictStrategy.replace)
  final String translationCode;

  /// The bundled asset's version, as the app's translation registry declares
  /// it.
  ///
  /// A translation is seeded when this is lower than the version the app
  /// ships, and skipped when they match. Bumping the asset's version is
  /// therefore what ships a corrected translation to devices that already have
  /// the old one.
  int seedVersion;

  /// How many verse rows this translation held when the seed last wrote it.
  ///
  /// This is the check a version number alone cannot make: a matching version
  /// with a count that does not match the box means the previous seed did not
  /// finish, so the translation is written again.
  int verseCount;

  /// When this row was last written, in milliseconds since the epoch.
  ///
  /// A number rather than a `DateTime`, following the notes rows: the value is
  /// only compared and shown, and a non-nullable column keeps the ordering
  /// simple.
  int seededAtMillis;
}
