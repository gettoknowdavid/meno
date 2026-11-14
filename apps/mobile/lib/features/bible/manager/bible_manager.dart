import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/bible/model/_model.dart';
import 'package:meno_mobile/features/bible/services/_services.dart';

/// Manages the state of the Bible reader, including navigation and selection.
///
/// The verses come from the device's store through [BibleLocalService] — every
/// translation is bundled and seeded, so a chapter is a local read with no
/// request behind it.
///
/// Book position is an index (`0` is Genesis) because that is what the pickers
/// walk; the store keys verses by the source's book number instead. The two are
/// kept in step by [bookIdsLength], which is read once per translation and
/// cached.
final class BibleManager implements Disposable {
  new({required this._local, required this._logger});

  final BibleLocalService _local;
  final MenoLogger _logger;

  // =========================================================================
  // CORE STATE
  // =========================================================================

  /// Index into the book list (0 = Genesis).
  final book = ValueNotifier<int>(0);

  /// 1-based chapter number.
  final chapter = ValueNotifier<int>(1);

  /// 1-based verse number — null means the whole chapter is shown.
  final verse = ValueNotifier<int?>(null);

  /// Active translation code (e.g. 'kjv').
  final translation = ValueNotifier<String>('kjv');

  /// Current chapter's verses.
  final verses = ListNotifier<BibleVerseRow>(data: []);

  // =========================================================================
  // MULTI-VERSE SELECTION
  // =========================================================================

  /// Verses the user has selected for copy / send-to-chat.
  /// Always kept sorted ascending by verse number.
  final selectedVerses = ListNotifier<BibleVerseRow>(data: []);

  // =========================================================================
  // CACHED BOOK LIST
  // =========================================================================

  String? _booksCode;
  List<int> _bookIds = const [];
  List<String> _names = const [];

  /// Reads the book list for the active translation, once per translation.
  ///
  /// Cheap enough to do on demand: 66 book numbers and their names, read from
  /// an indexed box, and only when the translation has changed.
  void _ensureBooks() {
    final code = translation.value;
    if (_booksCode == code) return;

    _bookIds = _local.bookIds(code);
    _names = _local.bookNames(code);
    _booksCode = code;
  }

  /// The book number the store uses for the book at [index].
  int? _bookIdAt(int index) {
    _ensureBooks();
    if (index < 0 || index >= _bookIds.length) return null;
    return _bookIds[index];
  }

  /// All book names in canonical order.
  List<String> get bookNames {
    _ensureBooks();
    return _names;
  }

  /// Name for a given 0-based book index.
  String bookName(int id) {
    _ensureBooks();
    if (id < 0 || id >= _names.length) return '';
    return _names[id];
  }

  /// Currently active book name.
  String get currentBookName => bookName(book.value);

  /// Chapter count for the currently active book.
  int get chapterCount {
    final id = _bookIdAt(book.value);
    return id == null ? 0 : _local.chapterCount(translation.value, id);
  }

  /// Verse count for the currently active chapter.
  int get verseCount {
    final id = _bookIdAt(book.value);
    return id == null
        ? 0
        : _local.verseCount(translation.value, id, chapter.value);
  }

  // =========================================================================
  // NAVIGATION GUARDS
  // =========================================================================

  bool get isFirstChapter => book.value == 0 && chapter.value <= 1;

  bool get isLastChapter {
    _ensureBooks();
    if (_bookIds.isEmpty) return true;

    final lastIndex = _bookIds.length - 1;
    return book.value >= lastIndex &&
        chapter.value >=
            _local.chapterCount(translation.value, _bookIds[lastIndex]);
  }

  // =========================================================================
  // COMPUTED REFERENCE
  // =========================================================================

  /// Reactive combined reference string — e.g. "Romans 1".
  late final ValueListenable<String> reference = book.combineLatest(
    chapter,
    (b, c) => '${bookName(b)} $c',
  );

  // =========================================================================
  // COMMANDS
  // =========================================================================

  /// Load verses for explicit [BibleArgs]. Also resets selection.
  late final Command<BibleArgs, void> getVerses =
      Command.createSyncNoResult<BibleArgs>(
        (args) {
          book.value = args.book;
          chapter.value = args.chapter;
          verse.value = args.verse;

          // Prefer the args translation; fall back to the manager's active one.
          final trans = args.translation.isNotEmpty
              ? args.translation
              : translation.value;

          _loadVerses(
            bookIndex: args.book,
            chapterNum: args.chapter,
            trans: trans,
          );
        },
        errorFilterFn: menoExceptionFilter,
        debugName: 'bible.getVerses',
      );

  /// Reload the current chapter (used after translation change).
  late final Command<void, void> refreshChapter =
      Command.createSyncNoParamNoResult(
        () => _loadVerses(
          bookIndex: book.value,
          chapterNum: chapter.value,
          trans: translation.value,
        ),
        errorFilterFn: menoExceptionFilter,
        debugName: 'bible.refreshChapter',
      );

