import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/notes/notes.dart';
import 'package:mocktail/mocktail.dart';

import '../../_support/stub_api.dart';

const _noteId = '3f2b1c9e-8a4d-4f6b-9c1e-2d3a4b5c6d7e';
const _otherNoteId = '3f2b1c9e-8a4d-4f6b-9c1e-2d3a4b5c6d7f';
const _folderId = '3f2b1c9e-8a4d-4f6b-9c1e-2d3a4b5c6d80';

final class _MockLocal extends Mock implements NotesLocalService;

/// A note as the device holds it.
Note _note({
  String id = _noteId,
  bool confirmed = true,
  bool deleted = false,
  bool pinned = false,
  int version = 1,
}) => Note(
  id: Id.fromString(id),
  title: SingleLineString('Morning'),
  content: MultiLineString('{"ops":[]}'),
  pinned: pinned,
  version: version,
  createdAt: confirmed ? DateTime.utc(2026, 1, 2, 9) : null,
  updatedAt: confirmed ? DateTime.utc(2026, 1, 3, 10) : null,
  deleted: deleted,
);

NoteFolder _folder({bool confirmed = true, bool deleted = false}) =>
    NoteFolder(
      id: Id.fromString(_folderId),
      title: SingleLineString('Prayers'),
      version: 2,
      createdAt: confirmed ? DateTime.utc(2026, 1, 2, 9) : null,
      updatedAt: confirmed ? DateTime.utc(2026, 1, 3, 10) : null,
      deleted: deleted,
    );

Map<String, dynamic> _noteJson({String id = _noteId, bool pinned = false}) => {
  'id': id,
  'title': 'Morning',
  'content': '{"ops":[]}',
  'pinned': pinned,
  'version': 4,
  'createdAt': '2026-01-02T09:00:00.000Z',
  'updatedAt': '2026-01-03T10:00:00.000Z',
  'deleted': false,
};

Map<String, dynamic> _folderJson() => {
  'id': _folderId,
  'title': 'Prayers',
  'pinned': false,
  'version': 3,
  'createdAt': '2026-01-02T09:00:00.000Z',
  'updatedAt': '2026-01-03T10:00:00.000Z',
  'deleted': false,
};

void main() {
  late _MockLocal local;

  setUpAll(() {
    registerFallbackValue(Note.empty);
    registerFallbackValue(Id.empty);
    registerFallbackValue(NoteFolder.empty);
  });

  setUp(() {
    local = _MockLocal();
    when(() => local.saveNote(any(), pending: any(named: 'pending')))
        .thenAnswer((_) async {});
    when(() => local.saveFolder(any(), pending: any(named: 'pending')))
        .thenAnswer((_) async {});
    when(() => local.removeNote(any())).thenAnswer((_) async {});
    when(() => local.removeFolder(any())).thenAnswer((_) async {});
    when(() => local.pendingNotes()).thenReturn([]);
    when(() => local.pendingFolders()).thenReturn([]);
  });

  NotesSyncService syncOver(StubApi api) => NotesSyncService(
    http: NotesHttpService(stubClient(api)),
    local: local,
    logger: MockMenoLogger(),
  );

  test('a note the server never saw is created under its own id', () async {
    final api = StubApi([
      () async => envelope(data: _noteJson()),
    ]);

    await syncOver(api).pushNote(_note(confirmed: false));

    final request = api.requests.single;
    expect(request.method, 'POST');
    expect(request.path, '/notes');
    expect((request.data as Map<String, dynamic>)['id'], _noteId);
  });

  test('a confirmed note is patched with the version it holds', () async {
    final api = StubApi([
      () async => envelope(data: _noteJson()),
    ]);

    await syncOver(api).pushNote(_note(version: 3));

    final request = api.requests.single;
    expect(request.method, 'PATCH');
    expect(request.path, '/notes/$_noteId');
    expect((request.data as Map<String, dynamic>)['baseVersion'], 3);
  });

  test('an answered write is stored as confirmed', () async {
    final api = StubApi([
      () async => envelope(data: _noteJson()),
    ]);

    await syncOver(api).pushNote(_note(confirmed: false));

    final stored = verify(
      () => local.saveNote(captureAny(), pending: false),
    ).captured.single as Note;

    expect(stored.version, 4);
    expect(stored.createdAt, isNotNull);
    expect(stored.isNew, isFalse);
  });

  test('a folder created offline is a create, once', () async {
    final api = StubApi([
      () async => envelope(data: _folderJson()),
    ]);

    await syncOver(api).pushFolder(_folder(confirmed: false));

    expect(api.requests.single.method, 'POST');
    expect(api.requests.single.path, '/folders');
  });

  test('a failed send leaves the row pending instead of throwing', () async {
    final api = StubApi([
      () async => envelope(status: 500),
    ]);
    when(() => local.pendingNotes()).thenReturn([_note()]);

    await syncOver(api).pushPending();

    expect(api.requests, hasLength(1));
    verifyNever(() => local.saveNote(any(), pending: any(named: 'pending')));
  });

  test('one refused row does not hold back the one behind it', () async {
    final api = StubApi([
      () async => envelope(status: 500),
      () async => envelope(data: _noteJson(id: _otherNoteId)),
    ]);
    when(() => local.pendingNotes()).thenReturn([
      _note(),
      _note(id: _otherNoteId),
    ]);

    await syncOver(api).pushPending();

    expect(api.requests, hasLength(2));
    verify(() => local.saveNote(any(), pending: false)).called(1);
  });

  test('deleting a note the server never saw sends nothing', () async {
    final api = StubApi();

    await syncOver(api).pushNote(_note(confirmed: false, deleted: true));

    expect(api.requests, isEmpty);
    verify(() => local.removeNote(any())).called(1);
  });

  test('deleting a confirmed note is a versioned delete', () async {
    final api = StubApi([() async => envelope()]);

    await syncOver(api).pushNote(_note(version: 5, deleted: true));

    final request = api.requests.single;
    expect(request.method, 'DELETE');
    expect(request.path, '/notes/$_noteId');
    expect((request.data as Map<String, dynamic>)['baseVersion'], 5);
  });

  test('a batch move is one request', () async {
    final api = StubApi([() async => envelope()]);

    await syncOver(api).moveNotes(
      [Id.fromString(_noteId), Id.fromString(_otherNoteId)],
      folderId: Id.fromString(_folderId),
    );

    final request = api.requests.single;
    expect(request.method, 'PUT');
    expect(request.path, '/notes');
    expect((request.data as Map<String, dynamic>)['noteIds'], hasLength(2));
    expect(
      (request.data as Map<String, dynamic>)['folderId'],
      _folderId,
    );
  });
}
