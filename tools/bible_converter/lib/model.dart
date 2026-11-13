import 'package:objectbox/objectbox.dart';

@Entity()
class BibleVersion {
  @Id()
  int id;
  String name;
  String code;
  String shortName;
  String language;
  String sourceModule;
  String year;
  String publisher;
  String owner;
  String copyrightStatement;
  String url;
  String lang;
  String langShort;
  String moduleVersion;
  String? description;
  int copyright;
  int citationLimit;
  int restrict;
  int italics;
  int strongs;
  int redLetter;
  int paragraph;
  int official;
  int research;
  String? audioStructure;

  BibleVersion({
    this.id = 0,
    required this.code,
    required this.name,
    required this.shortName,
    this.language = '',
    this.sourceModule = '',
    this.year = '',
    this.publisher = '',
    this.owner = '',
    this.copyrightStatement = '',
    this.url = '',
    this.lang = '',
    this.langShort = '',
    this.moduleVersion = '',
    this.description,
    this.copyright = 0,
    this.citationLimit = 0,
    this.restrict = 0,
    this.italics = 0,
    this.strongs = 0,
    this.redLetter = 0,
    this.paragraph = 0,
    this.official = 0,
    this.research = 0,
    this.audioStructure,
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
