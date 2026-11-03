import 'package:material_ui/material_ui.dart';
import 'package:meno_mobile/features/notes/model/_model.dart';
import 'package:meno_mobile/features/notes/widgets/folder_clipper.dart';
import 'package:meno_ui/meno_ui.dart';
import 'package:skeletonizer/skeletonizer.dart';

class FolderCard extends StatelessWidget {
  const new({
    required this.folder,
    super.key,
    this.onTap,
    this.onOptionsTap,
    this.isSelected = false,
  });

  final NoteFolder folder;
  final VoidCallback? onTap;
  final VoidCallback? onOptionsTap;
  final bool isSelected;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    final border = Border.all(
      width: 2,
      color: colors.primary,
      strokeAlign: BorderSide.strokeAlignOutside,
    );

    return InkWell(
      onTap: onTap,
      borderRadius: Corners.lg,
      child: Container(
        height: 78,
        padding: const .all(16),
        decoration: BoxDecoration(
          color: colors.surfaceTint,
          borderRadius: Corners.lg,
          border: isSelected ? border : null,
        ),
        child: Row(
          children: [
            Skeleton.leaf(
              child: SizedBox(
                height: 46,
                width: 52,
                child: ClipPath(
                  clipper: FolderClipper(r: 8, notch: 4),
                  child: ColoredBox(
                    color: colors.primary,
                    child: Center(
                      child: Icon(
                        MIcons.file_02,
                        size: 20,
                        color: colors.onPrimary,
                      ),
                    ),
                  ),
                ),
              ),
            ),
            Spaces.horizontalSmall,
            Expanded(
              child: Column(
                mainAxisAlignment: MainAxisAlignment.center,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  SizedBox(
                    height: 24,
                    child: Align(
                      alignment: Alignment.centerLeft,
                      child: MText(
                        folder.title.getOrCrash,
                        style: textTheme.captionMedium,
                      ),
                    ),
                  ),
                  Spaces.verticalMicro,
                  SizedBox(
                    height: 18,
                    child: MText(
                      '${folder.numberOfNotes} notes',
                      style: textTheme.captionRegular,
                      color: colors.onBackgroundVariant,
                    ),
                  ),
                ],
              ),
            ),
            Align(
              alignment: .topCenter,
              child: SizedBox.square(
                dimension: 16,
                child: IconButton(
                  icon: const Icon(MIcons.dots_vertical),
                  padding: .zero,
                  color: colors.onDisabledContainer,
                  iconSize: 20,
                  onPressed: onOptionsTap,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
