import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno/_core/_core.dart';
import 'package:meno/features/profile/model/_model.dart';
import 'package:meno/features/profile/services/_services.dart';

class UserProfileManager with MLogger implements Disposable {
  UserProfileManager({required ProfileHttpService http, required Id userId})
    : _http = http,
      _userId = userId;

  final ProfileHttpService _http;
  final Id _userId;

  late final proxy = ValueNotifier<UserProfileProxy?>(null);

  late final fetch = Command.createAsyncNoParamNoResult(() async {
    final remoteProfile = await _http.getProfile(_userId);
    proxy.value = UserProfileProxy(remoteProfile);
  }, errorFilterFn: menoExceptionFilter);

  @override
  FutureOr<dynamic> onDispose() {
    proxy.dispose();
    fetch.dispose();
  }
}
