import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

class EmptyFolderPageWidget extends StatelessWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return SizedBox(
      width: 266,
      height: 224,
      child: Column(
        children: [
          Assets.images.newFile.image(
            height: 120,
            width: 160,
            package: 'meno_ui',
          ),
          MText(
            'Let’s add some notes to this folder',
            style: textTheme.bodyRegular,
            textAlign: TextAlign.center,
          ),
          Spaces.verticalXLarge,
          SizedBox(
            width: 155,
            height: 32,
            child: MSecondaryButton.icon(
              label: 'Add to this Folder',
              icon: const Icon(MIcons.plus),
              style: OutlinedButton.styleFrom(
                textStyle: textTheme.microMedium,
                foregroundColor: colors.onBackground,
                iconColor: colors.onBackground,
                shape: const RoundedRectangleBorder(borderRadius: Corners.sm),
                side: BorderSide(color: colors.outlineVariant3, width: 1.50),
              ),
              onPressed: () {},
              // onPressed: () => router.push(Routes.notesModal, extra: folder),
            ),
          ),
        ],
      ),
    );
  }
}
