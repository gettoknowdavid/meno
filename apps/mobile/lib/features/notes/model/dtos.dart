import 'package:freezed_annotation/freezed_annotation.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/notes/model/entities.dart';

part 'dtos.freezed.dart';
part 'dtos.g.dart';

/// `NoteDto` in `apps/api/src/modules/notes/dto.rs`.
///
/// # What the wire says that the entity does not
///
/// `deleted` is derived from the row's soft-delete marker and always present,
/// while the entity carries it for completeness only — a deleted note is never
/// drawn. `version` is the concurrency token every write must quote back.
///
/// # Casing
///
/// `camelCase`, read straight off the wire: the notes module emits the same
/// shape as every other module, unlike `master`, which emitted `snake_case`
/// here and forced a client to keep two naming rules.
@freezed
abstract class NoteDto with _$NoteDto {
  /// Creates a note payload.
  const factory({
    required String id,
    required String title,
    required String content,
    @Default(false) bool pinned,
    String? folderId,
    @Default(1) int version,
    DateTime? createdAt,
    DateTime? updatedAt,
    @Default(false) bool deleted,
  }) = _NoteDto;

  factory fromJson(Map<String, dynamic> json) => _$NoteDtoFromJson(json);
}

/// A note payload as the domain holds it.
extension NoteDtoX on NoteDto {
  /// The note, with no folder resolved — the manager attaches that, because the
  /// payload cannot know which folders this client has loaded.
  Note get toDomain => Note(
    id: Id.fromString(id),
    title: SingleLineString(title),
    content: MultiLineString(content),
    pinned: pinned,
    folderId: folderId == null ? null : Id.fromString(folderId!),
    version: version,
    createdAt: createdAt,
    updatedAt: updatedAt,
    deleted: deleted,
  );
}

/// `FolderDto` in `apps/api/src/modules/notes/dto.rs`.
@freezed
abstract class FolderDto with _$FolderDto {
  /// Creates a folder payload.
  const factory({
    required String id,
    required String title,
    @Default(false) bool pinned,
    @Default(1) int version,
    DateTime? createdAt,
    DateTime? updatedAt,
    @Default(false) bool deleted,
  }) = _FolderDto;

  factory fromJson(Map<String, dynamic> json) =>
      _$FolderDtoFromJson(json);
}

/// A folder payload as the domain holds it.
extension FolderDtoX on FolderDto {
  /// The folder.
  NoteFolder get toDomain => NoteFolder(
    id: Id.fromString(id),
    title: SingleLineString(title),
    pinned: pinned,
    version: version,
    createdAt: createdAt,
    updatedAt: updatedAt,
    deleted: deleted,
  );
}
