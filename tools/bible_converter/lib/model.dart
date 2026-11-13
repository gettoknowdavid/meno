import 'package:objectbox/objectbox.dart';

/// A translation's identity and attribution.
///
/// Deliberately four fields. The source JSON carries about twenty more
/// (description, publisher, module version, typography flags, ...) and the
/// app's design shows none of them yet, so they are not stored. Adding one back
/// is a field here, a line in `mapMetadata`, and a regeneration — cheaper than
/// carrying columns nothing reads.
@Entity()
class BibleVersion {
  @Id()
  int id;

  /// `kjv`, `web`, `asv` — the same code every verse of this translation holds.
  String code;

  String name;

  /// The source JSON's `copyright_statement`.
  String copyrightStatement;

  /// The source JSON's `copyright` flag, kept as the number it is there.
  int copyright;

  BibleVersion({
    this.id = 0,
    required this.code,
    required this.name,
    this.copyrightStatement = '',
    this.copyright = 0,
  });
}

@Entity()
class Verse {
  @Id()
  int id;

  /// The code of the Bible version this verse belongs to.
  String versionCode;

  /// The book number of this verse.
  int book;

  /// The chapter number of this verse.
  int chapter;

  /// The verse number of this verse.
  int verse;

  /// The text of this verse.
  String text;

  /// The name of the book this verse belongs to.
  String bookName;

  Verse({
    this.id = 0,
    required this.versionCode,
    this.book = 0,
    this.chapter = 0,
    this.verse = 0,
    this.text = '',
    this.bookName = '',
  });
}
