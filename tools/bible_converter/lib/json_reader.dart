import 'dart:convert';
import 'dart:io';

import 'package:bible_converter/model.dart';

Map<String, dynamic> readJsonFile(String path) {
  final text = File(path).readAsStringSync();
  return jsonDecode(text) as Map<String, dynamic>;
}

BibleVersion mapMetadata(String versionCode, Map<String, dynamic> metadata) {
  int toInt(dynamic value) {
    if (value is int) return value;
    if (value is num) return value.toInt();
    return 0;
  }

  return BibleVersion(
    code: versionCode,
    name: metadata['name'] as String,
    shortName: metadata['shortname'] as String,
    language: metadata['lang'] as String? ?? '',
    sourceModule: metadata['module'] as String? ?? '',
    year: metadata['year'] as String? ?? '',
    publisher: metadata['publisher'] as String? ?? '',
    owner: metadata['owner'] as String? ?? '',
    copyrightStatement: metadata['copyright_statement'] as String? ?? '',
    url: metadata['url'] as String? ?? '',
    lang: metadata['lang'] as String? ?? '',
    langShort: metadata['lang_short'] as String? ?? '',
    moduleVersion: metadata['module_version'] as String? ?? '',
    description: metadata['description'] as String?,
    copyright: toInt(metadata['copyright']),
    citationLimit: toInt(metadata['citation_limit']),
    restrict: toInt(metadata['restrict']),
    italics: toInt(metadata['italics']),
    strongs: toInt(metadata['strongs']),
    redLetter: toInt(metadata['red_letter']),
    paragraph: toInt(metadata['paragraph']),
    official: toInt(metadata['official']),
    research: toInt(metadata['research']),
    audioStructure: metadata['audio_structure'] as String?,
  );
}

List<Verse> mapVerses(String versionCode, List<dynamic> verses) {
  final result = verses.map((item) {
    final json = item as Map<String, dynamic>;
    return Verse(
      versionCode: versionCode,
      book: (json['book'] as num).toInt(),
      chapter: (json['chapter'] as num).toInt(),
      verse: (json['verse'] as num).toInt(),
      text: json['text'] as String,
      bookName: json['book_name'] as String? ?? '',
    );
  }).toList();
  return result;
}
