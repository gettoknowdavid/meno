import 'package:meno_mobile/_core/_core.dart' as core;
import 'package:meno_mobile/_core/logging/meno_logger.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';
import 'package:objectbox/objectbox.dart';

/// One row of the drafts box.
///
/// A storage row, not a domain entity: ObjectBox needs a mutable identity and
/// its own key, which a `@freezed` entity cannot carry. The mapping to
/// [BroadcastDraft] lives beside it so the two cannot drift.
@Entity()
class BroadcastDraftRow {
  new({
    required this.draftId,
    required this.ownerId,
    this.title = '',
    this.description = '',
    this.record = false,
    this.cohostIds = '',
    this.imagePath,
    this.imageId,
    this.createdBroadcastId,
    this.creationStepValue = 0,
    DateTime? lastModified,
  }) : lastModified = lastModified ?? DateTime.now();

  @Id()
  int id = 0;

  @Index()
  final String draftId;

  @Index()
  final String ownerId;

  String title;

  String? description;

  bool record;

  /// The cohost ids, comma-separated: ObjectBox stores a list as a relation,
  /// which is more machinery than a handful of ids is worth.
  String cohostIds;

  /// The locally picked image, when one was chosen and not yet uploaded.
  String? imagePath;

  /// The uploaded image's storage key, once the editor has sent it.
  String? imageId;

  String? createdBroadcastId;

  @Property(type: PropertyType.int)
  int creationStepValue;

  @Property(type: PropertyType.date)
  DateTime lastModified;
}

/// One row of the favourites box.
@Entity()
class FavouriteBroadcastRow {
  new({
    required this.broadcastId,
    required this.ownerId,
    this.title = '',
    this.creatorName = '',
    this.imageUrl,
    DateTime? savedAt,
  }) : savedAt = savedAt ?? DateTime.now();

  @Id()
  int id = 0;

  @Index()
  final String broadcastId;

  @Index()
  final String ownerId;

  final String title;

  final String creatorName;

  final String? imageUrl;

  @Property(type: PropertyType.date)
  final DateTime savedAt;
}

/// The domain view of a [BroadcastDraftRow].
extension BroadcastDraftRowX on BroadcastDraftRow {
  BroadcastDraft toDomain({MenoLogger? logger}) => BroadcastDraft(
    id: core.Id.fromString(draftId),
    ownerId: core.Id.fromString(ownerId),
    title: title,
    description: description,
    record: record,
    cohosts: _cohostIds(logger),
    imagePath: core.ImageInput.fromStored(imagePath),
    imageId: imageId,
    createdBroadcastId: createdBroadcastId != null
        ? core.Id.fromString(createdBroadcastId!)
        : null,
    creationStep: _step(creationStepValue),
    lastModified: lastModified,
  );

  List<core.Id> _cohostIds(MenoLogger? logger) {
    final ids = <core.Id>[];
    for (final raw in cohostIds.split(',')) {
      if (raw.isEmpty) continue;

      final id = core.Id.fromString(raw);
      if (id.isValid) {
        ids.add(id);
        continue;
      }

      logger?.w(
        'Dropped a stored cohost id that is not a UUID',
        tag: MenoLogTag.di.label,
        error: id.failureOrNull,
      );
    }
    return ids;
  }

  static BroadcastCreationStep _step(int value) {
    const steps = BroadcastCreationStep.values;
    return value >= 0 && value < steps.length
        ? steps[value]
        : BroadcastCreationStep.none;
  }
}

/// The storage view of a [BroadcastDraft].
extension BroadcastDraftX on BroadcastDraft {
  BroadcastDraftRow get toRow => BroadcastDraftRow(
    draftId: id.getOrCrash,
    ownerId: ownerId.getOrCrash,
    title: title,
    description: description,
    record: record,
    cohostIds: [for (final id in cohosts) id.getOrCrash].join(','),
    imagePath: imagePath?.toUrlOrFilePathOrNull,
    imageId: imageId,
    createdBroadcastId: createdBroadcastId?.getOrNull,
    creationStepValue: creationStep.index,
    lastModified: lastModified,
  );
}

/// The domain view of a [FavouriteBroadcastRow].
extension FavouriteBroadcastRowX on FavouriteBroadcastRow {
  FavouriteBroadcast get toDomain => FavouriteBroadcast(
    broadcastId: core.Id.fromString(broadcastId),
    ownerId: core.Id.fromString(ownerId),
    title: core.SingleLineString(title),
    creatorName: core.SingleLineString(creatorName),
    image: core.ImageInput.fromUrl(imageUrl),
    savedAt: savedAt,
  );
}

/// The storage view of a [FavouriteBroadcast].
extension FavouriteBroadcastX on FavouriteBroadcast {
  FavouriteBroadcastRow get toRow => FavouriteBroadcastRow(
    broadcastId: broadcastId.getOrCrash,
    ownerId: ownerId.getOrCrash,
    title: title.getOrCrash,
    creatorName: creatorName.orEmpty,
    imageUrl: image?.toUrlOrFilePathOrNull,
    savedAt: savedAt,
  );
}
