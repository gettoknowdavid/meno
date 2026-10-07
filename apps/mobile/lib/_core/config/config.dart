import 'package:flutter/foundation.dart';

@immutable
final class Config {
  const new({
    required this.env,
    required this.menoApiUrl,
    required this.webSocketUrl,
    required this.menoLiveKitUrl,
  });

  final Environment env;
  final String menoApiUrl;
  final String webSocketUrl;
  final String menoLiveKitUrl;
}

enum Environment {
  dev,
  prod;

  bool get isProd => this == prod;
  bool get isDev => this == dev;
}

final class Scope {
  const new _();

  static const String root = 'root';
  static const String authenticated = 'authenticated';
}
