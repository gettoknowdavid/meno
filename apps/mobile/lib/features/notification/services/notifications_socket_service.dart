import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/notification/model/_model.dart';

/// The socket's notifications, decoded once for the manager.
///
/// # The frame is a nudge, not a record
///
/// `notification` carries `{userId, title, body, timestamp}`. It has no id, no
/// type and no deep link, so it cannot become a row: a card built from it would
/// have nothing to open and no action to offer. [onNotification] therefore
/// reports the arrival and the manager re-reads the inbox, which is where the
/// stored notification — with its id, type and links — comes from.
///
/// # Installing is explicit
///
/// [install] attaches the handler and [detach] removes it; the account
/// connection owner decides when delivery is active. Both are idempotent: a
/// second install would deliver every frame twice.
final class NotificationsSocketService
    implements Disposable, ConnectionAdapter {
  /// Creates an adapter over [_client].
  new({required this._client, required this._logger});

  final SocketClient _client;
  final MenoLogger _logger;

  final _arrived = StreamController<NotificationSignal>.broadcast();

  /// The handles [install] created, held so [detach] cancels exactly them.
  List<SocketSubscription> _handles = [];

  bool _installed = false;

  /// A notification was created for this account.
  Stream<NotificationSignal> get onNotification => _arrived.stream;

  @override
  void install() {
    if (_installed) return;
    _installed = true;

    _handles = [_client.on(SocketEvent.notification, _dispatch)];
  }

  @override
  void detach() {
    if (!_installed) return;
    _installed = false;

    final handles = _handles;
    _handles = [];
    for (final handle in handles) {
      handle.cancel();
    }
  }

  @override
  Future<void> onDispose() async {
    detach();
    await _arrived.close();
  }

  /// Decodes one frame, or drops it with a log line.
  ///
  /// A malformed frame is not a reason to tear anything down: it arrives on a
  /// stream nothing awaits, and the stored notification is still readable from
  /// the inbox on the next refresh.
  void _dispatch(dynamic data) {
    if (data is! Map<String, dynamic>) {
      _logger.w(
        'Dropped a notification frame that was not an object',
        tag: MenoLogTag.network.label,
      );
      return;
    }

    final title = (data['title'] as String? ?? '').trim();
    final body = (data['body'] as String? ?? '').trim();

    if (title.isEmpty && body.isEmpty) {
      _logger.w(
        'Dropped a notification frame that carried no copy',
        tag: MenoLogTag.network.label,
      );
      return;
    }

    if (_arrived.isClosed) return;
    _arrived.add(NotificationSignal(title: title, body: body));
  }
}
