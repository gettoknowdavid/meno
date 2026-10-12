import 'package:equatable/equatable.dart';
import 'package:meno/_core/_core.dart';
import 'package:meno/_shared/model/entities/user.dart';
import 'package:meno/features/broadcast/model/entities/participant.dart';
import 'package:meno/features/profile/model/entities/user_stats.dart';
import 'package:skeletonizer/skeletonizer.dart';

final class Profile with EquatableMixin {
  const Profile({
    required this.id,
    required this.fullName,
    this.bio,
    this.stats = const UserStats(),
    this.image,
    this.role = .guest,
    this.isSubscribed = false,
  });

  factory Profile.fromUserEntity(User user) {
    return Profile(
      id: user.id,
      fullName: user.fullName,
      bio: user.bio,
      role: user.role,
      image: user.image,
    );
  }

  factory Profile.fromParticipant(Participant participant) {
    return Profile(
      id: participant.id,
      fullName: participant.fullName,
      bio: participant.bio,
      image: participant.imageUrl != null
          ? ImageInput.fromUrl(participant.imageUrl)
          : null,
    );
  }

  static const Profile empty = Profile(
    id: Id.empty,
    fullName: SingleLineString.empty,
    bio: MultiLineString.empty,
  );

  final Id id;
  final SingleLineString fullName;
  final MultiLineString? bio;
  final UserStats stats;
  final ImageInput? image;
  final UserRole role;
  final bool isSubscribed;

  Profile copyWith({
    Id? id,
    SingleLineString? fullName,
    MultiLineString? bio,
    UserStats? stats,
    ImageInput? image,
    UserRole? role,
    bool? isSubscribed,
    int? numberOfBroadcasts,
    int? numberOfSubscribers,
    int? numberOfSubscriptions,
  }) {
    return Profile(
      id: id ?? this.id,
      fullName: fullName ?? this.fullName,
      bio: bio ?? this.bio,
      stats: stats ?? this.stats,
      image: image ?? this.image,
      role: role ?? this.role,
      isSubscribed: isSubscribed ?? this.isSubscribed,
    );
  }

  @override
  bool get stringify => true;

  @override
  List<Object?> get props => [
    id,
    fullName,
    bio,
    stats,
    image,
    role,
    isSubscribed,
  ];
}

extension ProfileX on Profile {
  Profile get stripped {
    return Profile(
      id: id,
      fullName: fullName,
      bio: bio,
      image: image,
      stats: stats,
    );
  }

  bool get isEmpty => this == .empty;

  bool get isValid => id.isValid && fullName.isValid;
}

final fakeProfile = Profile(
  id: Id.empty,
  fullName: SingleLineString(BoneMock.fullName),
  bio: MultiLineString(BoneMock.paragraph),
  stats: const UserStats(broadcasts: 14, subscribers: 300, subscriptions: 15),
);

final fakeProfiles = <Profile>[
  Profile(
    id: Id.fromString('178d88c6-1674-4135-a68b-88877b902ab2'),
    bio: MultiLineString("The Lord's favoured."),
    fullName: SingleLineString('Christie David Michael'),
    image: ImageInput.fromUrl(
      'https://res.cloudinary.com/gson007/image/upload/v1668024849/pjxxonawlab2bvn8la9o.jpg',
    ),
  ),
  Profile(
    id: Id.fromString('6fe8dbf2-e0ec-4d8c-bb13-fb9583cda788'),
    bio: MultiLineString(
      '''
David Michael: Always wanting to know more of God. Super charged with the Spirit.\nHallelujah!''',
    ),
    fullName: SingleLineString('David Michael'),
    image: ImageInput.fromUrl(
      'https://res.cloudinary.com/gson007/image/upload/v1741767889/erixhls5hpuuibb6ou9h.jpg',
    ),
  ),
  Profile(
    id: Id.fromString('3e43bf4d-7ab1-4d30-92d7-02fedf2d5ed1'),
    fullName: SingleLineString('David Michael II'),
    image: ImageInput.fromUrl(
      'https://res.cloudinary.com/gson007/image/upload/v1698913558/nephz6baho5wgkg8wrz0.jpg',
    ),
  ),
  Profile(
    id: Id.fromString('6a86d27a-f923-4e52-9b01-b8667591375a'),
    fullName: SingleLineString('STU David Michael'),
    image: ImageInput.fromUrl(
      'https://res.cloudinary.com/gson007/image/upload/v1668023831/l0kvyd27pddlspuwa2xp.jpg',
    ),
  ),
];

final fakeProfileIds = fakeProfiles.map((p) => p.id).toList();
