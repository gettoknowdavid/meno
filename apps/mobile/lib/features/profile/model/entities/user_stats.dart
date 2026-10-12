import 'package:equatable/equatable.dart';

final class UserStats with EquatableMixin {
  const UserStats({
    this.subscribers = 0,
    this.subscriptions = 0,
    this.broadcasts = 0,
  });

  final int subscribers;
  final int subscriptions;
  final int broadcasts;

  UserStats copyWith({int? subscribers, int? subscriptions, int? broadcasts}) {
    return UserStats(
      subscribers: subscribers ?? this.subscribers,
      subscriptions: subscriptions ?? this.subscriptions,
      broadcasts: broadcasts ?? this.broadcasts,
    );
  }

  @override
  bool get stringify => true;

  @override
  List<Object?> get props => [subscribers, subscriptions, broadcasts];
}

extension UserStatsX on int {
  String toSanitizedStr(String unit) {
    if (this == 1) return '$this $unit';
    return '$this ${unit}s';
  }
}
