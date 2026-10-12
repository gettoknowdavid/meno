import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno/_core/_core.dart';
import 'package:meno/features/profile/profile.dart';

class UserProfileProxy extends ChangeNotifier implements Disposable {
  UserProfileProxy(this._profile);

  Profile _profile;

  Profile get profile => _profile;

  bool? _isSubscribedOverride;

  Id get id => _profile.id;

  String get fullName => _profile.fullName.getOrCrash();
  MultiLineString? get bio => _profile.bio;
  String? get imageUrl => _profile.image?.getUrl();
  UserStats get stats => _profile.stats;
  bool get isSubscribed => _isSubscribedOverride ?? _profile.isSubscribed;

  set profile(Profile value) {
    _isSubscribedOverride = null;
    _profile = value;
    notifyListeners();
  }

  late final subscribe = Command.createUndoableNoParamNoResult<bool>(
    (stack) async {
      stack.push(isSubscribed);
      _isSubscribedOverride = true;
      _updateStats(delta: 1);
      notifyListeners();
      await di<ProfileHttpService>().subscribe(id);
    },
    undo: (stack, reason) {
      _isSubscribedOverride = stack.pop();
      _updateStats(delta: -1);
      notifyListeners();
    },
  );

  late final unsubscribe = Command.createUndoableNoParamNoResult<bool>(
    (stack) async {
      stack.push(isSubscribed);
      _isSubscribedOverride = false;
      _updateStats(delta: -1);
      notifyListeners();
      await di<ProfileHttpService>().unsubscribe(id);
    },
    undo: (stack, reason) {
      _isSubscribedOverride = stack.pop();
      _updateStats(delta: 1);
      notifyListeners();
    },
  );

  late final isRunning = subscribe.isRunning.combineLatest(
    unsubscribe.isRunning,
    (subscribing, unsubscribing) => subscribing || unsubscribing,
  );

  void _updateStats({required int delta}) {
    final updated = _profile.copyWith(
      stats: _profile.stats.copyWith(
        subscribers: _profile.stats.subscribers + delta,
      ),
    );
    _profile = updated;

    // Update the subscriptions stats in the profile of the currently
    // authenticated user
    di<MyProfileManager>().updateStats(delta: delta);
  }

  @override
  FutureOr<dynamic> onDispose() {
    subscribe.dispose();
    unsubscribe.dispose();
    super.dispose();
  }
}
