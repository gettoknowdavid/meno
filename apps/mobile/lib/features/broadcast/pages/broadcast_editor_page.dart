import 'dart:async';

import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/_shared/services/permission_prompts.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_ui/meno_ui.dart';

/// What a required field says while it is empty.
///
/// The form holds plain strings now, so the message the value objects used to
/// carry is stated once here instead.
const _emptyFieldMessage = 'This value cannot be empty.';

class BroadcastEditorPage extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final manager = di<BroadcastEditorManager>();

    // Watched, because it decides whether a pop is allowed: `PopScope.canPop`
    // has to be re-evaluated when the form becomes dirty, not only when some
    // other field re-renders this page.
    final hasUnsavedChanges = watchValue<BroadcastEditorManager, bool>(
      (manager) => manager.hasUnsavedChanges,
    );

    // Watched here rather than inside the button, because an empty list must
    // leave no gap in the header: the action is absent, not invisible.
    final drafts = watchValue<BroadcastEditorManager, List<BroadcastDraft>>(
      (manager) => manager.drafts,
    );

    // Creation is two API calls with a room in between, so the page chains
    // them: the button creates (or resumes) the row, and going live is what
    // produces the session the room is entered with. Each step reports its own
    // failure, so a chain that stops half-way leaves the form on screen.
    registerHandler<BroadcastEditorManager, void>(
      select: (manager) => manager.createBroadcast,
      handler: (context, _, _) => manager.startBroadcast.run(),
    );

    registerHandler<BroadcastEditorManager, void>(
      select: (manager) => manager.startBroadcast,
      handler: (context, _, _) {
        final broadcastId = manager.session.value?.broadcast.id;
        // An empty session means the chain stopped before the room existed, so
        // the form stays put and the commands' own handlers show why.
        if (broadcastId == null || broadcastId.isEmpty) return;

        manager.resetForm.run();
        context.replace(MRoutes.liveSessionInit);
      },
    );

    final textTheme = MTextTheme.of(context);

    return PopScope(
      // A dirty form swallows the pop — the system back gesture and the app
      // bar's Cancel both land in the callback below, which is where the
      // question is asked. A clean form pops straight through.
      canPop: !hasUnsavedChanges,
      onPopInvokedWithResult: (didPop, result) {
        if (didPop) {
          manager.resetForm.run();
          return;
        }
        unawaited(_confirmAndLeave(context, manager));
      },
      child: Material(
        child: MScaffold(
          appBar: MAppBar.secondary(
            title: 'Go Live Now',
            implyLeading: false,
            titleTextStyle: textTheme.heading3Bold,
            // Drafts sit left of the close: the same "what else can this
            // screen do" cluster, ordered from the reversible action to the one
            // that leaves. The drafts action is a word, not a glyph — a folded
            // page is not a universally understood "saved broadcasts", and a
            // label costs a few dp that the title still has room for. It is
            // absent at zero rather than empty, so the header carries no
            // permanently blank slot.
            actions: [
              if (drafts.isNotEmpty) ...[
                _DraftsAction(count: drafts.length),
                Spaces.horizontalSmall,
              ],
              _CloseAction(
                onPressed: () => unawaited(_confirmAndLeave(context, manager)),
              ),
              Spaces.horizontalLarge,
            ],
          ),
          body: const SingleChildScrollView(child: _EditorFormWidget()),
          persistentFooterButtons: const [StartBroadcastButton()],
        ),
      ),
    );
  }

  /// Leaves the editor, asking first when there is something to lose.
  ///
  /// The three answers are the only three outcomes a user can have: keep
  /// editing, keep the work as a draft, or throw it away. Dismissing the dialog
  /// is the first of them — a swipe-down must not be read as consent to
  /// discard, which is why the dialog answers `null` rather than a default.
  Future<void> _confirmAndLeave(
    BuildContext context,
    BroadcastEditorManager manager,
  ) async {
    if (manager.hasUnsavedChanges.value) {
      final choice = await _showSaveDraftDialog(context);
      if (choice == null) return;
      if (choice == _LeaveChoice.save) await manager.saveDraft();
      if (choice == _LeaveChoice.discard) await manager.discardDraft();
    }

    if (!context.mounted) return;
    manager.resetForm.run();
    context.pop();
  }

  /// Asks what to do with a half-written broadcast.
  ///
  /// `null` means "keep editing": the dialog was dismissed, or the user chose
  /// the one action that is not leaving.
  Future<_LeaveChoice?> _showSaveDraftDialog(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return showDialog<_LeaveChoice>(
      context: context,
      builder: (context) => AlertDialog(
        title: MText('Save this draft?', style: textTheme.heading3Medium),
        content: MText(
          'You have not started this broadcast yet. Save it to pick it up '
          'later, or discard what you have written.',
          style: textTheme.bodyRegular,
        ),
        actions: [
          MTextButton(
            onPressed: () => Navigator.pop(context),
            label: 'Keep editing',
            style: TextButton.styleFrom(
              foregroundColor: colors.onBackgroundVariant,
            ),
          ),
          MTextButton(
            onPressed: () => Navigator.pop(context, _LeaveChoice.discard),
            label: 'Discard',
            style: TextButton.styleFrom(foregroundColor: colors.error),
          ),
          MTextButton(
            onPressed: () => Navigator.pop(context, _LeaveChoice.save),
            label: 'Save draft',
          ),
        ],
      ),
    );
  }
}

