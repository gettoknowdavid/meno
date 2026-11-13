import 'dart:io';

import 'package:bible_converter/json_reader.dart';
import 'package:bible_converter/model.dart';
import 'package:bible_converter/objectbox_writer.dart';

void convertJsonToObjectBox({
  required Map<String, dynamic> metadata,
  required List<dynamic> versesJson,
  required String versionCode,
  required String outputPath,
}) {
  final store = openTemporaryStore();
  final storeDir = store.directoryPath;

  try {
    final version = mapMetadata(versionCode, metadata);
    store.box<BibleVersion>().put(version);

    final verseBox = store.box<Verse>();
    const batchSize = 1000;
    var batch = <Verse>[];

    for (final verseItem in versesJson) {
      final json = verseItem as Map<String, dynamic>;
      batch.add(
        Verse(
          versionCode: versionCode,
          book: (json['book'] as num).toInt(),
          chapter: (json['chapter'] as num).toInt(),
          verse: (json['verse'] as num).toInt(),
          text: json['text'] as String,
          bookName: json['book_name'] as String? ?? '',
        ),
      );

      if (batch.length >= batchSize) {
        verseBox.putMany(batch);
        batch = <Verse>[];
      }
    }

    if (batch.isNotEmpty) {
      verseBox.putMany(batch);
    }
  } finally {
    // Close before copying so the data file is fully flushed to disk.
    store.close();
  }

  final source = File(dataFileIn(storeDir));
  if (!source.existsSync()) {
    throw StateError('ObjectBox data file was not created: ${source.path}');
  }

  final output = File(outputPath);
  output.parent.createSync(recursive: true);
  source.copySync(output.path);
}
