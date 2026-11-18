import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:dio/dio.dart';
import 'package:flutter/foundation.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/http_client.dart';
import 'package:meno_mobile/features/bible/model/bible_translation_release.dart';
import 'package:path/path.dart' as p;
import 'package:path_provider/path_provider.dart';

/// One verified payload already on the device.
///
/// Found by scanning the cache rather than remembered in a row: the file *is*
/// the record, so a cache that was half-deleted can never claim a payload it
/// does not hold.
@immutable
final class CachedBiblePayload {
  /// Creates a cache entry.
  const new({required this.code, required this.version, required this.file});

  /// The translation the payload belongs to.
  final String code;

  /// The payload's version, from its file name.
  final int version;

  /// The payload file itself, gzipped seed JSON.
  final File file;
}

/// How far a download has got, as bytes received out of [total].
///
/// [total] is 0 when the server did not send a length, which is why the caller
/// computes the fraction rather than being handed one.
typedef BibleDownloadProgress = void Function(int received, int total);

/// Downloads, verifies and caches one translation payload.
///
/// # Nothing unverified reaches the store
///
/// A download is written to a `.part` file next to its target, then checked
/// against the catalogue row's exact byte length and SHA-256. Only a file that
/// matches both is renamed into place; a mismatch deletes it and fails loudly.
/// The seeder therefore only ever reads bytes the catalogue vouched for — the
/// alternative is a truncated or substituted payload writing thousands of rows
/// that look like scripture.
///
/// # The cache is what makes an offline run possible
///
/// Verified payloads are kept under the app's support directory
/// (`bible/<code>/<version>.json.gz`), outside the documents directory the user
/// can browse. A device that already downloaded a translation seeds it again
/// from that copy — after a store reset, or when the network is gone — without
/// asking the CDN for anything. Version keys are immutable, so a cached version
/// never has to be revalidated.
final class BibleDownloadService {
  /// Creates the downloader.
  ///
  /// [_supportDirectory] is injectable so a test can download into a temporary
  /// directory instead of the platform's; production uses the app's support
  /// directory.
  new({
    required this._http,
    required this._logger,
    Future<Directory> Function()? supportDirectory,
  }) : _supportDirectory = supportDirectory ?? getApplicationSupportDirectory;

  final HttpClient _http;
  final MenoLogger _logger;
  final Future<Directory> Function() _supportDirectory;

  /// The folder under the support directory that holds the cached payloads.
  static const String _cacheFolder = 'bible';

  /// What a payload file is called: `<version>.json.gz`.
  ///
  /// Versioned on purpose — the bytes are immutable per version, so a cached
  /// copy never needs revalidating and a newer version sits beside the old one
  /// until the seeder has read it.
  static final RegExp _payloadName = RegExp(r'^(\d+)\.json\.gz$');

  /// Where [code]'s payloads are cached.
  Future<Directory> cacheDirectory(String code) async {
    final support = await _supportDirectory();
    return Directory(p.join(support.path, _cacheFolder, code));
  }

  /// The file one version of [code] is cached in, whether it exists yet or
  /// not.
  Future<File> _payloadFile({
    required String code,
    required int version,
  }) async {
    final directory = await cacheDirectory(code);
    return File(p.join(directory.path, '$version.json.gz'));
  }

  /// Every cached payload, newest version per translation.
  ///
  /// One directory scan per translation and no network: this is what startup
  /// uses to seed a device whose store was reset while its downloads survived.
  /// A malformed file name is skipped rather than guessed at.
  Future<List<CachedBiblePayload>> cachedPayloads() async {
    final support = await _supportDirectory();
    final root = Directory(p.join(support.path, _cacheFolder));
    if (!root.existsSync()) return const [];

    final payloads = <CachedBiblePayload>[];

    await for (final entry in root.list()) {
      if (entry is! Directory) continue;
      final code = p.basename(entry.path);

      File? newest;
      var newestVersion = 0;
      await for (final file in entry.list()) {
        if (file is! File) continue;
        final match = _payloadName.firstMatch(p.basename(file.path));
        if (match == null) continue;

        final version = int.tryParse(match.group(1)!) ?? 0;
        if (version > newestVersion) {
          newestVersion = version;
          newest = file;
        }
      }

      if (newest == null) continue;
      payloads.add(
        CachedBiblePayload(code: code, version: newestVersion, file: newest),
      );
    }

    return payloads;
  }

  /// The cached file for [release], downloading and verifying it when needed.
  ///
  /// A cached file is checked again before it is reused: the disk is not the
  /// catalogue, and a payload that no longer hashes to what the row says must
  /// not be seeded.
  ///
  /// # Errors
  ///
  /// Whatever the transport raises — [NetworkException], [CancelledException],
  /// a status the CDN answered with — and [StorageException] when the received
  /// bytes are not the ones the catalogue describes. Nothing is left behind in
  /// either case: the partial file is deleted and the previous cache survives.
  Future<File> ensureDownloaded(
    BibleTranslationRelease release, {
    BibleDownloadProgress? onProgress,
    ValueListenable<bool>? isCancelled,
  }) async {
    final target = await _payloadFile(
      code: release.code,
      version: release.version,
    );

    if (target.existsSync() && await _verify(target, release)) {
      _logger.d(
        'Bible payload ${release.code} v${release.version} already cached',
        tag: MenoLogTag.lifecycle.label,
      );
      return target;
    }

    await target.parent.create(recursive: true);
    final partial = File('${target.path}.part');
    if (partial.existsSync()) await partial.delete();

    // Dio stops a transfer only through a token, so the command's cancel flag
    // is bridged to one for the length of this download. The listener is
    // removed in the `finally` below, whether the download finished or failed.
    final cancelToken = CancelToken();
    void stopIfCancelled() {
      if ((isCancelled?.value ?? false) && !cancelToken.isCancelled) {
        cancelToken.cancel('The download was cancelled.');
      }
    }

    isCancelled?.addListener(stopIfCancelled);

    try {
      await _http.download(
        release.payloadUrl,
        partial.path,
        authMode: AuthMode.anonymous,
        cancelToken: cancelToken,
        onReceiveProgress: onProgress,
      );

      if (!await _verify(partial, release)) {
        throw StorageException(
          message:
              'The downloaded ${release.code} payload does not match the '
              'catalogue and was discarded.',
        );
      }

      await partial.rename(target.path);
      _logger.i(
        'Downloaded ${release.code} v${release.version} '
        '(${release.bytes} bytes)',
        tag: MenoLogTag.lifecycle.label,
      );
      return target;
    } on Object {
      // A failure leaves no partial file and no wrong cache entry: the previous
      // version stays where the seeder can still find it.
      if (partial.existsSync()) await partial.delete();
      rethrow;
    } finally {
      isCancelled?.removeListener(stopIfCancelled);
    }
  }

  /// Whether [file] is exactly the payload [release] describes.
  ///
  /// Length first — one `stat`, and it catches a truncated transfer — then the
  /// digest, which catches a file of the right size holding other bytes.
  Future<bool> _verify(File file, BibleTranslationRelease release) async {
    if (file.lengthSync() != release.bytes) return false;

    // Streamed rather than read whole: a payload is a couple of megabytes, and
    // this runs on a phone that is doing something else at the same time.
    final digest = await sha256.bind(file.openRead()).first;
    return digest.toString() == release.sha256;
  }
}
