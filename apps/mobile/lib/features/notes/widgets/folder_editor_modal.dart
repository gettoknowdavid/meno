import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/features/notes/manager/_manager.dart';
import 'package:meno_mobile/features/notes/services/_services.dart';
import 'package:meno_ui/meno_ui.dart';

class FolderEditorModal extends WatchingWidget {
  const new({super.key, this.folderIdStr});

  final String? folderIdStr;

  static Future<bool?> show(BuildContext context, [String? folderId]) {
    return showModalBottomSheet<bool?>(
      context: context,
      builder: (context) => FolderEditorModal(folderIdStr: folderId),
      isScrollControlled: true,
      useRootNavigator: true,
    );
  }

  @override
  Widget build(BuildContext context) {
    pushScope(
      init: (getIt) {
        getIt.registerLazySingleton<FolderEditorManager>(() {
          return FolderEditorManager(
            local: di<NotesLocalService>(),
            sync: di<NotesSyncService>(),
            folders: di<FoldersManager>(),
            logger: di<MenoLogger>(),
            folderId: folderIdStr == null ? null : .fromString(folderIdStr!),
          );
        }, onCreated: (instance) => instance.initialize.run());
      },
    );

    registerHandler<FolderEditorManager, void>(
      select: (manager) => manager.submit,
      handler: (context, newValue, cancel) => context.pop(true),
    );

    return const _EditorContent();
  }
}

class _EditorContent extends WatchingWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    // Wait for initialize to complete before building the form
    final isEdit = watch(di<FolderEditorManager>().isEdit).value;
    final isLoading =
        watch(di<FolderEditorManager>().initialize.isRunning).value;

    if (isLoading) {
      return const Padding(
        padding: EdgeInsets.all(32),
        child: Center(child: MLoadingIndicator.box()),
      );
    }

    return Padding(
      padding: MediaQuery.viewInsetsOf(context),
      child: MModal(
        title: isEdit ? 'Rename Your Folder' : 'Give Your Folder a Name',
        builder: (context) => const Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            SizedBox(height: 10),
            FolderFormTitleField(),
            SizedBox(height: 56),
            _SubmitButton(),
          ],
        ),
      ),
    );
  }
}

class FolderFormTitleField extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context).inputDecorationTheme;
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    final border = UnderlineInputBorder(
      borderSide: BorderSide(color: theme.border!.borderSide.color),
    );

    final errorBorder = UnderlineInputBorder(
      borderSide: BorderSide(color: theme.errorBorder!.borderSide.color),
    );

    final disabledBorder = UnderlineInputBorder(
      borderSide: BorderSide(color: theme.disabledBorder!.borderSide.color),
    );

    final manager = di<FolderEditorManager>();
    final folder = watch(di<FolderEditorManager>().folder).value;

    final controller = createOnce(() {
      final currentTitle = folder.title.getOrElse((_) => '');
      return TextEditingController(text: currentTitle);
    });

    return TextFormField(
      autofocus: true,
      style: textTheme.heading1Regular,
      controller: controller,
      textAlign: TextAlign.center,
      enabled: !watch(di<FolderEditorManager>().submit.isRunning).value,
      onChanged: manager.onTitleChanged,
      validator: (_) => folder.title.failureOrNull?.msg,
      decoration: InputDecoration(
        hintText: 'Title',
        border: border,
        enabledBorder: border,
        focusedBorder: border,
        errorBorder: errorBorder,
        focusedErrorBorder: errorBorder,
        disabledBorder: disabledBorder,
        hintStyle: textTheme.heading1Regular.copyWith(
          color: colors.onBackgroundVariant,
        ),
      ),
    );
  }
}

class _SubmitButton extends WatchingWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    final manager = di<FolderEditorManager>();
    final isEdit = watch(di<FolderEditorManager>().isEdit).value;
    final isLoading = watch(di<FolderEditorManager>().submit.isRunning).value;
    return MPrimaryButton(
      label: isEdit ? 'Save Changes' : 'Create Folder',
      loading: isLoading,
      disabled: isLoading,
      onPressed: manager.submit.run,
    );
  }
}
