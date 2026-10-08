import 'package:flutter/material.dart';
import 'package:meno_ui/src/m_internal.dart';
import 'package:meno_ui/src/theme/m_color.dart';

/// A custom theme extension for managing colors within the app.
///
/// The [MColorScheme] class extends [ThemeExtension] to define a collection of
/// colors used throughout the application. This helps in maintaining a
/// consistent color scheme and allows for easy customization of colors
/// based on the application's color scheme.
class MColorScheme extends ThemeExtension<MColorScheme> {
  /// Creates an [MColorScheme] instance with the provided color scheme.
  ///
  /// All colors in the scheme are optional and can be customized individually.
  const new({
    required this.brightness,
    required this.primary,
    required this.onPrimary,
    required this.primaryContainer,
    required this.onPrimaryContainer,
    required this.secondary,
    required this.onSecondary,
    required this.secondaryContainer,
    required this.onSecondaryContainer,
    required this.tertiary,
    required this.onTertiary,
    required this.tertiaryContainer,
    required this.onTertiaryContainer,
    required this.error,
    required this.onError,
    required this.errorContainer,
    required this.onErrorContainer,
    required this.informational,
    required this.onInformational,
    required this.informationalContainer,
    required this.onInformationalContainer,
    required this.warning,
    required this.onWarning,
    required this.warningContainer,
    required this.onWarningContainer,
    required this.success,
    required this.onSuccess,
    required this.successContainer,
    required this.onSuccessContainer,
    required this.notification,
    required this.onNotification,
    required this.inActive,
    required this.onInActive,
    required this.inActiveContainer,
    required this.onInActiveContainer,
    required this.disabled,
    required this.onDisabled,
    required this.disabledContainer,
    required this.onDisabledContainer,
    required this.background,
    required this.onBackground,
    required this.onBackgroundProminent,
    required this.onBackgroundVariant,
    required this.surface,
    required this.onSurface,
    required this.surfaceTint,
    required this.surfaceShade,
    required this.onSurfaceShade,
    required this.inverseSurface,
    required this.onInverseSurface,
    required this.inversePrimary,
    required this.onInversePrimary,
    required this.scrim,
    required this.shadow,
    required this.outline,
    required this.outlineVariant1,
    required this.outlineVariant2,
    required this.outlineVariant3,
  });

