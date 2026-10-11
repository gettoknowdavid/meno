import 'package:flutter/material.dart';
import 'package:meno_ui/meno_ui.dart';

class AuthRedirectionText extends StatelessWidget {
  const new({
    required this.title,
    required this.buttonText,
    super.key,
    this.onPressed,
  });

  final String title;
  final String buttonText;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);
    final textTheme = MTextTheme.of(context);
    
    return Row(
      mainAxisAlignment: .center,
      children: [
        MText(title, style: textTheme.captionMedium),
        Spaces.horizontalMicro,
        InkWell(
          onTap: onPressed,
          child: MText(
            buttonText,
            style: textTheme.captionMedium,
            color: colors.primary,
          ),
        ),
      ],
    );
  }
}
