import 'package:envied/envied.dart';

part 'env.g.dart';

@Envied(path: '.env.dev', obfuscate: true)
abstract class DevEnv {
  @EnviedField(varName: 'MENO_API_URL')
  static final String menoApiUrl = _DevEnv.menoApiUrl;

  @EnviedField(varName: 'WEB_SOCKET_URL')
  static final String webSocketUrl = _DevEnv.webSocketUrl;

  @EnviedField(varName: 'MENO_LIVEKIT_URL')
  static final String menoLiveKitUrl = _DevEnv.menoLiveKitUrl;
}

@Envied(path: '.env.prod', obfuscate: true)
abstract class ProdEnv {
  @EnviedField(varName: 'MENO_API_URL')
  static final String menoApiUrl = _ProdEnv.menoApiUrl;

  @EnviedField(varName: 'WEB_SOCKET_URL')
  static final String webSocketUrl = _ProdEnv.webSocketUrl;

  @EnviedField(varName: 'MENO_LIVEKIT_URL')
  static final String menoLiveKitUrl = _ProdEnv.menoLiveKitUrl;
}
