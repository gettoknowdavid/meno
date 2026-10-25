import 'package:flutter/foundation.dart';
import 'package:meno_mobile/_core/value_objects/id.dart';

@immutable
final class AuthContext {
  const new({required this.accountId, required this.generation});

  final Id accountId;
  final int generation;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is AuthContext &&
          runtimeType == other.runtimeType &&
          accountId == other.accountId &&
          generation == other.generation;

  @override
  int get hashCode => Object.hash(accountId, generation);

  @override
  String toString() => 'AuthContext($accountId, generation: $generation)';
}

@immutable
final class RequestIdentity {
  const new({
    required this.accountId,
    required this.generation,
    required this.epoch,
    required this.revision,
  });

  final Id accountId;
  final int generation;
  final String epoch;
  final int revision;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is RequestIdentity &&
          runtimeType == other.runtimeType &&
          accountId == other.accountId &&
          generation == other.generation &&
          epoch == other.epoch &&
          revision == other.revision;

  @override
  int get hashCode => Object.hash(accountId, generation, epoch, revision);

  @override
  String toString() =>
      'RequestIdentity(accountId: $accountId, generation: $generation, '
      'epoch: $epoch, revision: $revision)';
}
