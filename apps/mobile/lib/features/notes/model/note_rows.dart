import 'package:meno_mobile/_core/_core.dart' as core;
import 'package:meno_mobile/features/notes/model/_model.dart';
import 'package:objectbox/objectbox.dart';

/// The `<owner>/<logicalId>` key both rows are unique by.
///
/// Encoded as well as separated, so an id that somehow contains the separator
/// cannot name a key that belongs to another account.
String noteLogicalKey(String ownerId, String logicalId) =>
    '${Uri.encodeComponent(ownerId)}/${Uri.encodeComponent(logicalId)}';

/// One row of the notes box.
///
/// A storage row, not a domain entity: ObjectBox needs a mutable identity and
/// its own key, which a `@freezed` entity cannot carry. The mapping to [Note]
/// lives beside it so the two cannot drift.
///
/// # Times are numbers
///
/// Every timestamp is stored as milliseconds since the epoch. A nullable date
/// is a column this store would have to guess a null ordering for, and the
/// domain already treats "no server timestamp" as "never confirmed", which a
/// zero says outright.
///
/// # The pending flag
///
/// [pending] is what makes the feature offline-first: a row is written locally
/// first, with the change the user made, and only then sent. A row that is
/// still pending is the change the server has not confirmed yet — it survives
/// a restart, which is what a queue in memory could not do.
@Entity()
class NoteRow {
  new({
    required this.noteId,
    required this.ownerId,
    this.title = '',
    this.content = '',
    this.pinned = false,
    this.folderId,
    this.version = 1,
    this.createdAtMillis = 0,
    this.updatedAtMillis = 0,
    this.deleted = false,
    this.pending = false,
    int? lastModifiedMillis,
  }) : uniqueKey = noteLogicalKey(ownerId, noteId),
       lastModifiedMillis =
           lastModifiedMillis ?? DateTime.now().millisecondsSinceEpoch;

  @Id()
  int id = 0;

  @Index()
  final String noteId;

  @Index()
  final String ownerId;

  /// The logical key: one row per note per account.
  @Unique(onConflict: ConflictStrategy.replace)
  String uniqueKey;

  String title;

  String content;

  bool pinned;

  @Index()
  String? folderId;

  /// The version the server last wrote, or 1 for a row it has never seen.
  int version;

  /// When the server created it; zero until it has.
  int createdAtMillis;

  /// When the server last wrote it; zero until it has.
  int updatedAtMillis;

  /// Whether the server has deleted it. The row is kept until the deletion is
  /// confirmed, so an offline delete is a state rather than a lost intent.
  bool deleted;

  /// Whether this row holds a change the server has not confirmed.
  @Index()
  bool pending;

  /// What the list is ordered by: the later of the server's stamp and the
  /// moment this device wrote the row, so an unsent note sorts by when it was
  /// typed rather than sinking to the bottom.
  int lastModifiedMillis;
}

/// One row of the folders box.
@Entity()
class NoteFolderRow {
  new({
    required this.folderId,
    required this.ownerId,
    this.title = '',
    this.pinned = false,
    this.version = 1,
    this.createdAtMillis = 0,
    this.updatedAtMillis = 0,
    this.deleted = false,
    this.pending = false,
    int? lastModifiedMillis,
  }) : uniqueKey = noteLogicalKey(ownerId, folderId),
       lastModifiedMillis =
           lastModifiedMillis ?? DateTime.now().millisecondsSinceEpoch;

  @Id()
  int id = 0;

  @Index()
  final String folderId;

  @Index()
  final String ownerId;

  /// The logical key: one row per folder per account.
  @Unique(onConflict: ConflictStrategy.replace)
  String uniqueKey;

  String title;

  bool pinned;

  int version;

  int createdAtMillis;

  int updatedAtMillis;

  bool deleted;

  bool pending;

  int lastModifiedMillis;
}

/// The domain view of a [NoteRow].
extension NoteRowX on NoteRow {
  /// The note this row holds.
  Note get toDomain => Note(
    id: core.Id.fromString(noteId),
    title: core.SingleLineString(title),
    content: core.MultiLineString(content),
    pinned: pinned,
    folderId: folderId == null ? null : core.Id.fromString(folderId!),
    version: version,
    createdAt: _time(createdAtMillis),
    updatedAt: _time(updatedAtMillis),
    deleted: deleted,
  );

  static DateTime? _time(int millis) => millis == 0
      ? null
      : DateTime.fromMillisecondsSinceEpoch(millis, isUtc: true);
}

/// The storage view of a [Note].
extension NoteX on Note {
  /// The row this note is kept in.
  ///
  /// [pending] is the caller's answer, because it is not the note's: the same
  /// note is pending right after it is typed and confirmed once the server has
  /// answered, and nothing in the entity changes in between.
  NoteRow toRow({
    required String ownerId,
    bool pending = false,
    int? lastModifiedMillis,
  }) => NoteRow(
    noteId: id.getOrCrash,
    ownerId: ownerId,
    title: title.getOrElse((_) => ''),
    content: content.getOrElse((_) => ''),
    pinned: pinned,
    folderId: folderId?.getOrNull,
    version: version,
    createdAtMillis: createdAt?.millisecondsSinceEpoch ?? 0,
    updatedAtMillis: updatedAt?.millisecondsSinceEpoch ?? 0,
    deleted: deleted,
    pending: pending,
    lastModifiedMillis: lastModifiedMillis,
  );
}

/// The domain view of a [NoteFolderRow].
extension NoteFolderRowX on NoteFolderRow {
  /// The folder this row holds.
  NoteFolder get toDomain => NoteFolder(
    id: core.Id.fromString(folderId),
    title: core.SingleLineString(title),
    pinned: pinned,
    version: version,
    createdAt: _time(createdAtMillis),
    updatedAt: _time(updatedAtMillis),
    deleted: deleted,
  );

  static DateTime? _time(int millis) => millis == 0
      ? null
      : DateTime.fromMillisecondsSinceEpoch(millis, isUtc: true);
}

/// The storage view of a [NoteFolder].
extension NoteFolderX on NoteFolder {
  /// The row this folder is kept in.
  NoteFolderRow toRow({
    required String ownerId,
    bool pending = false,
    int? lastModifiedMillis,
  }) => NoteFolderRow(
    folderId: id.getOrCrash,
    ownerId: ownerId,
    title: title.getOrElse((_) => ''),
    pinned: pinned,
    version: version,
    createdAtMillis: createdAt?.millisecondsSinceEpoch ?? 0,
    updatedAtMillis: updatedAt?.millisecondsSinceEpoch ?? 0,
    deleted: deleted,
    pending: pending,
    lastModifiedMillis: lastModifiedMillis,
  );
}
