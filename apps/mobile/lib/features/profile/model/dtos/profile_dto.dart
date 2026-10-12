import 'package:meno/_core/_core.dart';
import 'package:meno/_shared/model/entities/user.dart';
import 'package:meno/features/profile/model/_model.dart';

final class ProfileDto {
  const ProfileDto({
    required this.id,
    required this.fullName,
    this.bio,
    this.stats = const UserStatsDto(),
    this.imageUrl,
    this.role = .guest,
    this.isSubscribed = false,
  });

  factory ProfileDto.fromJson(dynamic json) {
    if (json is! Map<String, dynamic>) throw FormatError<ProfileDto>();

    final statsJson = json[_kStats];
    final stats = statsJson != null
        ? UserStatsDto.fromJson(statsJson)
        : UserStatsDto(
            subscribers: (json[_kNumOfSubscribers] as num?)?.toInt() ?? 0,
            subscriptions: (json[_kNumOfSubscriptions] as num?)?.toInt() ?? 0,
            broadcasts: (json[_kNumOfBroadcasts] as num?)?.toInt() ?? 0,
          );

    return ProfileDto(
      id: json[_kId] as String,
      fullName: json[_kFullName] as String,
      bio: json[_kBio] as String?,
      stats: stats,
      imageUrl: json[_kImageUrl] as String?,
      role: json[_kRole] != null ? UserRole.fromJson(json[_kRole]) : .guest,
      isSubscribed:
          (json[_kIsSubscribedToUser] as bool?) ??
          (json[_kSubscribed] as bool?) ??
          false,
    );
  }

  final String id;
  final String fullName;
  final String? bio;
  final UserStatsDto stats;
  final String? imageUrl;
  final UserRole role;
  final bool isSubscribed;

  static const String _kId = 'id';
  static const String _kFullName = 'fullName';
  static const String _kBio = 'bio';
  static const String _kStats = '_count';
  static const String _kImageUrl = 'imageUrl';
  static const String _kRole = 'role';
  static const String _kNumOfBroadcasts = 'numberOfBroadcasts';
  static const String _kNumOfSubscribers = 'numberOfSubscribers';
  static const String _kNumOfSubscriptions = 'numberOfSubscriptions';
  static const String _kIsSubscribedToUser = 'isSubscribedToUser';
  static const String _kSubscribed = 'subscribed';

  Map<String, dynamic> toJson() => {
    _kId: id,
    _kFullName: fullName,
    _kBio: bio,
    _kStats: stats.toJson(),
    _kImageUrl: imageUrl,
    _kRole: role.value,
    _kIsSubscribedToUser: isSubscribed,
    _kSubscribed: isSubscribed,
  };
}

extension ProfileToDto on Profile {
  ProfileDto get toDto {
    return ProfileDto(
      id: id.getOrCrash(),
      fullName: fullName.getOrCrash(),
      bio: bio?.getOrNull(),
      stats: stats.toDto,
      imageUrl: switch (image?.getOrNull()) {
        NetworkImageOrigin(:final url) => url,
        LocalImageOrigin(:final file) => file.path,
        _ => null,
      },
      isSubscribed: isSubscribed,
      role: role,
    );
  }
}

extension ProfileToDomain on ProfileDto {
  Profile get toDomain {
    return Profile(
      id: Id.fromString(id),
      fullName: SingleLineString(fullName),
      bio: bio == null ? null : MultiLineString(bio!),
      stats: stats.toDomain,
      image: imageUrl != null ? ImageInput.fromUrl(imageUrl) : null,
      isSubscribed: isSubscribed,
      role: role,
    );
  }

  ProfileDto get stripped {
    return ProfileDto(
      id: id,
      fullName: fullName,
      bio: bio,
      imageUrl: imageUrl,
      stats: stats,
    );
  }
}
