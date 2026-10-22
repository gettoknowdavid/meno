import 'package:flutter_it/flutter_it.dart';
import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:timeago/timeago.dart' as timeago;

class BroadcastDraftModal extends WatchingWidget {
  const new _({required this.onDraftSelected, required this.onCreateNew})
    : super(key: null);

  final ValueChanged<BroadcastDraft> onDraftSelected;
  final VoidCallback onCreateNew;

  static Future<dynamic> show(
    BuildContext context, {
    required ValueChanged<BroadcastDraft> onDraftSelected,
    required VoidCallback onCreateNew,
  }) {
    final size = MediaQuery.sizeOf(context);
    return showModalBottomSheet<dynamic>(
      context: context,
      isScrollControlled: true,
      useRootNavigator: true,
      constraints: BoxConstraints(maxHeight: size.height * 0.85),
      builder: (context) => BroadcastDraftModal._(
        onDraftSelected: onDraftSelected,
        onCreateNew: onCreateNew,
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);
    final colors = MColorScheme.of(context);

    final manager = di<BroadcastEditorManager>();

    final drafts = watchValue<BroadcastEditorManager, List<BroadcastDraft>>(
      (manager) => manager.drafts,
    );

    return MModal(
      title: 'Resume or Create New',
      builder: (context) => Column(
        mainAxisSize: .min,
        crossAxisAlignment: .stretch,
        children: [
          Flexible(
            child: ListView.separated(
              shrinkWrap: true,
              padding: const .symmetric(vertical: Insets.lg),
              itemCount: drafts.length + 1,
              separatorBuilder: (_, _) => Spaces.verticalLarge,
              itemBuilder: (context, index) {
                if (index == 0) {
                  return MText(
                    '''You have ${drafts.length} saved draft${drafts.length == 1 ? '' : 's'}''',
                    style: textTheme.captionRegular.copyWith(
                      color: colors.onBackgroundVariant,
                    ),
                  );
                }

                final draft = drafts[index - 1];
                // if (draft == null) return const SizedBox.shrink();

                return _DraftItem(
                  key: ValueKey(draft.id),
                  draft: draft,
                  onTap: () {
                    Navigator.pop(context);
                    onDraftSelected(draft);
                  },
                  onDelete: () async {
                    final confirmed = await _showDeleteConfirmation(context);
                    if (confirmed ?? false) manager.deleteDraft.run(draft.id);
                  },
                );
              },
            ),
          ),

          const Divider(height: 1),

          // Create new button
          Padding(
            padding: const .only(top: 16),
            child: MPrimaryButton(
              label: 'Create New Broadcast',
              onPressed: () {
                Navigator.pop(context);
                onCreateNew();
              },
            ),
          ),
        ],
      ),
    );
  }

  Future<bool?> _showDeleteConfirmation(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: MText('Delete Draft?', style: textTheme.heading3Medium),
        content: MText(
          'This draft will be permanently deleted. This cannot be undone.',
          style: textTheme.bodyRegular,
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: MText('Cancel', color: colors.onBackgroundVariant),
          ),
          TextButton(
            onPressed: () => Navigator.pop(context, true),
            child: MText('Delete', color: colors.error),
          ),
        ],
      ),
    );
  }
}

class _DraftItem extends StatelessWidget {
  const new({
    required this.draft,
    required this.onTap,
    required this.onDelete,
    super.key,
  });

  final BroadcastDraft draft;
  final VoidCallback onTap;
  final VoidCallback onDelete;

  @override
  Widget build(BuildContext context) {
    final textTheme = MTextTheme.of(context);
    final colors = MColorScheme.of(context);

    // A draft can hold a URL as well as a local file, and a stored path that
    // no longer exists resolves to no file at all. The thumbnail needs the
    // file itself, not merely an image input.
    final imageFile = draft.imagePath?.getFile;
    final hasImage = imageFile != null;
    final hasPartialCreation = draft.hasPartialCreation;
    final lastModified = draft.lastModified ?? DateTime.now();

    return Dismissible(
      key: ValueKey(draft.id),
      direction: .endToStart,
      confirmDismiss: (direction) async {
        onDelete();
        return false;
      },
      background: Container(
        alignment: Alignment.centerRight,
        padding: const .only(right: 20),
        decoration: BoxDecoration(
          color: colors.error,
          borderRadius: .circular(8),
        ),
        child: Column(
          mainAxisAlignment: .center,
          children: [
            Icon(Icons.delete_outline, color: colors.onError, size: 24),
            const SizedBox(height: 4),
            MText(
              'Delete',
              style: textTheme.microMedium.copyWith(color: colors.onError),
            ),
          ],
        ),
      ),
      child: InkWell(
        onTap: onTap,
        borderRadius: .circular(8),
        child: Container(
          padding: const .symmetric(horizontal: Insets.sm, vertical: Insets.lg),
          decoration: BoxDecoration(
            color: colors.surface,
            borderRadius: .circular(8),
          ),
          child: Row(
            children: [
              // Thumbnail
              Container(
                width: 79,
                height: 64,
                decoration: ShapeDecoration(
                  color: colors.disabled,
                  shape: RoundedSuperellipseBorder(
                    borderRadius: .circular(Insets.md),
                  ),
                  image: imageFile == null
                      ? null
                      : DecorationImage(
                          image: FileImage(imageFile),
                          fit: .cover,
                        ),
                ),
                child: hasImage
                    ? Icon(
                        Icons.mic_rounded,
                        color: colors.onBackgroundVariant,
                        size: 24,
                      )
                    : null,
              ),

              const SizedBox(width: 12),

              // Content
              Expanded(
                child: Column(
                  crossAxisAlignment: .start,
                  children: [
                    Row(
                      children: [
                        Expanded(
                          child: MText(
                            draft.title.isEmpty
                                ? 'Untitled Broadcast'
                                : draft.title,
                            style: textTheme.captionMedium,
                            maxLines: 1,
                            overflow: .ellipsis,
                          ),
                        ),
                        if (hasPartialCreation) ...[
                          const SizedBox(width: 8),
                          _CreationStepBadge(step: draft.creationStep),
                        ],
                      ],
                    ),
                    Spaces.verticalMicro,
                    MText(
                      (draft.description?.isEmpty ?? true)
                          ? 'No description'
                          : draft.description!,
                      style: textTheme.captionRegular,
                      color: colors.onBackgroundVariant,
                      maxLines: 1,
                      overflow: .ellipsis,
                    ),
                    Spaces.verticalMicro,
                    MText(
                      'Last edited ${timeago.format(lastModified)}',
                      style: textTheme.microRegular,
                      color: colors.onBackgroundVariant,
                      maxLines: 1,
                      overflow: .ellipsis,
                    ),
                  ],
                ),
              ),

              const SizedBox(width: 8),
              Icon(
                Icons.chevron_right_rounded,
                color: colors.onBackgroundVariant,
                size: 20,
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _CreationStepBadge extends StatelessWidget {
  const new({required this.step});

  final BroadcastCreationStep step;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    final (label, color) = switch (step) {
      BroadcastCreationStep.created => ('Created', colors.warning),
      BroadcastCreationStep.started => ('Started', colors.tertiary),
      BroadcastCreationStep.saved => ('Ready', colors.success),
      _ => ('', Colors.transparent),
    };

    if (label.isEmpty) return const SizedBox.shrink();

    return Container(
      padding: const .symmetric(horizontal: 8, vertical: 2),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.15),
        borderRadius: .circular(4),
      ),
      child: MText(label, style: textTheme.microMedium.copyWith(color: color)),
    );
  }
}
