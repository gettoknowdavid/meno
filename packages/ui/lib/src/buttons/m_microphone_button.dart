import 'package:material_ui/material_ui.dart';
import 'package:meno_ui/src/buttons/m_icon_button.dart';
import 'package:meno_ui/src/theme/extensions/m_color_scheme.dart';
import 'package:meno_ui/src/tokens/m_icons.dart';

/// The diameter of the button, which is larger than the spacing scale.
const double _kSize = 40;

/// The size of the microphone glyph inside the button.
const double _kIconSize = 20;

/// The alpha of the tinted fill behind the glyph.
const double _kFillOpacity = 0.1;

/// The alpha of the glyph while the control is disabled.
const double _kDisabledOpacity = 0.4;

/// A microphone toggle, drawn as a tinted circular icon button.
///
/// The glyph switches with [isMicrophoneEnabled], and the button dims rather
/// than disappearing while [isDisabled] is true.
///
/// Example usage:
/// ```dart
/// MMicrophoneButton(
///   isMicrophoneEnabled: isMicOn,
///   onTap: toggleMicrophone,
/// );
/// ```
class MMicrophoneButton extends StatelessWidget {
  /// Creates a microphone button.
  ///
  /// Parameters:
  /// - [key]: An optional key to identify the widget.
  /// - [isMicrophoneEnabled]: Whether the microphone is on.
  /// - [onTap]: The callback invoked when the button is tapped.
  /// - [isDisabled]: Whether the button ignores taps.
  const new({
    super.key,
    this.isMicrophoneEnabled = true,
    this.onTap,
    this.isDisabled = false,
  });

  /// Whether the microphone is on. When false, the muted glyph is shown.
  final bool isMicrophoneEnabled;

  /// The callback invoked when the button is tapped.
  final VoidCallback? onTap;

  /// Whether the button ignores taps.
  final bool isDisabled;

  @override
  Widget build(BuildContext context) {
    final colors = MColorScheme.of(context);

    return MIconButton(
      icon: isMicrophoneEnabled
          ? const Icon(MIcons.microphone)
          : const Icon(MIcons.microphone_off),
      color: isDisabled
          ? colors.primary.withValues(alpha: _kDisabledOpacity)
          : colors.primary,
      isFilled: true,
      iconSize: _kIconSize,
      size: _kSize,
      fillColor: colors.primary.withValues(alpha: _kFillOpacity),
      onPressed: onTap,
      isDisabled: isDisabled,
    );
  }
}
