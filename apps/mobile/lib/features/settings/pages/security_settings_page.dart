import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/meno_ui.dart';

class SecuritySettingsPage extends StatelessWidget {
  const new({super.key});

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);

    return MScaffold(
      appBar: MAppBar.secondary(title: 'Security', centerTitle: true),
      body: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            const SizedBox(height: 16),
            MText(
              'Change/update password',
              style: textTheme.captionMedium,
              color: colors.inActive,
            ),
            const SizedBox(height: 32),
            const MTextFormField(label: 'Old Password'),
            const SizedBox(height: 24),
            const MTextFormField(label: 'New Password'),
            const SizedBox(height: 24),
            const MTextFormField(label: 'Confirm New Password'),
            const SizedBox(height: 32),
            MPrimaryButton(label: 'Save changes', onPressed: () {}),
          ],
        ),
      ),
    );
  }
}
