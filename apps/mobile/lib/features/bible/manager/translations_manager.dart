import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/bible/model/_model.dart';
import 'package:meno_mobile/features/bible/services/_services.dart';

/// Manages the Bible translations this device holds.
///
/// The old implementation downloaded translations from the API. This one ships
/// all of them inside the app and the seeder writes them on first launch, so
/// "downloaded" means "already in the store" and there is nothing to fetch.
class TranslationsManager implements Disposable {
  new({required this._local, required this._seed, required this._logger}) {
    // Read once up front: this manager is created the first time a screen needs
    // it, by which point the seeder has almost certainly finished. [initialize]
    // is the explicit refresh for when it has not.
    _refreshDownloaded();

    initialize = Command.createAsyncNoParamNoResult(
      () async {
        await _seed.ensureSeeded();
        _refreshDownloaded();
      },
      errorFilterFn: menoExceptionFilter,
      debugName: 'bible.translations.initialize',
    );

    downloadTranslation = Command.createAsyncNoResultWithProgress<String>(
      (abbreviation, handle) async {
        // No remote catalogue: every translation is in the binary. The command
        // exists because the translations modal offers a download button for
        // anything in the online list — which stays empty until there is an
        // endpoint to fill it.
        _logger.w(
          'TranslationsManager: no remote source for "$abbreviation"',
          tag: MenoLogTag.lifecycle.label,
        );
      },
      errorFilterFn: menoExceptionFilter,
      debugName: 'bible.translations.download',
    );
  }

  final BibleLocalService _local;
  final BibleSeedService _seed;
  final MenoLogger _logger;

  /// Translations present in the store.
  final downloadedTranslations = ListNotifier<BibleVersionRow>(data: []);

  /// Translations offered for download.
  ///
  /// Always empty for now: the app bundles kjv, web and asv, and there is no
  /// Bible endpoint to list anything else.
  final remoteTranslations = ListNotifier<BibleVersionRow>(data: []);

  /// Code of the translation being downloaded, or null.
  final downloadingAbbreviation = ValueNotifier<String?>(null);

  /// Seeds if needed, then re-reads the stored translations.
  late final Command<void, void> initialize;

  /// Reserved for a remote catalogue; see the class comment.
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