/// What the user chose to do with an unfinished form.
enum _LeaveChoice { save, discard }

/// Closes the editor, through the same guard Cancel used to have.
///
/// An icon rather than the word "Cancel": the header has two actions now, and
/// a glyph pair reads as a toolbar instead of as two sentences.
class _CloseAction extends StatelessWidget {
  const new({required this.onPressed});

  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);

    return Tooltip(
      message: 'Close',
      child: MIconButton(
        icon: const Icon(MIcons.x_close),
        // 48 across for the tap target, 16 for the glyph: the icon is a close
        // affordance, not a headline, so it is drawn small inside the size the
        // finger needs.
        iconSize: Insets.xl,
        color: colors.onBackgroundVariant,
        onPressed: onPressed,
      ),
    );
  }
}

/// The way into the saved drafts, from the editor.
///
/// A text action rather than an icon: "Drafts" says what the tap does without
/// being learned, and a screen reader reads it as a word rather than as an
/// unknown glyph. The count is part of the label for the same reason —
/// "Drafts (3)" is one announcement, where a bare badge beside an icon is a
/// number with no noun attached to it.
class _DraftsAction extends StatelessWidget {
  const new({required this.count});

  /// How many drafts are on the device.
  final int count;

  @override
  Widget build(BuildContext context) {
    final manager = di<BroadcastEditorManager>();

    // Past nine the exact number stops being useful and starts widening the
    // header; the list itself is one tap away.
    final badge = count > 9 ? '9+' : '$count';

    return Tooltip(
      message: 'Open drafts, $count saved',
      child: Semantics(
        button: true,
        label: 'Drafts, $count saved',
        child: MTextButton(
          onPressed: () => unawaited(
            BroadcastDraftModal.show(
              context,
              onDraftSelected: manager.selectDraft.run,
              onCreateNew: manager.resetForm.run,
            ),
          ),
          label: 'Drafts ($badge)',
        ),
      ),
    );
  }
}

class _EditorFormWidget extends WatchingWidget {
  const new();

  @override
  Widget build(BuildContext context) {
    final formKey = createOnce(GlobalKey<FormState>.new);

    // Both halves of the creation: the API refusing the draft, and going live
    // failing. They are shown in place because the button that raised them is
    // on this screen.
    registerHandler<BroadcastEditorManager, CommandError<BroadcastDraft>?>(
      select: (manager) => manager.createBroadcast.errors,
      handler: (context, error, _) {
        if (error?.error == null) return;
        context.showErrorToast(errorMessage(error?.error));
      },
    );

    registerHandler<BroadcastEditorManager, CommandError<void>?>(
      select: (manager) => manager.startBroadcast.errors,
      handler: (context, error, _) {
        if (error?.error == null) return;
        context.showErrorToast(errorMessage(error?.error));
      },
    );

    final textTheme = MTextTheme.of(context);

    return Form(
      key: formKey,
      autovalidateMode: .onUserInteraction,
      child: Column(
        crossAxisAlignment: .stretch,
        children: [
          Spaces.verticalSmall,
          const _AvatarField(key: Key('broadcastForm_avatarField')),
          Spaces.verticalLarge,
          const _TitleField(key: Key('broadcastForm_titleField')),
          Spaces.verticalXLarge,
          const _DescriptionField(key: Key('broadcastForm_descriptionField')),
          Spaces.verticalXLarge,
          const CoHostSection(),
          Spaces.verticalXLarge,
          _BroadcastEditorListTile(
            leadingText: 'Remaining time today',
            subtitleText: 'Your daily broadcast time will reset in 24hrs',
            trailing: MText('0hr 30min', style: textTheme.captionRegular),
          ),
          Spaces.verticalXLarge,
          const RecordToggleSwitchField(),
          Spaces.verticalXLarge,
        ],
      ),
    );
  }
}

class _AvatarField extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);

    final manager = di<BroadcastEditorManager>();

    final isLoading = watchValue<BroadcastEditorManager, bool>(
      (m) => m.isRunning,
    );
    final imagePath = watchValue<BroadcastEditorManager, ImageInput?>(
      (m) => m.imagePath,
    );
    final imageFile = imagePath?.getFile;

    return Column(
      children: [
        MAvatar(radius: 48, file: imageFile),
        MTextButton(
          label: 'Change Artwork',
          onPressed: () {
            if (isLoading) return;
            unawaited(
              context.showModal<void>(
                MImageSourceModal(
                  onGallerySourceTap: () => unawaited(manager.pickImage()),
                  onCameraSourceTap: () =>
                      unawaited(manager.pickImage(fromGallery: false)),
                ),
              ),
            );
          },
        ),
        Center(
          child: SizedBox(
            width: 167,
            child: MText(
              'JPG or PNG accepted. Max size 10mb.',
              maxLines: 2,
              style: textTheme.microRegular,
              textAlign: TextAlign.center,
            ),
          ),
        ),
      ],
    );
  }
}

