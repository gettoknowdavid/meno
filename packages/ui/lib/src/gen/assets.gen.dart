// dart format width=80

/// GENERATED CODE - DO NOT MODIFY BY HAND
/// *****************************************************
///  FlutterGen
/// *****************************************************

// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: deprecated_member_use,directives_ordering,implicit_dynamic_list_literal,unnecessary_import

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_svg/flutter_svg.dart' as _svg;
import 'package:vector_graphics/vector_graphics.dart' as _vg;

class $AssetsImagesGen {
  const $AssetsImagesGen();

  /// File path: assets/images/celebrate.png
  AssetGenImage get celebrate =>
      const AssetGenImage('assets/images/celebrate.png');

  /// File path: assets/images/clapping hands.png
  AssetGenImage get clappingHands =>
      const AssetGenImage('assets/images/clapping hands.png');

  /// File path: assets/images/collision.png
  AssetGenImage get collision =>
      const AssetGenImage('assets/images/collision.png');

  /// File path: assets/images/facebook.svg
  SvgGenImage get facebook => const SvgGenImage('assets/images/facebook.svg');

  /// File path: assets/images/finger snap.png
  AssetGenImage get fingerSnap =>
      const AssetGenImage('assets/images/finger snap.png');

  /// File path: assets/images/flame.png
  AssetGenImage get flame => const AssetGenImage('assets/images/flame.png');

  /// File path: assets/images/folder.png
  AssetGenImage get folder => const AssetGenImage('assets/images/folder.png');

  /// File path: assets/images/geometric lines.svg
  SvgGenImage get geometricLines =>
      const SvgGenImage('assets/images/geometric lines.svg');

  /// File path: assets/images/google.svg
  SvgGenImage get google => const SvgGenImage('assets/images/google.svg');

  /// File path: assets/images/high voltage.png
  AssetGenImage get highVoltage =>
      const AssetGenImage('assets/images/high voltage.png');

  /// File path: assets/images/ic_launcher.png
  AssetGenImage get icLauncher =>
      const AssetGenImage('assets/images/ic_launcher.png');

  /// File path: assets/images/live_for_you.png
  AssetGenImage get liveForYou =>
      const AssetGenImage('assets/images/live_for_you.png');

  /// File path: assets/images/loading.gif
  AssetGenImage get loading => const AssetGenImage('assets/images/loading.gif');

  /// File path: assets/images/logo-dark.svg
  SvgGenImage get logoDark => const SvgGenImage('assets/images/logo-dark.svg');

  /// File path: assets/images/logo-light.svg
  SvgGenImage get logoLight =>
      const SvgGenImage('assets/images/logo-light.svg');

  /// File path: assets/images/logo.png
  AssetGenImage get logo => const AssetGenImage('assets/images/logo.png');

  /// File path: assets/images/meno-purple.png
  AssetGenImage get menoPurple =>
      const AssetGenImage('assets/images/meno-purple.png');

  /// File path: assets/images/meno-white.png
  AssetGenImage get menoWhite =>
      const AssetGenImage('assets/images/meno-white.png');

  /// File path: assets/images/new-file.png
  AssetGenImage get newFile =>
      const AssetGenImage('assets/images/new-file.png');

  /// File path: assets/images/onboarding-1.png
  AssetGenImage get onboarding1 =>
      const AssetGenImage('assets/images/onboarding-1.png');

  /// File path: assets/images/onboarding-2.png
  AssetGenImage get onboarding2 =>
      const AssetGenImage('assets/images/onboarding-2.png');

  /// File path: assets/images/onboarding-3.png
  AssetGenImage get onboarding3 =>
      const AssetGenImage('assets/images/onboarding-3.png');

  /// File path: assets/images/onboarding-4.png
  AssetGenImage get onboarding4 =>
      const AssetGenImage('assets/images/onboarding-4.png');

  /// File path: assets/images/raising hands.png
  AssetGenImage get raisingHands =>
      const AssetGenImage('assets/images/raising hands.png');

  /// File path: assets/images/red heart.png
  AssetGenImage get redHeart =>
      const AssetGenImage('assets/images/red heart.png');

  /// File path: assets/images/sparkles.png
  AssetGenImage get sparkles =>
      const AssetGenImage('assets/images/sparkles.png');

  /// File path: assets/images/splash-logo.png
  AssetGenImage get splashLogo =>
      const AssetGenImage('assets/images/splash-logo.png');

  /// File path: assets/images/success.png
  AssetGenImage get success => const AssetGenImage('assets/images/success.png');

  /// File path: assets/images/thumbs up.png
  AssetGenImage get thumbsUp =>
      const AssetGenImage('assets/images/thumbs up.png');

  /// File path: assets/images/waving hand.png
  AssetGenImage get wavingHand =>
      const AssetGenImage('assets/images/waving hand.png');

  /// File path: assets/images/writing hand.png
  AssetGenImage get writingHand =>
      const AssetGenImage('assets/images/writing hand.png');