  /// Provides the default [MColorScheme] for the app based on the given
  /// [brightness].
  ///
  /// This factory method initializes an [MColorScheme] instance using
  /// a predefined set of colors.
  factory $default(Brightness brightness) {
    final isLight = brightness == Brightness.light;
    return MColorScheme(
      brightness: brightness,
      primary: MInternal.resolve(isLight, MColor.primary300, MColor.primary75),
      onPrimary: MInternal.resolve(isLight, MColor.white, MColor.primary700),
      primaryContainer: MInternal.resolve(
        isLight,
        MColor.primary50,
        MColor.primary600,
      ),
      onPrimaryContainer: MInternal.resolve(
        isLight,
        MColor.primary700,
        MColor.primary50,
      ),
      secondary: MInternal.resolve(
        isLight,
        MColor.secondary300,
        MColor.secondary75,
      ),
      onSecondary: MInternal.resolve(
        isLight,
        MColor.white,
        MColor.secondary600,
      ),
      secondaryContainer: MInternal.resolve(
        isLight,
        MColor.secondary50,
        MColor.secondary75,
      ),
      onSecondaryContainer: MColor.secondary600,
      tertiary: MInternal.resolve(
        isLight,
        MColor.decorativeYellow75,
        MColor.decorativeYellow200,
      ),
      onTertiary: MColor.black,
      tertiaryContainer: MColor.decorativeYellow50,
      onTertiaryContainer: MColor.black,
      error: MInternal.resolve(isLight, MColor.error300, MColor.error75),
      onError: MInternal.resolve(isLight, MColor.white, MColor.error600),
      errorContainer: MInternal.resolve(
        isLight,
        MColor.error75,
        MColor.error500,
      ),
      onErrorContainer: MInternal.resolve(
        isLight,
        MColor.error600,
        MColor.error75,
      ),
      informational: MInternal.resolve(isLight, MColor.blue300, MColor.blue75),
      onInformational: MInternal.resolve(isLight, MColor.white, MColor.blue600),
      informationalContainer: MInternal.resolve(
        isLight,
        MColor.blue50,
        MColor.secondary500,
      ),
      onInformationalContainer: MInternal.resolve(
        isLight,
        MColor.blue300,
        MColor.secondary50,
      ),
      warning: MInternal.resolve(isLight, MColor.yellow300, MColor.yellow75),
      onWarning: MInternal.resolve(isLight, MColor.white, MColor.yellow600),
      warningContainer: MInternal.resolve(
        isLight,
        MColor.yellow75,
        MColor.yellow500,
      ),
      onWarningContainer: MInternal.resolve(
        isLight,
        MColor.yellow600,
        MColor.yellow50,
      ),
      success: MInternal.resolve(isLight, MColor.success300, MColor.success75),
      onSuccess: MInternal.resolve(isLight, MColor.white, MColor.success600),
      successContainer: MInternal.resolve(
        isLight,
        MColor.success75,
        MColor.success500,
      ),
      onSuccessContainer: MInternal.resolve(
        isLight,
        MColor.success600,
        MColor.success50,
      ),
      notification: MInternal.resolve(isLight, MColor.error300, MColor.error75),
      onNotification: MInternal.resolve(
        isLight,
        MColor.white,
        MColor.success600,
      ),
      inActive: MInternal.resolve(isLight, MColor.grey70, MColor.grey50),
      onInActive: MInternal.resolve(isLight, MColor.grey900, MColor.grey200),
      inActiveContainer: MInternal.resolve(
        isLight,
        MColor.grey30,
        MColor.grey300,
      ),
      onInActiveContainer: MInternal.resolve(
        isLight,
        MColor.grey500,
        MColor.grey10,
      ),
      disabled: MInternal.resolve(isLight, MColor.grey50, MColor.primary600),
      onDisabled: MInternal.resolve(isLight, MColor.grey900, MColor.grey10),
      disabledContainer: MInternal.resolve(
        isLight,
        MColor.grey30,
        MColor.primary600,
      ),
      onDisabledContainer: MInternal.resolve(
        isLight,
        MColor.grey500,
        MColor.grey200,
      ),
      background: MInternal.resolve(isLight, MColor.white, MColor.primary700),
      onBackground: MInternal.resolve(isLight, MColor.black, MColor.white),
      surface: MInternal.resolve(isLight, MColor.white, MColor.primary700),
      onSurface: MInternal.resolve(isLight, MColor.black, MColor.white),
      onBackgroundProminent: MInternal.resolve(
        isLight,
        MColor.primary600,
        MColor.primary50,
      ),
      onBackgroundVariant: MColor.grey80,
      surfaceTint: MInternal.resolve(isLight, MColor.tint, MColor.primaryAlt),
      surfaceShade: MInternal.resolve(isLight, MColor.tint, MColor.primaryAlt),
      onSurfaceShade: MInternal.resolve(isLight, MColor.grey200, MColor.grey30),
      inverseSurface: MInternal.resolve(
        isLight,
        MColor.primary600,
        MColor.primary75,
      ),
      onInverseSurface: MInternal.resolve(
        isLight,
        MColor.white,
        MColor.primary600,
      ),
      inversePrimary: MInternal.resolve(
        isLight,
        MColor.primary75,
        MColor.primary200,
      ),
      onInversePrimary: MInternal.resolve(
        isLight,
        MColor.primary600,
        MColor.white,
      ),
      scrim: MColor.n0,
      shadow: MColor.shadow,
      outline: MInternal.resolve(isLight, MColor.primary300, MColor.primary75),
      outlineVariant1: MInternal.resolve(
        isLight,
        MColor.grey30,
        MColor.grey400,
      ),
      outlineVariant2: MInternal.resolve(
        isLight,
        MColor.grey30,
        MColor.grey400,
      ),
      outlineVariant3: MInternal.resolve(
        isLight,
        MColor.grey50,
        MColor.grey200,
      ),
    );
  }

  /// The [brightness] color
  final Brightness? brightness;

  /// The [primary] color
  final Color primary;

