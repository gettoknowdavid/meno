import 'dart:convert';
import 'dart:io';

import 'package:bible_converter/model.dart';

Map<String, dynamic> readJsonFile(String path) {
  final text = File(path).readAsStringSync();
  return jsonDecode(text) as Map<String, dynamic>;
}

/// Maps the source JSON's `metadata` object onto the stored version record.
///
/// Only the four fields the app shows are taken. Everything else in the source
/// metadata (description, publisher, module version, typography flags, ...) is
/// intentionally left behind: the app's design does not display it, and the
/// JSON remains the source of record if it is ever needed again.
BibleVersion mapMetadata(String versionCode, Map<String, dynamic> metadata) {
  return BibleVersion(
    code: versionCode,
    name: metadata['name'] as String? ?? versionCode,
    copyrightStatement: metadata['copyright_statement'] as String? ?? '',
    copyright: toInt(metadata['copyright']),
  );
}

/// Reads a JSON number as an int, treating anything absent or unparseable as 0.
int toInt(dynamic value) {
  if (value is int) return value;
  if (value is num) return value.toInt();
  return 0;
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
