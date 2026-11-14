import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_it/flutter_it.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/profile/profile.dart';

/// The account search the add-co-host sheet runs.
///
/// It is a manager rather than a call inside the sheet because the sheet is
/// rebuilt on every keystroke: the debounce, the last results and the request
/// in flight have to survive those rebuilds, and a command is how the rest of
/// the app reports a failed search.
final class CohostSearchManager implements Disposable {
  /// Creates the manager over the profile service.
  new({required this._profiles}) {
    search = .createAsyncNoParam<List<Profile>>(
      () async {
        final term = query.value.trim();
        if (term.isEmpty) {
          results.value = [];
          return results.value;
        }
        final page = await _profiles.searchUsers(term);
        results.value = page.items.toList();
        return results.value;
      },
      initialValue: const <Profile>[],
      errorFilter: null,
      errorFilterFn: menoExceptionFilter,
      debugName: 'cohost.search',
    );
  }

  final ProfileHttpService _profiles;

  /// What the reader has typed.
  final query = ValueNotifier<String>('');

  /// The accounts the last search found.
  final results = ValueNotifier<List<Profile>>([]);

  /// Runs a search for [query].
  late final Command<void, List<Profile>> search;

  Timer? _debounce;

  /// Records what was typed and searches once the typing pauses.
  ///
  /// The delay is what keeps a fast typist from spending one request per
  /// letter; a search that is already in flight is left to finish, and its
  /// answer is superseded by the newer request's.
  void onQueryChanged(String input) {
    query.value = input;
    _debounce?.cancel();
    _debounce = Timer(const Duration(milliseconds: _debounceMs), search.run);
  }

  /// Forgets the last query and its results.
  void clear() {
    _debounce?.cancel();
    query.value = '';
    results.value = [];
  }

  @override
  Future<void> onDispose() async {
    _debounce?.cancel();
    search.dispose();
    query.dispose();
    results.dispose();
  }

  /// How long typing has to pause before the request goes out.
  static const int _debounceMs = 300;
}