  /// The [onPrimary] color
  final Color onPrimary;

  /// The [primaryContainer] color
  final Color primaryContainer;

  /// The [onPrimaryContainer] color
  final Color onPrimaryContainer;

  /// The [secondary] color
  final Color secondary;

  /// The [onSecondary] color
  final Color onSecondary;

  /// The [secondaryContainer] color
  final Color secondaryContainer;

  /// The [onSecondaryContainer] color
  final Color onSecondaryContainer;

  /// The [tertiary] color
  final Color tertiary;

  /// The [onTertiary] color
  final Color onTertiary;

  /// The [tertiaryContainer] color
  final Color tertiaryContainer;

  /// The [onTertiaryContainer] color
  final Color onTertiaryContainer;

  /// The [error] color
  final Color error;

  /// The [onError] color
  final Color onError;

  /// The [errorContainer] color
  final Color errorContainer;

  /// The [onErrorContainer] color
  final Color onErrorContainer;

  /// The [informational] color
  final Color informational;

  /// The [onInformational] color
  final Color onInformational;

  /// The [informationalContainer] color
  final Color informationalContainer;

  /// The [onInformationalContainer] color
  final Color onInformationalContainer;

  /// The [warning] color
  final Color warning;

  /// The [onWarning] color
  final Color onWarning;

  /// The [warningContainer] color
  final Color warningContainer;

  /// The [onWarningContainer] color
  final Color onWarningContainer;

  /// The [success] color
  final Color success;

  /// The [onSuccess] color
  final Color onSuccess;

  /// The [successContainer] color
  final Color successContainer;

  /// The [onSuccessContainer] color
  final Color onSuccessContainer;

  /// The [notification] color
  final Color notification;

  /// The [onNotification] color
  final Color onNotification;

  /// The [inActive] color
  final Color inActive;

  /// The [onInActive] color
  final Color onInActive;

  /// The [inActiveContainer] color
  final Color inActiveContainer;

  /// The [onInActiveContainer] color
  final Color onInActiveContainer;

  /// The [disabled] color
  final Color disabled;

  /// The [onDisabled] color
  final Color onDisabled;

  /// The [disabledContainer] color
  final Color disabledContainer;

  /// The [onDisabledContainer] color
  final Color onDisabledContainer;

  /// The [background] color
  final Color background;

  /// The [onBackground] color
  final Color onBackground;

  /// The [onBackgroundProminent] color
  final Color onBackgroundProminent;

  /// The [onBackgroundVariant] color
  final Color onBackgroundVariant;

  /// The [surface] color
  final Color surface;

  /// The [onSurface] color
  final Color onSurface;

  /// The [surfaceTint] color
  final Color surfaceTint;

  /// The [surfaceShade] color
  final Color surfaceShade;

  /// The [onSurfaceShade] color
  final Color onSurfaceShade;

  /// The [inverseSurface] color
  final Color inverseSurface;

  /// The [onInverseSurface] color
  final Color onInverseSurface;

  /// The [inversePrimary] color
  final Color inversePrimary;

  /// The [onInversePrimary] color
  final Color onInversePrimary;

  /// The [scrim] color
  final Color scrim;

  /// The [shadow] color
  final Color shadow;

  /// The [outline] color
  final Color outline;

  /// The [outlineVariant1] color
  final Color outlineVariant1;

  /// The [outlineVariant2] color
  final Color outlineVariant2;

  /// The [outlineVariant3] color
  final Color outlineVariant3;

