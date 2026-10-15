import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/broadcast/model/_model.dart';
import 'package:meno_mobile/features/broadcast/services/_services.dart';

/// One broadcast's detail screen.
///
/// The proxy is created once, for the first answer, and then fed every later
/// one: a card that is already on screen holds the same proxy, so a refresh
/// repaints it instead of replacing the widget's subject underneath it.
final class BroadcastDetailsManager implements Disposable {
  new({
    required this._http,
    required this._userId,
    required this._broadcastId,
    required this._logger,
  }) {
    fetch = .createAsyncNoParamNoResult(
      () async {
        final broadcast = await _http.getBroadcast(_broadcastId);
        final existing = _proxy.value;
        if (existing == null) {
          _proxy.value = BroadcastProxy(broadcast, _userId);
        } else {
          existing.broadcast = broadcast;
        }
        _logger.d(
          'Loaded broadcast $_broadcastId (${broadcast.state.wire})',
          tag: MenoLogTag.lifecycle.label,
        );
      },
      errorFilterFn: menoExceptionFilter,
      debugName: 'broadcast.details.fetch',
    );
  }

  final BroadcastHttpService _http;
  final String _userId;
  final String _broadcastId;
  final MenoLogger _logger;

  final _proxy = ValueNotifier<BroadcastProxy?>(null);

  /// The broadcast as the screen draws it.
  ValueListenable<BroadcastProxy?> get proxy => _proxy;

  /// `GET /broadcasts/{id}`.
  late final Command<void, void> fetch;

  @override
  FutureOr<dynamic> onDispose() {
    _proxy.value?.dispose();
    _proxy.dispose();
    fetch.dispose();
  }
}
