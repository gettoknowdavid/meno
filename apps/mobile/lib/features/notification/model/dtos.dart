import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/notification/model/entities.dart';

/// One notification, as `GET /notifications` sends it.
///
/// Mirrors `NotificationItem` in `apps/api/src/modules/notifications/dto.rs`:
/// camelCase keys, an RFC 3339 `createdAt`, a nullable actor, and the rendered
/// `title` and `body` the API computed. The wire has no nested `content`
/// object, so this reads the flat item and hands the cards the fields they read
/// through [NotificationContent].
final class NotificationDto {
  /// Creates a notification DTO.
  const new({
    required this.id,
    required this.type,
    required this.title,
    required this.body,
    required this.read,
    this.imageUrl,
    this.broadcastId,
    this.actorId,
    this.actorName,
    this.actorImageUrl,
    this.createdAt,
  });

  /// Reads one item of a page.
  ///
  /// # Errors
  ///
  /// Throws a [FormatException] when the item names an unknown type, or a
  /// [MenoException] when its id is not a UUID. The page decoder reports the
  /// skip and keeps the rows it could read.
  factory fromJson(Map<String, dynamic> json) {
    return NotificationDto(
      id: Id.fromWire(json['id'] as String? ?? '').getOrCrash,
      type: NotificationType.fromJson(
        json['typeCode'] as String? ?? json['type'] as String?,
      ),
      title: (json['title'] as String? ?? '').trim(),
      body: (json['body'] as String? ?? '').trim(),
      imageUrl: json['imageUrl'] as String?,
      read: json['read'] as bool? ?? false,
      broadcastId: json['broadcastId'] as String?,
      actorId: _actorField(json, 'id'),
      actorName: _actorField(json, 'fullName'),
      actorImageUrl: _actorField(json, 'avatarUrl'),
      createdAt: DateTime.tryParse(json['createdAt'] as String? ?? ''),
    );
  }

  /// The notification's primary key.
  final String id;

  /// The type code the API computed, already mapped to a card this build draws.
  final NotificationType type;

  /// The rendered headline.
  final String title;

  /// The rendered body.
  final String body;

  /// The actor's avatar or the broadcast's cover, depending on the type.
  final String? imageUrl;

  /// Whether the recipient has read it.
  final bool read;

  /// The broadcast the notification refers to, when it refers to one.
  final String? broadcastId;

  /// Who triggered it, when the API names one.
  final String? actorId;

  /// The actor's display name.
  final String? actorName;

  /// The actor's avatar URL.
  final String? actorImageUrl;

  /// When the API created it.
  final DateTime? createdAt;

  /// Reads one field of the nullable `actor` object.
  static String? _actorField(Map<String, dynamic> json, String key) {
    final actor = json['actor'];
    if (actor is! Map<String, dynamic>) return null;

    final value = (actor[key] as String?)?.trim();
    return value == null || value.isEmpty ? null : value;
  }
}

/// Maps a wire item onto the model the cards read.
extension NotificationDtoToDomain on NotificationDto {
  /// This notification as the list holds it.
  Notification get toDomain {
    final headline = title.isNotEmpty ? title : body;

    return Notification(
      id: id,
      type: type,
      read: read,
      createdAt: createdAt,
      content: NotificationContent(
        id: broadcastId,
        title: headline,
        imageUrl: imageUrl,
        broadcastId: broadcastId,
        broadcastTitle: headline,
        broadcastImageUrl: imageUrl,
        broadcastCreator: actorName,
        subscriberId: actorId,
        subscriberName: actorName,
        subscriberImageUrl: actorImageUrl,
        cohostId: actorId,
        cohostFullName: actorName,
        cohostImageUrl: actorImageUrl,
      ),
    );
  }
}
