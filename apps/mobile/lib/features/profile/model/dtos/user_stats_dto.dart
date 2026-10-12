import 'package:meno/_core/exceptions/meno_exception.dart';
import 'package:meno/features/profile/model/entities/user_stats.dart';

final class UserStatsDto {
  const UserStatsDto({
    this.subscribers = 0,
    this.subscriptions = 0,
    this.broadcasts = 0,
  });

  factory UserStatsDto.fromJson(dynamic json) {
    if (json is! Map<String, dynamic>) throw FormatError<UserStatsDto>();

    return UserStatsDto(
      subscribers: (json[_kSubscribers] as num?)?.toInt() ?? 0,
      subscriptions: (json[_kSubscriptions] as num?)?.toInt() ?? 0,
      broadcasts: (json[_kBroadcasts] as num?)?.toInt() ?? 0,
    );
  }

  final int subscribers;
  final int subscriptions;
  final int broadcasts;

  static const String _kSubscribers = 'subscribers';
  static const String _kSubscriptions = 'subscriptions';
  static const String _kBroadcasts = 'broadcasts';

  Map<String, dynamic> toJson() => {
    _kSubscribers: subscribers,
    _kSubscriptions: subscriptions,
    _kBroadcasts: broadcasts,
  };
}

extension UserStatsToDtoX on UserStats {
  UserStatsDto get toDto {
    return UserStatsDto(
      broadcasts: broadcasts,
      subscribers: subscribers,
      subscriptions: subscriptions,
    );
  }
}

extension UserStatsToDomainX on UserStatsDto {
  UserStats get toDomain {
    return UserStats(
      broadcasts: broadcasts,
      subscribers: subscribers,
      subscriptions: subscriptions,
    );
  }
}