class _TitleField extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final manager = di<BroadcastEditorManager>();

    final controller = createOnce(
      () => TextEditingController(text: manager.title.value),
    );

    registerHandler<BroadcastEditorManager, String>(
      select: (manager) => manager.title,
      handler: (context, title, _) {
        if (controller.text != title) controller.text = title;
      },
    );

    final title = watchValue<BroadcastEditorManager, String>((m) => m.title);

    final isLoading = watchValue<BroadcastEditorManager, bool>(
      (m) => m.isRunning,
    );

    return MTextFormField(
      key: key,
      label: 'Broadcast title',
      hint: "Jim Halpert's live audio",
      controller: controller,
      onChanged: manager.onTitleChanged,
      textInputAction: TextInputAction.next,
      enabled: !isLoading,
      validator: (_) => title.trim().isEmpty ? _emptyFieldMessage : null,
    );
  }
}

class _DescriptionField extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final manager = di<BroadcastEditorManager>();

    final controller = createOnce(
      () => TextEditingController(text: manager.description.value),
    );

    registerHandler<BroadcastEditorManager, String?>(
      select: (manager) => manager.description,
      handler: (context, description, _) {
        if (controller.text != description) controller.text = description ?? '';
      },
    );

    final description = watchValue<BroadcastEditorManager, String?>(
      (m) => m.description,
    );

    final isLoading = watchValue<BroadcastEditorManager, bool>(
      (m) => m.isRunning,
    );

    return MTextArea(
      key: key,
      label: 'About broadcast',
      hint: 'Enter a brief description',
      maxLines: 5,
      maxLength: 244,
      controller: controller,
      onChanged: manager.onDescriptionChanged,
      enabled: !isLoading,
      validator: (_) =>
          (description?.trim().isEmpty ?? true) ? _emptyFieldMessage : null,
    );
  }
}

class CoHostSection extends StatelessWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    return const LimitedBox(
      maxHeight: 72,
      child: Row(children: [Spaces.horizontalSmall, Wrap(spacing: 8)]),
    );
  }
}

class RecordToggleSwitchField extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final manager = di<BroadcastEditorManager>();
    final record = watchValue<BroadcastEditorManager, bool>((m) => m.record);
    final isLoading = watchValue<BroadcastEditorManager, bool>(
      (m) => m.isRunning,
    );

    return _BroadcastEditorListTile(
      leadingText: 'Enable recording',
      subtitleText: 'Record your broadcast to listen back to later',
      trailing: SizedBox(
        width: 48,
        child: Switch(
          value: record,
          onChanged: isLoading ? null : manager.onToggleRecord,
        ),
      ),
    );
  }
}

class StartBroadcastButton extends WatchingWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final manager = di<BroadcastEditorManager>();

    final canSubmit = watchValue<BroadcastEditorManager, bool>(
      (m) => m.canSubmit,
    );

    final isLoading = watchValue<BroadcastEditorManager, bool>(
      (m) => m.isRunning,
    );

    final step = watchValue<BroadcastEditorManager, BroadcastCreationStep>(
      (m) => m.step,
    );

    final buttonLabel = switch (step) {
      .none => 'Start Broadcast',
      .created => 'Continue & Start',
      .started => 'Finalize Broadcast',
      .saved => 'Start Broadcast',
    };

    return Container(
      height: 77,
      padding: const .symmetric(horizontal: Insets.sm),
      child: Column(
        crossAxisAlignment: .stretch,
        mainAxisSize: .min,
        children: [
          MPrimaryButton(
            label: buttonLabel,
            loading: isLoading,
            disabled: isLoading || !canSubmit,
            // The draft is written before it is sent, so a creation the API
            // never answers is still on the device when the screen is reopened.
            onPressed: () async {
              // The room is entered speaking: a host whose microphone was
              // refused would go live silent, so it is asked for before the
              // broadcast exists at all.
              if (!await PermissionPrompts.broadcast(context)) return;
              final draft = await manager.saveDraft();
              manager.createBroadcast.run(draft);
            },
          ),
        ],
      ),
    );
  }
}

class _BroadcastEditorListTile extends StatelessWidget {
  const new({
    required this.leadingText,
    required this.subtitleText,
    this.trailing,
  });

  final String leadingText;
  final String subtitleText;
  final Widget? trailing;

  @override
  Widget build(BuildContext context) {
    return MInfoListTile(
      leadingText: leadingText,
      subtitleText: subtitleText,
      trailing: trailing,
    );
  }
}
