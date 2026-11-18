import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/bible/manager/bible_manager.dart';
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

        // Equal version: nothing to fetch and nothing to write. The catalogue
        // list is re-read anyway, so a row that is now installed leaves it.
        if (_local.seedVersion(code) >= release.version) {
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

          if (!seeded && _local.seedVersion(code) < release.version) {
            throw StorageException(
              message: 'The ${release.name} download could not be installed.',
            );
          }

          _refreshDownloaded();
          _publishRemote();
        } finally {
          downloadingAbbreviation.value = null;
        }
      },
      errorFilterFn: menoExceptionFilter,
      debugName: 'bible.translations.download',
    );

    removeTranslation = Command.createAsyncNoResult<String>(
      (code) async {
        if (isBundled(code)) {
          throw const ForbiddenException(
            message: 'That translation ships with the app and cannot be '
                'removed.',
          );
        }

        if (downloadingAbbreviation.value == code) {
          throw const ConflictException(
            message: 'Stop the download before removing this translation.',
          );
        }

        // The reader has to keep something to read: the last translation stays
        // until another one has been downloaded.
        if (downloadedTranslations.value.length <= 1) {
          throw const ConflictException(
            message: 'Keep at least one translation on this device.',
          );
        }

        // The cache goes first. If deleting it fails, the store still holds a
        // translation that reads fine and the ledger still matches it; the
        // reverse order would leave the rows gone and a payload waiting to
        // reinstall them at the next launch.
        await _downloads.removeCached(code);
        await _local.remove(code);

        _refreshDownloaded();
        _publishRemote();

        // Staying on scripture this device no longer holds would show an empty
        // chapter with nothing the reader could do about it.
        di<BibleManager>().fallBackFrom(code);
      },
      errorFilterFn: menoExceptionFilter,
      debugName: 'bible.translations.remove',
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

  /// Whether the catalogue could not be read the last time it was tried.
  ///
  /// A catalogue failure is deliberately not a command failure — an offline
  /// launch is ordinary and should not raise a toast — so this flag is the only
  /// thing that lets the translations page offer a retry instead of an empty
  /// "available" list that looks like there is nothing to download.
  final catalogueUnavailable = ValueNotifier<bool>(false);

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

  /// Deletes one translation from the device.
  ///
  /// Refused for a translation the app bundles (it would seed again on the next
  /// launch) and for the last one left (the reader would have nothing to read).
  late final Command<String, void> removeTranslation;

  /// Whether [code] ships with the app, so removing it would only bring it
  /// back on the next launch.
  bool isBundled(String code) => _bundledCodes.contains(code);

  /// Whether [code] may be removed right now.
  ///
  /// The page asks this before drawing the action, so a disabled control always
  /// has the same reason the command would refuse for.
  bool canRemove(String code) =>
      !isBundled(code) && downloadedTranslations.value.length > 1;

  /// How many verses this device holds for [code], for the removal copy.
  int verseTotal(String code) => _local.verseTotal(code);

  /// The catalogue row that would update [code], or null when it is current.
  BibleTranslationRelease? updateFor(String code) {
    final installed = _local.seedVersion(code);
    for (final release in _releases) {
      if (release.code == code && release.version > installed) return release;
    }
    return null;
  }

  /// The codes this build ships, so a translated row can say "Included".
  static final Set<String> _bundledCodes = {
    for (final translation in bundledTranslations) translation.code,
  };

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
      catalogueUnavailable.value = false;
      _publishRemote();
    } on CancelledException {
      return;
    } on Object catch (error, stackTrace) {
      catalogueUnavailable.value = true;
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
    final known = _knownRelease(code);
    if (known != null) return known;

    await _loadCatalogue();
    return _knownRelease(code);
  }

  BibleTranslationRelease? _knownRelease(String code) {
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
    catalogueUnavailable.dispose();

    initialize.dispose();
    downloadTranslation.dispose();
    removeTranslation.dispose();
  }
}
