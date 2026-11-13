import 'package:meno_mobile/features/bible/model/bible_rows.dart';

/// Renders a selection of verses as one reference, for example
/// `Romans 1:1-3,5`.
///
/// Only verses of the same chapter are ever selected (the reader shows one
/// chapter at a time), so the reference is built from the first verse's book
/// and chapter and the verse numbers are collapsed into runs.
abstract final class VerseReferenceFormatter {
  /// Formats [verses] — an empty selection formats to an empty string.
  static String format(List<BibleVerseRow> verses) {
    if (verses.isEmpty) return '';

    final sorted = [...verses]..sort((a, b) => a.verse.compareTo(b.verse));
    final first = sorted.first;

    final runs = <String>[];
    var runStart = first.verse;
    var runEnd = first.verse;

    for (final verse in sorted.skip(1)) {
      if (verse.verse == runEnd + 1) {
        runEnd = verse.verse;
        continue;
      }

      runs.add(_run(runStart, runEnd));
      runStart = verse.verse;
      runEnd = verse.verse;
    }
    runs.add(_run(runStart, runEnd));

    return '${first.bookName} ${first.chapter}:${runs.join(',')}';
  }

  static String _run(int start, int end) =>
      start == end ? '$start' : '$start-$end';
}