  /// Navigate to the next chapter, crossing book boundaries.
  late final Command<void, void> nextChapter =
      Command.createSyncNoParamNoResult(
        () {
          if (isLastChapter) return;

          final chapters = chapterCount;
          if (chapter.value < chapters) {
            chapter.value++;
            return;
          }

          final nextBookIdx = book.value + 1;
          if (nextBookIdx < bookIdsLength) {
            book.value = nextBookIdx;
            chapter.value = 1;
          }
        },
        errorFilterFn: menoExceptionFilter,
        debugName: 'bible.nextChapter',
      )..pipeToCommand(refreshChapter);

  /// Navigate to the previous chapter, crossing book boundaries.
  late final Command<void, void> prevChapter =
      Command.createSyncNoParamNoResult(
        () {
          if (isFirstChapter) return;

          if (chapter.value > 1) {
            chapter.value--;
            return;
          }

          final prevBookIdx = book.value - 1;
          if (prevBookIdx >= 0) {
            book.value = prevBookIdx;
            final id = _bookIdAt(prevBookIdx);
            chapter.value = id == null
                ? 1
                : _local.chapterCount(translation.value, id);
          }
        },
        errorFilterFn: menoExceptionFilter,
        debugName: 'bible.prevChapter',
      )..pipeToCommand(refreshChapter);

  /// How many books the active translation holds.
  int get bookIdsLength {
    _ensureBooks();
    return _bookIds.length;
  }

  // =========================================================================
  // SELECTION
  // =========================================================================

  /// Toggle [v] in/out of the selection set. Keeps list sorted.
  void toggleVerseSelection(BibleVerseRow v) {
    final current = List<BibleVerseRow>.from(selectedVerses.value);
    final existingIdx = current.indexWhere((s) => s.verse == v.verse);

    if (existingIdx != -1) {
      current.removeAt(existingIdx);
    } else {
      current.add(v);
      current.sort((a, b) => a.verse.compareTo(b.verse));
    }

    selectedVerses.startTransAction();
    selectedVerses.clear();
    selectedVerses.addAll(current);
    selectedVerses.endTransAction();
  }

  void clearSelection() {
    selectedVerses.startTransAction();
    selectedVerses.clear();
    selectedVerses.endTransAction();
  }

  /// Formatted reference string for the current selection —
  /// e.g. "Romans 1:1-3,5" — or empty if nothing is selected.
  String get selectionReference {
    final sel = List<BibleVerseRow>.from(selectedVerses.value);
    if (sel.isEmpty) return '';
    return VerseReferenceFormatter.format(sel);
  }

  /// Plain text of the current selection suitable for chat / clipboard.
  /// Format: `<Reference>\n[1] verse text [2] verse text …`
  String get selectionText {
    final sel = List<BibleVerseRow>.from(selectedVerses.value);
    if (sel.isEmpty) return '';
    final ref = selectionReference;
    final body = sel.map((v) => '[${v.verse}] ${v.text}').join(' ');
    return '$ref\n$body';
  }

  // =========================================================================
  // PRIVATE HELPERS
  // =========================================================================

  void _loadVerses({
    required int bookIndex,
    required int chapterNum,
    required String trans,
  }) {
    final bookId = _bookIdAt(bookIndex);
    final result = bookId == null
        ? <BibleVerseRow>[]
        : _local.verses(translation: trans, book: bookId, chapter: chapterNum);

    verses.startTransAction();
    verses.clear();
    verses.addAll(result);
    verses.endTransAction();

    // Clear selection whenever the chapter/book changes.
    clearSelection();
  }

  // =========================================================================
  // DISPOSE
  // =========================================================================

  @override
  FutureOr<dynamic> onDispose() {
    _logger.i('BibleManager: Disposing…', tag: MenoLogTag.lifecycle.label);

    book.dispose();
    chapter.dispose();
    verse.dispose();
    translation.dispose();
    verses.dispose();
    selectedVerses.dispose();

    getVerses.dispose();
    refreshChapter.dispose();
    nextChapter.dispose();
    prevChapter.dispose();
  }
}

/// Value object representing a specific location in the Bible.
final class BibleArgs {
  const new({
    this.book = 0,
    this.chapter = 1,
    this.verse,
    this.translation = 'kjv',
  });

  /// 0-based book index (0 = Genesis).
  final int book;

  /// 1-based chapter number.
  final int chapter;

  /// 1-based verse number. Null means the whole chapter.
  final int? verse;

  /// Optional translation code. If empty, manager's default is used.
  final String translation;

  BibleArgs copyWith({
    int? book,
    int? chapter,
    int? verse,
    String? translation,
  }) {
    return BibleArgs(
      book: book ?? this.book,
      chapter: chapter ?? this.chapter,
      verse: verse ?? this.verse,
      translation: translation ?? this.translation,
    );
  }
}
