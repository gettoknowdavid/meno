import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/http_client.dart';
import 'package:meno_mobile/features/bible/model/bible_translation_release.dart';

/// Reads what scripture the API publishes for download.
///
/// One request, no account: `GET /bible/translations` is public and answers the
/// same rows for every caller, so it is sent anonymously — a signed-in device
/// has no reason to hand this endpoint its token.
///
/// The payloads themselves never travel through here: each row carries an
/// absolute CDN URL, which `BibleDownloadService` fetches.
final class BibleCatalogueService {
  /// Creates the client over the app's transport.
  const new(this._http);

  final HttpClient _http;

  /// The endpoint's path, written out like every other module's.
  static const String path = '/bible/translations';

  /// The translations on offer, in the order the API lists them.
  ///
  /// A row that cannot be verified is dropped by [HttpClient.getList] rather
  /// than failing the whole catalogue: one malformed entry should not hide the
  /// translations that are fine.
  Future<List<BibleTranslationRelease>> fetch() {
    return _http.getList<BibleTranslationRelease>(
      path,
      fromJson: BibleTranslationRelease.fromJson,
      authMode: AuthMode.anonymous,
    );
  }
}
