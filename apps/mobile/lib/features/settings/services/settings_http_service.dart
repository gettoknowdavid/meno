import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/settings/model/_model.dart';

/// The settings module's endpoints, one method each.
///
/// Mirrors `apps/api/src/modules/settings/handlers.rs`: a read, a partial update
/// and a token clear. Nothing here caches or decides — the manager owns that.
final class SettingsHttpService {
  /// Creates a service over [_client].
  const new(this._client);

  final HttpClient _client;

  /// `GET /settings` — the signed-in account's switches.
  ///
  /// The API answers with defaults for an account that has never saved any, so
  /// there is no 404 to handle.
  Future<UserSettings> getSettings() =>
      _client.get('/settings', fromJson: _settings);

  /// `PATCH /settings` — a patch of the fields that changed, answering with the
  /// fresh record.
  ///
  /// The body is [patch]'s own JSON, and a field the caller did not set is not
  /// in it at all. The API deep-merges, so an absent key is untouched rather
  /// than cleared — that is what keeps preferences this build does not know
  /// about alive (audit M13). The API refuses a patch that carries nothing, so
  /// an empty one is refused here instead of costing a round trip.
  Future<UserSettings> updateSettings(SettingsPatchDto patch) {
    if (patch.isEmpty) {
      throw ArgumentError.value(
        patch,
        'patch',
        'A settings patch cannot be empty',
      );
    }
    return _client.patch(
      '/settings',
      data: patch.toJson(),
      fromJson: _settings,
    );
  }

  /// `DELETE /settings/push-token` — unset the device token.
  ///
  /// Idempotent: clearing a token that was never set is the state the caller
  /// asked for, not an error.
  Future<void> clearPushToken() => _client.deleteUnit('/settings/push-token');

  /// Reads the record both `/settings` endpoints answer with.
  static UserSettings _settings(dynamic data) =>
      SettingsDto.fromJson(data as Map<String, dynamic>).toDomain;
}