  /// List of all assets
  List<dynamic> get values => [
    celebrate,
    clappingHands,
    collision,
    facebook,
    fingerSnap,
    flame,
    folder,
    geometricLines,
    google,
    highVoltage,
    icLauncher,
    liveForYou,
    loading,
    logoDark,
    logoLight,
    logo,
    menoPurple,
    menoWhite,
    newFile,
    onboarding1,
    onboarding2,
    onboarding3,
    onboarding4,
    raisingHands,
    redHeart,
    sparkles,
    splashLogo,
    success,
    thumbsUp,
    wavingHand,
    writingHand,
  ];
}

abstract final class Assets {
  static const $AssetsImagesGen images = $AssetsImagesGen();
}

class AssetGenImage {
  const AssetGenImage(
    this._assetName, {
    this.size,
    this.flavors = const {},
    this.animation,
  });

  final String _assetName;

  final Size? size;
  final Set<String> flavors;
  final AssetGenImageAnimation? animation;

  Image image({
    Key? key,
    AssetBundle? bundle,
    ImageFrameBuilder? frameBuilder,
    ImageErrorWidgetBuilder? errorBuilder,
    String? semanticLabel,
    bool excludeFromSemantics = false,
    double? scale,
    double? width,
    double? height,
    Color? color,
    Animation<double>? opacity,
    BlendMode? colorBlendMode,
    BoxFit? fit,
    AlignmentGeometry alignment = Alignment.center,
    ImageRepeat repeat = ImageRepeat.noRepeat,
    Rect? centerSlice,
    bool matchTextDirection = false,
    bool gaplessPlayback = true,
    bool isAntiAlias = false,
    String? package,
    FilterQuality filterQuality = FilterQuality.medium,
    int? cacheWidth,
    int? cacheHeight,
  }) {
    return Image.asset(
      _assetName,
      key: key,
      bundle: bundle,
      frameBuilder: frameBuilder,
      errorBuilder: errorBuilder,
      semanticLabel: semanticLabel,
      excludeFromSemantics: excludeFromSemantics,
      scale: scale,
      width: width,
      height: height,
      color: color,
      opacity: opacity,
      colorBlendMode: colorBlendMode,
      fit: fit,
      alignment: alignment,
      repeat: repeat,
      centerSlice: centerSlice,
      matchTextDirection: matchTextDirection,
      gaplessPlayback: gaplessPlayback,
      isAntiAlias: isAntiAlias,
      package: package,
      filterQuality: filterQuality,
      cacheWidth: cacheWidth,
      cacheHeight: cacheHeight,
    );
  }

  ImageProvider provider({AssetBundle? bundle, String? package}) {
    return AssetImage(_assetName, bundle: bundle, package: package);
  }

  String get path => _assetName;

  String get keyName => _assetName;
}

class AssetGenImageAnimation {
  const AssetGenImageAnimation({
    required this.isAnimation,
    required this.duration,
    required this.frames,
  });

  final bool isAnimation;
  final Duration duration;
  final int frames;
}

class SvgGenImage {
  const SvgGenImage(this._assetName, {this.size, this.flavors = const {}})
    : _isVecFormat = false;

  const SvgGenImage.vec(this._assetName, {this.size, this.flavors = const {}})
    : _isVecFormat = true;

  final String _assetName;
  final Size? size;
  final Set<String> flavors;
  final bool _isVecFormat;

  _svg.SvgPicture svg({
    Key? key,
    bool matchTextDirection = false,
    AssetBundle? bundle,
    String? package,
    double? width,
    double? height,
    BoxFit fit = BoxFit.contain,
    AlignmentGeometry alignment = Alignment.center,
    bool allowDrawingOutsideViewBox = false,
    WidgetBuilder? placeholderBuilder,
    String? semanticsLabel,
    bool excludeFromSemantics = false,
    _svg.SvgTheme? theme,
    _svg.ColorMapper? colorMapper,
    ColorFilter? colorFilter,
    Clip clipBehavior = Clip.hardEdge,
    @deprecated Color? color,
    @deprecated BlendMode colorBlendMode = BlendMode.srcIn,
    @deprecated bool cacheColorFilter = false,
  }) {
    final _svg.BytesLoader loader;
    if (_isVecFormat) {
      loader = _vg.AssetBytesLoader(
        _assetName,
        assetBundle: bundle,
        packageName: package,
      );
    } else {
      loader = _svg.SvgAssetLoader(
        _assetName,
        assetBundle: bundle,
        packageName: package,
        theme: theme,
        colorMapper: colorMapper,
      );
    }
    return _svg.SvgPicture(
      loader,
      key: key,
      matchTextDirection: matchTextDirection,
      width: width,
      height: height,
      fit: fit,
      alignment: alignment,
      allowDrawingOutsideViewBox: allowDrawingOutsideViewBox,
      placeholderBuilder: placeholderBuilder,
      semanticsLabel: semanticsLabel,
      excludeFromSemantics: excludeFromSemantics,
      colorFilter:
          colorFilter ??
          (color == null ? null : ColorFilter.mode(color, colorBlendMode)),
      clipBehavior: clipBehavior,
      cacheColorFilter: cacheColorFilter,
    );
  }

  String get path => _assetName;

  String get keyName => _assetName;
}
