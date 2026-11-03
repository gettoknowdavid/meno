import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/features/notes/manager/_manager.dart';
import 'package:meno_mobile/features/notes/services/_services.dart';
import 'package:meno_mobile/features/notes/widgets/note_editor_widget.dart';

class NoteEditorPage extends WatchingWidget {
  const new({required this.noteIdStr, super.key});

  final String? noteIdStr;

  @override
  Widget build(BuildContext context) {
    pushScope(
      init: (getIt) {
        getIt.registerLazySingleton<NoteEditorManager>(() {
          return NoteEditorManager(
            sync: di<NotesSyncService>(),
            notes: di<NotesManager>(),
            logger: di<MenoLogger>(),
            noteId: noteIdStr != null ? .fromString(noteIdStr!) : null,
          );
        }, onCreated: (instance) => instance.initialize.run());
      },
    );

    return const _Content();
  }
}

class _Content extends WatchingStatefulWidget {
  const new();

  @override
  State<_Content> createState() => _ContentState();
}

class _ContentState extends State<_Content> with WidgetsBindingObserver {
  final NoteEditorManager _manager = di<NoteEditorManager>();

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState st) {
    // Covers: home button, task switcher, screen lock, app kill by OS. The
    // editor autosaves on a debounce, so leaving the app early would otherwise
    // drop the draft that had not been flushed yet.
    if (st == AppLifecycleState.paused || st == AppLifecycleState.detached) {
      unawaited(_manager.saveNow());
    }
  }

  Future<void> _onPopInvoked(bool didPop) async {
    if (didPop) return;
    if (_manager.status.value.isDirty) await _manager.saveNow();
    if (mounted) context.pop();
  }

  @override
  Widget build(BuildContext context) {
    return PopScope(
      canPop: false,
      onPopInvokedWithResult: (didPop, _) => _onPopInvoked(didPop),
      child: const NoteEditorWidget(),
    );
  }
}
