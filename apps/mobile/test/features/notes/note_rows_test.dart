import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/notes/notes.dart';

const _noteId = '3f2b1c9e-8a4d-4f6b-9c1e-2d3a4b5c6d7e';
const _folderId = '3f2b1c9e-8a4d-4f6b-9c1e-2d3a4b5c6d80';
const _owner = 'c0ffee00-1111-4222-8333-444455556666';

void main() {
  group('a note row', () {
    test('carries the note it was written from', () {
      final note = Note(
        id: Id.fromString(_noteId),
        title: SingleLineString('Morning'),
        content: MultiLineString('{"ops":[]}'),
        pinned: true,
        folderId: Id.fromString(_folderId),
        version: 4,
        createdAt: DateTime.utc(2026, 1, 2, 9),
        updatedAt: DateTime.utc(2026, 1, 3, 10),
      );

      final row = note.toRow(ownerId: _owner);

      expect(row.noteId, _noteId);
      expect(row.ownerId, _owner);
      expect(row.uniqueKey, noteLogicalKey(_owner, _noteId));
      expect(row.title, 'Morning');
      expect(row.pinned, isTrue);
      expect(row.folderId, _folderId);
      expect(row.version, 4);
      expect(row.pending, isFalse);
      expect(
        row.createdAtMillis,
        DateTime.utc(2026, 1, 2, 9).millisecondsSinceEpoch,
      );
    });

    test('reads back as the same note', () {
      final note = Note(
        id: Id.fromString(_noteId),
        title: SingleLineString('Morning'),
        content: MultiLineString('{"ops":[]}'),
        pinned: true,
        folderId: Id.fromString(_folderId),
        version: 2,
        createdAt: DateTime.utc(2026, 1, 2, 9),
        updatedAt: DateTime.utc(2026, 1, 3, 10),
      );

      final restored = note.toRow(ownerId: _owner).toDomain;

      expect(restored, note);
    });

    test('keeps an unsent note unsent after a round trip', () {
      final note = Note(
        id: Id.fromString(_noteId),
        title: SingleLineString('Typed offline'),
        content: MultiLineString.empty,
      );

      final row = note.toRow(ownerId: _owner, pending: true);
      final restored = row.toDomain;

      expect(row.pending, isTrue);
      expect(restored.createdAt, isNull);
      expect(restored.updatedAt, isNull);
      expect(restored.isNew, isTrue);
    });

    test('separates two accounts that share a logical id', () {
      final note = Note(
        id: Id.fromString(_noteId),
        title: SingleLineString('Mine'),
        content: MultiLineString.empty,
      );

      final mine = note.toRow(ownerId: _owner);
      final theirs = note.toRow(ownerId: _folderId);

      expect(mine.uniqueKey, isNot(theirs.uniqueKey));
    });
  });

  group('a folder row', () {
    test('reads back as the same folder', () {
      final folder = NoteFolder(
        id: Id.fromString(_folderId),
        title: SingleLineString('Prayers'),
        pinned: true,
        version: 3,
        createdAt: DateTime.utc(2026, 2),
        updatedAt: DateTime.utc(2026, 2, 2),
      );

      final restored = folder
          .toRow(ownerId: _owner, lastModifiedMillis: 42)
          .toDomain;

      expect(restored, folder);
    });

    test('has no count of its own', () {
      final folder = NoteFolder(
        id: Id.fromString(_folderId),
        title: SingleLineString('Prayers'),
        numberOfNotes: 7,
      );

      // The count is the notes manager's, so it is not a stored column.
      expect(folder.toRow(ownerId: _owner).toDomain.numberOfNotes, 0);
    });
  });
}
