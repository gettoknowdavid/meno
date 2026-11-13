import 'dart:convert';
import 'dart:io';

/// Layout version of the seed payload.
///
/// Bumped whenever the shape changes, so the app can refuse a file it would
/// otherwise parse into the wrong rows.
const int seedPayloadFormat = 1;

/// Builds the payload the mobile app seeds its own ObjectBox store from.
///
/// The app cannot import a compiled `.mdb` — ObjectBox has no merge/import API
/// — so the shipped asset is data the app parses and writes through its own
/// entities. The layout is chosen to keep that cheap:
///
/// * `metadata` is the source JSON map verbatim, so nothing is dropped.
/// * `books` carries each book name once instead of on ~31k verse rows.
/// * `verses` are positional `[book, chapter, verse, text]` arrays.
Map<String, dynamic> buildSeedPayload({
  required String versionCode,
  required Map<String, dynamic> metadata,
  required List<dynamic> versesJson,
}) {
  final bookNames = <int, String>{};
  final verses = <List<Object>>[];

  for (final item in versesJson) {
    final verse = item as Map<String, dynamic>;
    final book = (verse['book'] as num).toInt();

    final bookName = verse['book_name'] as String?;
    if (bookName != null && bookName.isNotEmpty) bookNames[book] = bookName;

    verses.add([
      book,
      (verse['chapter'] as num).toInt(),
      (verse['verse'] as num).toInt(),
      verse['text'] as String,
    ]);
  }

  final books = bookNames.keys.toList()..sort();

  return {
    'format': seedPayloadFormat,
    'code': versionCode,
    'metadata': metadata,
    'books': [
      for (final book in books) [book, bookNames[book] ?? ''],
    ],
    'verses': verses,
  };
}

/// Writes the gzipped seed payload to [outputPath].
void writeSeedPayload({
  required String versionCode,
  required Map<String, dynamic> metadata,
  required List<dynamic> versesJson,
  required String outputPath,
}) {
  final payload = buildSeedPayload(
    versionCode: versionCode,
    metadata: metadata,
    versesJson: versesJson,
  );

  final bytes = gzip.encode(utf8.encode(jsonEncode(payload)));

  final file = File(outputPath);
  file.parent.createSync(recursive: true);
  file.writeAsBytesSync(bytes);

  final megabytes = (bytes.length / (1024 * 1024)).toStringAsFixed(2);
  stdout.writeln(
    'Seed: $outputPath '
    '(${payload['verses'].length} verses, $megabytes MB gzipped)',
  );
}
