import 'package:freezed_annotation/freezed_annotation.dart';
import 'package:meno_mobile/_core/_core.dart';

part 'entities.freezed.dart';

/// A note, as this app holds it.
///
/// # What is not here
///
/// There is no creator and no cached owner: the API has kept notes per account
/// since the notes module was rewritten, and every read is already scoped to
/// the caller. A field that could only ever hold the signed-in account would be
/// a second, staler copy of the session.
///
/// # Version and folder
///
/// [version] is the optimistic-concurrency token the API guards every write
/// with — a note edited on two devices is refused, not silently overwritten.
/// [folderId] is what the wire carries; [folder] is the same relation resolved
/// to the folder object the screens draw, which the manager attaches so a card
/// can render its tag without a second lookup per row.
@freezed
abstract class Note with _$Note {
  /// Creates a note.
  const factory({
    required Id id,
    required SingleLineString title,
    required MultiLineString content,

    /// Whether the note is pinned to the top of its list.
    @Default(false) bool pinned,

    /// The folder it is filed in, if any.
    Id? folderId,

    /// The folder object for [folderId], resolved by the manager that owns the
    /// list. Null when the note is unfiled, or when the folder is not loaded.
    NoteFolder? folder,

    /// The version the server last wrote.
    @Default(1) int version,

    /// When it was created, as the server said.
    DateTime? createdAt,

    /// When it was last written, as the server said.
    DateTime? updatedAt,

    /// Whether it has been deleted. The API keeps the row, so a pull can tell
    /// a deleted note from one that was never there.
    @Default(false) bool deleted,
  }) = _Note;

  /// A blank note under [id] — what the editor opens for "new note".
  ///
  /// The id is chosen here rather than by the server: a note written offline
  /// keeps the id it was created with, and the API takes the client's.
  factory fromNewId(Id id) => Note(
    id: id,
    title: SingleLineString.empty,
    content: MultiLineString.empty,
  );

  const new _();

  /// A note with nothing in it, for a notifier that has to start somewhere.
  static const empty = Note(
    id: Id.empty,
    title: SingleLineString.empty,
    content: MultiLineString.empty,
  );

  /// Whether this note can be sent to the API at all.
  ///
  /// Only the id: a title may be empty while it is being typed, and the API
  /// accepts an empty title and body.
  bool get isValid => id.isValid;

  /// Whether this note has been written to the server yet.
  ///
  /// The editor creates a note on its first save, so a note with no server
  /// timestamp has never left the device.
  bool get isNew => createdAt == null;
}

/// Three notes in different shapes, for the skeletons a loading list draws.
final List<Note> fakeNotes = List.generate(
  3,
  (index) => Note(
    id: Id.fromString('00000000-0000-4000-8000-00000000000$index'),
    title: SingleLineString('A note worth keeping'),
    content: MultiLineString('What the Lord showed me this morning.'),
    folder: NoteFolder(
      id: Id.empty,
      title: SingleLineString('A folder'),
    ),
    createdAt: DateTime(2026, 1, 10, 9, 30),
    updatedAt: DateTime(2026, 1, 10, 9, 45),
  ),
);

/// A folder of notes.
///
/// [numberOfNotes] is not on the wire — the API answers a page of notes, not a
/// count — so it is filled in by the manager from the notes it already holds.
/// A card that shows it therefore shows a real number from the list the app is
/// drawing, not a second request per folder.
@freezed
abstract class NoteFolder with _$NoteFolder {
  /// Creates a folder.
  const factory({
    required Id id,
    required SingleLineString title,

    /// Whether the folder is pinned to the top of the list.
    @Default(false) bool pinned,

    /// The version the server last wrote.
    @Default(1) int version,

    /// When it was created, as the server said.
    DateTime? createdAt,

    /// When it was last written, as the server said.
    DateTime? updatedAt,

    /// Whether it has been deleted.
    @Default(false) bool deleted,

    /// How many notes are filed in it, as the manager counted them.
    @Default(0) int numberOfNotes,
  }) = _NoteFolder;

  /// A blank folder under [id] — what the editor opens for "new folder".
  factory fromNewId(Id id) =>
      NoteFolder(id: id, title: SingleLineString.empty);

  const new _();

  /// A folder with nothing in it.
  static const empty = NoteFolder(
    id: Id.empty,
    title: SingleLineString.empty,
  );

  /// Whether this folder can be sent to the API at all.
  bool get isValid => id.isValid;

  /// Whether this folder has been written to the server yet.
  bool get isNew => createdAt == null;
}

/// Three folders, for the skeletons a loading list draws.
final List<NoteFolder> fakeFolders = List.generate(
  3,
  (index) => NoteFolder(
    id: Id.fromString('00000000-0000-4000-8000-00000000001$index'),
    title: SingleLineString('A folder'),
    numberOfNotes: 3,
    createdAt: DateTime(2026, 1, 10),
  ),
);
