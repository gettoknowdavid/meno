import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/notes/notes.dart';
import 'package:mocktail/mocktail.dart';

import '../../_support/stub_api.dart';

const _noteId = '3f2b1c9e-8a4d-4f6b-9c1e-2d3a4b5c6d7e';

final class _MockLocal extends Mock implements NotesLocalService;

Note _note({String id = _noteId, bool pinned = false}) => Note(
  id: Id.fromString(id),
  title: SingleLineString('Morning'),
  content: MultiLineString('{"ops":[]}'),
  pinned: pinned,
  version: 2,
  createdAt: DateTime.utc(2026, 1, 2, 9),
  updatedAt: DateTime.utc(2026, 1, 3, 10),
);

void main() {
  late _MockLocal local;
  late NotesHttpService http;
  late NotesSyncService sync;
  late StreamController<List<Note>> rows;
  late FoldersManager folders;
  late NotesManager manager;

  setUpAll(() {
    registerFallbackValue(Note.empty);
    registerFallbackValue(NoteFolder.empty);
    registerFallbackValue(Id.empty);
  });

  setUp(() {
    local = _MockLocal();
    rows = StreamController<List<Note>>.broadcast();

    when(
      () => local.watchNotes(
        pinned: any(named: 'pinned'),
        keywords: any(named: 'keywords'),
      ),
    ).thenAnswer((_) => rows.stream);
    when(
      () => local.watchFolders(keywords: any(named: 'keywords')),
    ).thenAnswer((_) => const Stream<List<NoteFolder>>.empty());
    when(() => local.allNotes()).thenReturn([]);
    when(() => local.countNotes()).thenReturn(0);
    when(() => local.countFolders()).thenReturn(0);
    when(() => local.note(any())).thenReturn(null);
    when(
      () => local.saveNote(any(), pending: any(named: 'pending')),
    ).thenAnswer((_) async {});

    http = NotesHttpService(stubClient(StubApi()));
    sync = NotesSyncService(
      http: http,
      local: local,
      logger: MockMenoLogger(),
    );

    folders = FoldersManager(
      http: http,
      local: local,
      sync: sync,
      logger: MockMenoLogger(),
    );
    manager = NotesManager(
      http: http,
      local: local,
      sync: sync,
      folders: folders,
      logger: MockMenoLogger(),
    );

    addTearDown(manager.onDispose);
    addTearDown(folders.onDispose);
    addTearDown(rows.close);
  });

  test('the list is what the device holds', () async {
    rows.add([_note(), _note(id: '3f2b1c9e-8a4d-4f6b-9c1e-2d3a4b5c6d7f')]);
    await Future<void>.delayed(Duration.zero);

    expect(manager.notes.value, hasLength(2));
  });

  test('the totals come from the store, not from the drawn list', () async {
    when(() => local.allNotes()).thenReturn([
      _note(),
      _note(id: '3f2b1c9e-8a4d-4f6b-9c1e-2d3a4b5c6d71'),
      _note(id: '3f2b1c9e-8a4d-4f6b-9c1e-2d3a4b5c6d72'),
    ]);

    // A search narrows the drawing to one note...
    rows.add([_note()]);
    await Future<void>.delayed(Duration.zero);

    expect(manager.notes.value, hasLength(1));
    // ...but the account still holds three.
    expect(manager.totalNotesCount.value, 3);
  });

  test('narrowing the pinned filter re-reads the local query', () async {
    clearInteractions(local);

    manager.setPinnedFilter(true);
    await Future<void>.delayed(Duration.zero);

    verify(
      () => local.watchNotes(
        pinned: true,
        keywords: any(named: 'keywords'),
      ),
    ).called(1);
  });

  test('an edited note is written through the store first', () async {
    final note = _note();

    await manager.remember(note);

    verify(() => local.saveNote(note, pending: true)).called(1);
  });

  test('a note is read from the store even when the list hides it', () {
    final note = _note(pinned: true);
    when(() => local.note(any())).thenReturn(note);

    expect(manager.byId(note.id), note);
  });

  test('a deleted note is not handed to an editor', () {
    when(() => local.note(any())).thenReturn(_note().copyWith(deleted: true));

    // A deleted row is still in the store until the server confirms it, and it
    // must not be reopened as if it were a note.
    expect(manager.byId(Id.fromString(_noteId)), isNull);
  });
}
