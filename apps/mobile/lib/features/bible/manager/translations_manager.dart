import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/bible/model/_model.dart';
import 'package:meno_mobile/features/bible/services/_services.dart';

/// Manages the Bible translations this device holds.
///
/// Three sources feed one list of scripture, in this order:
///
/// * the translations bundled in the app, seeded by [BibleSeedService] with no
///   network at all;
/// * payloads this device downloaded earlier and still has cached, which seed
///   for free after a store reset or while offline;
/// * the API's catalogue, which offers everything else and carries the version,
///   size and SHA-256 every download is checked against.
///
/// The catalogue is optional — being offline is an ordinary launch, so its
/// failure is logged and the lists simply stay as they were. A download is not:
/// it either verifies and installs, or it fails visibly.
class TranslationsManager implements Disposable {
  new({
    required this._local,
    required this._seed,
    required this._catalogue,
    required this._downloads,
    required this._logger,
  }) {
    // Read once up front so the list has something to draw. [initialize] is
    // the explicit refresh, and it waits for the seed run — a manager created
    // before seeding finished fills in rather than staying empty.
    _refreshDownloaded();

    initialize = Command.createAsyncNoParamNoResult(
      () async {
        // Bundled scripture first: no network, and it is what the reader shows
        // before anything has been downloaded.
        await _seed.ensureSeeded();
        // Then anything already on disk. This is the offline path: a device
        // whose store was reset seeds from its cached payloads without asking
        // the CDN for a byte.
        await _seedCachedPayloads();
        _refreshDownloaded();
        // Last and optional: the catalogue needs the network, and its absence
        // must not look like a broken app.
        await _loadCatalogue();
      },
      errorFilterFn: menoExceptionFilter,
      debugName: 'bible.translations.initialize',
    );

    downloadTranslation = Command.createAsyncNoResultWithProgress<String>(
      (code, handle) async {
        final release = await _releaseFor(code);
        if (release == null) {
          throw const NotFoundException(
            message: 'That translation is not available for download.',
          );
        }

        // Equal version: nothing to fetch and nothing to write. The lists are
        // still re-read, so a caller that asks for something it already holds
        // sees it where it belongs instead of nothing happening.
        if (_local.seedVersion(code) >= release.version) {
          _refreshDownloaded();
          _publishRemote();
          return;
        }

        downloadingAbbreviation.value = code;
        try {
          final file = await _downloads.ensureDownloaded(
            release,
            isCancelled: handle.isCanceled,
            onProgress: (received, total) {
              // A server that sends no length leaves the progress where it is;
              // inventing a fraction would be a lie the ring then draws.
              if (total <= 0) return;
              handle.updateProgress(received / total);
            },
          );

          if (handle.isCanceled.value) return;

          handle.updateStatusMessage('Installing ${release.name}…');

          // Only a file that matched the catalogue gets here, and the seeder
          // reads it from disk — the bytes never travel through this manager.
          final seeded = await _seed.seedFromCache(
            code: code,
            version: release.version,
            file: file,
          );

          _refreshDownloaded();
          _publishRemote();

          if (!seeded && _local.seedVersion(code) < release.version) {
            throw StorageException(
              message: 'The ${release.name} download could not be installed.',
            );
          }
        } finally {
          downloadingAbbreviation.value = null;
        }
      },
      errorFilterFn: menoExceptionFilter,
      debugName: 'bible.translations.download',
    );
  }

  final BibleLocalService _local;
  final BibleSeedService _seed;
  final BibleCatalogueService _catalogue;
  final BibleDownloadService _downloads;
  final MenoLogger _logger;

  /// Translations present in the store.
  final downloadedTranslations = ListNotifier<BibleVersionRow>(data: []);

  /// Catalogue releases this device does not hold at that version.
  ///
  /// A release whose version the device already seeded is left out — there is
  /// nothing to offer — and one with a higher version stays, because an update
  /// is a download like any other.
  final remoteTranslations = ListNotifier<BibleTranslationRelease>(data: []);