  @override
  ThemeExtension<MColorScheme> copyWith({
    Brightness? brightness,
    MColor? primary,
    MColor? onPrimary,
    MColor? primaryContainer,
    MColor? onPrimaryContainer,
    MColor? secondary,
    MColor? onSecondary,
    MColor? secondaryContainer,
    MColor? onSecondaryContainer,
    MColor? tertiary,
    MColor? onTertiary,
    MColor? tertiaryContainer,
    MColor? onTertiaryContainer,
    MColor? error,
    MColor? onError,
    MColor? errorContainer,
    MColor? onErrorContainer,
    MColor? informational,
    MColor? onInformational,
    MColor? informationalContainer,
    MColor? onInformationalContainer,
    MColor? warning,
    MColor? onWarning,
    MColor? warningContainer,
    MColor? onWarningContainer,
    MColor? success,
    MColor? onSuccess,
    MColor? successContainer,
    MColor? onSuccessContainer,
    MColor? notification,
    MColor? onNotification,
    MColor? inActive,
    MColor? onInActive,
    MColor? inActiveContainer,
    MColor? onInActiveContainer,
    MColor? disabled,
    MColor? onDisabled,
    MColor? disabledContainer,
    MColor? onDisabledContainer,
    MColor? background,
    MColor? onBackground,
    MColor? onBackgroundProminent,
    MColor? onBackgroundVariant,
    MColor? surface,
    MColor? onSurface,
    MColor? surfaceTint,
    MColor? surfaceShade,
    MColor? onSurfaceShade,
    MColor? inverseSurface,
    MColor? onInverseSurface,
    MColor? inversePrimary,
    MColor? onInversePrimary,
    MColor? scrim,
    MColor? shadow,
    MColor? outline,
    MColor? outlineVariant1,
    MColor? outlineVariant2,
    MColor? outlineVariant3,
  }) {
    return MColorScheme(
      brightness: brightness ?? this.brightness,
      primary: primary ?? this.primary,
      onPrimary: onPrimary ?? this.onPrimary,
      primaryContainer: primaryContainer ?? this.primaryContainer,
      onPrimaryContainer: onPrimaryContainer ?? this.onPrimaryContainer,
      secondary: secondary ?? this.secondary,
      onSecondary: onSecondary ?? this.onSecondary,
      secondaryContainer: secondaryContainer ?? this.secondaryContainer,
      onSecondaryContainer: onSecondaryContainer ?? this.onSecondaryContainer,
      tertiary: tertiary ?? this.tertiary,
      onTertiary: onTertiary ?? this.onTertiary,
      tertiaryContainer: tertiaryContainer ?? this.tertiaryContainer,
      onTertiaryContainer: onTertiaryContainer ?? this.onTertiaryContainer,
      error: error ?? this.error,
      onError: onError ?? this.onError,
      errorContainer: errorContainer ?? this.errorContainer,
      onErrorContainer: onErrorContainer ?? this.onErrorContainer,
      informational: informational ?? this.informational,
      onInformational: onInformational ?? this.onInformational,
      informationalContainer:
          informationalContainer ?? this.informationalContainer,
      onInformationalContainer:
          onInformationalContainer ?? this.onInformationalContainer,
      warning: warning ?? this.warning,
      onWarning: onWarning ?? this.onWarning,
      warningContainer: warningContainer ?? this.warningContainer,
      onWarningContainer: onWarningContainer ?? this.onWarningContainer,
      success: success ?? this.success,
      onSuccess: onSuccess ?? this.onSuccess,
      successContainer: successContainer ?? this.successContainer,
      onSuccessContainer: onSuccessContainer ?? this.onSuccessContainer,
      notification: notification ?? this.notification,
      onNotification: onNotification ?? this.onNotification,
      inActive: inActive ?? this.inActive,
      onInActive: onInActive ?? this.onInActive,
      inActiveContainer: inActiveContainer ?? this.inActiveContainer,
      onInActiveContainer: onInActiveContainer ?? this.onInActiveContainer,
      disabled: disabled ?? this.disabled,
      onDisabled: onDisabled ?? this.onDisabled,
      disabledContainer: disabledContainer ?? this.disabledContainer,
      onDisabledContainer: onDisabledContainer ?? this.onDisabledContainer,
      background: background ?? this.background,
      onBackground: onBackground ?? this.onBackground,
      onBackgroundProminent:
          onBackgroundProminent ?? this.onBackgroundProminent,
      onBackgroundVariant: onBackgroundVariant ?? this.onBackgroundVariant,
      surface: surface ?? this.surface,
      onSurface: onSurface ?? this.onSurface,
      surfaceTint: surfaceTint ?? this.surfaceTint,
      surfaceShade: surfaceShade ?? this.surfaceShade,
      onSurfaceShade: onSurfaceShade ?? this.onSurfaceShade,
      inverseSurface: inverseSurface ?? this.inverseSurface,
      onInverseSurface: onInverseSurface ?? this.onInverseSurface,
      inversePrimary: inversePrimary ?? this.inversePrimary,
      onInversePrimary: onInversePrimary ?? this.onInversePrimary,
      scrim: scrim ?? this.scrim,
      shadow: shadow ?? this.shadow,
      outline: outline ?? this.outline,
      outlineVariant1: outlineVariant1 ?? this.outlineVariant1,
      outlineVariant2: outlineVariant2 ?? this.outlineVariant2,
      outlineVariant3: outlineVariant3 ?? this.outlineVariant3,
    );
  }

