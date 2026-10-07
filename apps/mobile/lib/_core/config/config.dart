import 'package:flutter/foundation.dart';

@immutable
final class Config {
  const new({
    required this.menoApiUrl,
    required this.webSocketUrl,
    required this.menoLiveKitUrl,
  });

  final String menoApiUrl;
  final String webSocketUrl;
  final String menoLiveKitUrl;
}