  /// Code of the translation being downloaded, or null.
  final downloadingAbbreviation = ValueNotifier<String?>(null);

  /// The catalogue as it was last read, installed releases included.
  ///
  /// Kept apart from [remoteTranslations] because a download needs the row of a
  /// translation that is no longer in that list: it is installed the moment the
  /// seeder writes it, and the row is what that download was verified against.
  List<BibleTranslationRelease> _releases = const [];

  /// Seeds if needed, seeds cached downloads, then reads the catalogue.
  late final Command<void, void> initialize;

  /// Downloads, verifies and installs one translation from the catalogue.
  late final Command<String, void> downloadTranslation;

  // =========================================================================
  // PRIVATE HELPERS
  // =========================================================================

  void _refreshDownloaded() {
    final local = _local.versions();

    downloadedTranslations.startTransAction();
    downloadedTranslations.clear();
    downloadedTranslations.addAll(local);
    downloadedTranslations.endTransAction();
  }

  /// Seeds every payload this device already has cached.
  ///
  /// No network and no catalogue: the file names carry the version, the payload
  /// carries its own text, and [BibleSeedService.seedFromCache] checks the
  /// ledger. Failures are the seeder's to log — one unreadable file must not
  /// keep the others out of the store.
  ///
  /// A cache that cannot even be listed — a platform without an app support
  /// directory, a widget test — means "nothing cached", not a failed startup:
  /// bundled scripture is still seeded by the call before this one.
  Future<void> _seedCachedPayloads() async {
    final List<CachedBiblePayload> cached;
    try {
      cached = await _downloads.cachedPayloads();
    } on Object catch (error, stackTrace) {
      _logger.w(
        'Bible payload cache unavailable',
        tag: MenoLogTag.lifecycle.label,
        error: error,
        stackTrace: stackTrace,
      );
      return;
    }

    for (final payload in cached) {
      await _seed.seedFromCache(
        code: payload.code,
        version: payload.version,
        file: payload.file,
      );
    }
  }

  /// Reads the catalogue, keeping the previous one when it cannot be read.
  ///
  /// Deliberately not a command: nothing in the UI waits on the catalogue, and
  /// a failed read at startup should not raise a toast over a screen that works
  /// without it. A cancelled read is a navigation, not a failure.
  Future<void> _loadCatalogue() async {
    try {
      _releases = await _catalogue.fetch();
      _publishRemote();
    } on CancelledException {
      return;
    } on Object catch (error, stackTrace) {
      _logger.w(
        'Bible catalogue unavailable',
        tag: MenoLogTag.lifecycle.label,
        error: error,
        stackTrace: stackTrace,
      );
    }
  }

  /// Recomputes what the catalogue offers beyond what this device has.
  void _publishRemote() {
    final remote = <BibleTranslationRelease>[
      for (final release in _releases)
        if (_local.seedVersion(release.code) < release.version) release,
    ];

    remoteTranslations.startTransAction();
    remoteTranslations.clear();
    remoteTranslations.addAll(remote);
    remoteTranslations.endTransAction();
  }

  /// The catalogue row for [code], reading the catalogue once if needed.
  ///
  /// A download can be asked for before `initialize` has finished, or in a
  /// session where startup ran offline and the network has since come back, so
  /// an unknown code is a reason to read the catalogue again rather than a
  /// reason to fail.
  Future<BibleTranslationRelease?> _releaseFor(String code) async {
    final known = _release(code);
    if (known != null) return known;

    await _loadCatalogue();
    return _release(code);
  }

  BibleTranslationRelease? _release(String code) {
    for (final release in _releases) {
      if (release.code == code) return release;
    }
    return null;
  }

  // =========================================================================
  // DISPOSE
  // =========================================================================

  @override
  FutureOr<dynamic> onDispose() {
    _logger.i(
      'TranslationsManager: Disposing…',
      tag: MenoLogTag.lifecycle.label,
    );

    downloadedTranslations.dispose();
    remoteTranslations.dispose();
    downloadingAbbreviation.dispose();

    initialize.dispose();
    downloadTranslation.dispose();
  }
}