  @override
  ThemeExtension<MColorScheme> lerp(
    covariant ThemeExtension<MColorScheme>? other,
    double t,
  ) {
    if (other is! MColorScheme) return this;
    return MColorScheme(
      brightness: other.brightness,
      primary: Color.lerp(primary, other.primary, t)!,
      onPrimary: Color.lerp(onPrimary, other.onPrimary, t)!,
      primaryContainer: Color.lerp(
        primaryContainer,
        other.primaryContainer,
        t,
      )!,
      onPrimaryContainer: Color.lerp(
        onPrimaryContainer,
        other.onPrimaryContainer,
        t,
      )!,
      secondary: Color.lerp(secondary, other.secondary, t)!,
      onSecondary: Color.lerp(onSecondary, other.onSecondary, t)!,
      secondaryContainer: Color.lerp(
        secondaryContainer,
        other.secondaryContainer,
        t,
      )!,
      onSecondaryContainer: Color.lerp(
        onSecondaryContainer,
        other.onSecondaryContainer,
        t,
      )!,
      tertiary: Color.lerp(tertiary, other.tertiary, t)!,
      onTertiary: Color.lerp(onTertiary, other.onTertiary, t)!,
      tertiaryContainer: Color.lerp(
        tertiaryContainer,
        other.tertiaryContainer,
        t,
      )!,
      onTertiaryContainer: Color.lerp(
        onTertiaryContainer,
        other.onTertiaryContainer,
        t,
      )!,
      error: Color.lerp(error, other.error, t)!,
      onError: Color.lerp(onError, other.onError, t)!,
      errorContainer: Color.lerp(errorContainer, other.errorContainer, t)!,
      onErrorContainer: Color.lerp(
        onErrorContainer,
        other.onErrorContainer,
        t,
      )!,
      informational: Color.lerp(informational, other.informational, t)!,
      onInformational: Color.lerp(onInformational, other.onInformational, t)!,
      informationalContainer: Color.lerp(
        informationalContainer,
        other.informationalContainer,
        t,
      )!,
      onInformationalContainer: Color.lerp(
        onInformationalContainer,
        other.onInformationalContainer,
        t,
      )!,
      warning: Color.lerp(warning, other.warning, t)!,
      onWarning: Color.lerp(onWarning, other.onWarning, t)!,
      warningContainer: Color.lerp(
        warningContainer,
        other.warningContainer,
        t,
      )!,
      onWarningContainer: Color.lerp(
        onWarningContainer,
        other.onWarningContainer,
        t,
      )!,
      success: Color.lerp(success, other.success, t)!,
      onSuccess: Color.lerp(onSuccess, other.onSuccess, t)!,
      successContainer: Color.lerp(
        successContainer,
        other.successContainer,
        t,
      )!,
      onSuccessContainer: Color.lerp(
        onSuccessContainer,
        other.onSuccessContainer,
        t,
      )!,
      notification: Color.lerp(notification, other.notification, t)!,
      onNotification: Color.lerp(onNotification, other.onNotification, t)!,
      inActive: Color.lerp(inActive, other.inActive, t)!,
      onInActive: Color.lerp(onInActive, other.onInActive, t)!,
      inActiveContainer: Color.lerp(
        inActiveContainer,
        other.inActiveContainer,
        t,
      )!,
      onInActiveContainer: Color.lerp(
        onInActiveContainer,
        other.onInActiveContainer,
        t,
      )!,
      disabled: Color.lerp(disabled, other.disabled, t)!,
      onDisabled: Color.lerp(onDisabled, other.onDisabled, t)!,
      disabledContainer: Color.lerp(
        disabledContainer,
        other.disabledContainer,
        t,
      )!,
      onDisabledContainer: Color.lerp(
        onDisabledContainer,
        other.onDisabledContainer,
        t,
      )!,
      background: Color.lerp(background, other.background, t)!,
      onBackground: Color.lerp(onBackground, other.onBackground, t)!,
      onBackgroundProminent: Color.lerp(
        onBackgroundProminent,
        other.onBackgroundProminent,
        t,
      )!,
      onBackgroundVariant: Color.lerp(
        onBackgroundVariant,
        other.onBackgroundVariant,
        t,
      )!,
      surface: Color.lerp(surface, other.surface, t)!,
      onSurface: Color.lerp(onSurface, other.onSurface, t)!,
      surfaceTint: Color.lerp(surfaceTint, other.surfaceTint, t)!,
      surfaceShade: Color.lerp(surfaceShade, other.surfaceShade, t)!,
      onSurfaceShade: Color.lerp(onSurfaceShade, other.onSurfaceShade, t)!,
      inverseSurface: Color.lerp(inverseSurface, other.inverseSurface, t)!,
      onInverseSurface: Color.lerp(
        onInverseSurface,
        other.onInverseSurface,
        t,
      )!,
      inversePrimary: Color.lerp(inversePrimary, other.inversePrimary, t)!,
      onInversePrimary: Color.lerp(
        onInversePrimary,
        other.onInversePrimary,
        t,
      )!,
      scrim: Color.lerp(scrim, other.scrim, t)!,
      shadow: Color.lerp(shadow, other.shadow, t)!,
      outline: Color.lerp(outline, other.outline, t)!,
      outlineVariant1: Color.lerp(outlineVariant1, other.outlineVariant1, t)!,
      outlineVariant2: Color.lerp(outlineVariant2, other.outlineVariant2, t)!,
      outlineVariant3: Color.lerp(outlineVariant3, other.outlineVariant3, t)!,
    );
  }

  /// Retrieves the [MColorScheme] extension from the closest [Theme] instance
  /// that encloses the given [context].
  ///
  /// This method searches for the nearest [Theme] widget in the widget tree
  /// and returns the [MColorScheme] extension if it exists. If no
  /// [MColorScheme] extension is found, this method returns null.
  ///
  /// The [MColorScheme] extension must be added to the [ThemeData.extensions]
  /// in your theme configuration to be accessible using this method.
  ///
  /// Example usage:
  /// ```dart
  /// final mColorScheme = MColorScheme.of(context);
  /// ```
  ///
  /// - [context]: The build context from which to retrieve the [MColorScheme]
  /// extension.
  ///
  /// Returns the [MColorScheme] extension if found, or null if no
  /// [MColorScheme] extension is available in the closest [Theme] instance.
  static MColorScheme of(BuildContext context) {
    final brightness = MediaQuery.platformBrightnessOf(context);
    return Theme.of(context).extension<MColorScheme>() ?? .$default(brightness);
  }

  /// Creates the [ColorScheme] based the [MColorScheme] extension
  ColorScheme get getColorScheme {
    return ColorScheme(
      brightness: brightness!,
      primary: primary,
      onPrimary: onPrimary,
      onPrimaryContainer: onPrimaryContainer,
      primaryContainer: primaryContainer,
      secondary: secondary,
      onSecondary: onSecondary,
      secondaryContainer: secondaryContainer,
      onSecondaryContainer: onSecondaryContainer,
      tertiary: tertiary,
      onTertiary: onTertiary,
      tertiaryContainer: tertiaryContainer,
      onTertiaryContainer: onTertiaryContainer,
      error: error,
      onError: onError,
      errorContainer: errorContainer,
      onErrorContainer: onErrorContainer,
      surface: surface,
      onSurface: onSurface,
      surfaceTint: surfaceTint,
      inverseSurface: inverseSurface,
      onInverseSurface: onInverseSurface,
      inversePrimary: inversePrimary,
      outline: outline,
      outlineVariant: outlineVariant1,
      scrim: scrim,
      shadow: shadow,
    );
  }
}
