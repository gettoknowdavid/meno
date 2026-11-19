import 'package:flutter/foundation.dart';

/// The facts every report carries, whatever produced it.
///
/// Small on purpose: a crash report is only useful if the fields somebody
/// groups by are the same on every report, and a fixed set is what makes that
/// true. [accountId] is the only identity here — a UUID, never a name or an
/// email — because it is what support needs to find the person's other reports.
@immutable
final class TelemetryContext {
  const new({
    required this.env,
    required this.platform,
    required this.appVersion,
    required this.buildNumber,
    this.accountId,
  });

  /// The build's environment name: `dev` or `prod`.
  final String env;

  /// The platform the report came from, as Flutter names it.
  final String platform;

  /// The packaged version, or `unknown` until `PackageInfo` has answered.
  final String appVersion;

  /// The packaged build number, or `unknown` for the same reason.
  final String buildNumber;

  /// The signed-in account, when there is one.
  final String? accountId;

  /// A copy with the packaged version and build number filled in.
  TelemetryContext withAppInfo({
    required String version,
    required String build,
  }) => TelemetryContext(
    env: env,
    platform: platform,
    appVersion: version,
    buildNumber: build,
    accountId: accountId,
  );

  /// A copy that names the signed-in account, or forgets it on sign-out.
  ///
  /// An account identifier is not a secret, but it is only correct while that
  /// account is the active one: a report attributed to the previous account is
  /// worse than one attributed to nobody.
  TelemetryContext withAccount(String? id) => TelemetryContext(
    env: env,
    platform: platform,
    appVersion: appVersion,
    buildNumber: buildNumber,
    accountId: id,
  );

  Map<String, Object?> toJson() => {
    'env': env,
    'platform': platform,
    'appVersion': appVersion,
    'buildNumber': buildNumber,
    'accountId': ?accountId,
  };
}

/// A product event: what happened, not who it happened to.
@immutable
final class AnalyticsEvent {
  const new({
    required this.name,
    required this.properties,
    required this.timestamp,
    required this.context,
  });

  /// The event's name, e.g. `app_start`.
  final String name;

  /// The event's own fields. Flat values only: an event is a row, not a tree.
  final Map<String, Object?> properties;

  final DateTime timestamp;
  final TelemetryContext context;

  Map<String, Object?> toJson() => {
    'name': name,
    'timestamp': timestamp.toUtc().toIso8601String(),
    'properties': {
      for (final entry in properties.entries)
        entry.key: entry.value == null ? null : redactSecrets('${entry.value}'),
    },
    'context': context.toJson(),
  };
}

/// One uncaught failure, ready for a sink.
@immutable
final class CrashReport {
  const new({
    required this.error,
    required this.stackTrace,
    required this.scope,
    required this.fatal,
    required this.timestamp,
    required this.context,
  });

  /// The thrown object. Kept as thrown so a sink can inspect it; [toJson]
  /// renders it.
  final Object error;

  final StackTrace? stackTrace;

  /// Where it was caught: `flutter`, `platform`, `command`.
  final String scope;

  /// Whether the app could not continue. A framework error and a platform
  /// error are fatal; a command failure is not.
  final bool fatal;

  final DateTime timestamp;
  final TelemetryContext context;

  /// A stable identity for one failure, for de-duplication.
  ///
  /// The type and the first app frame: the same bug hit on every screen is one
  /// bug, and reporting it forty times in a minute tells nobody anything new.
  /// The message is deliberately not part of it — a message carrying a request
  /// id would make every report unique.
  String get fingerprint {
    final text = stackTrace?.toString();
    final frame = text == null
        ? ''
        : text
              .split('\n')
              .firstWhere(
                (line) => line.contains('package:meno_mobile'),
                orElse: () => '',
              );
    final key = '$scope|${error.runtimeType}|${frame.trim()}';
    return key.length <= 200 ? key : key.substring(0, 200);
  }

  Map<String, Object?> toJson() => {
    'type': error.runtimeType.toString(),
    'message': redactSecrets('$error'),
    'stackTrace': stackTrace == null
        ? null
        : redactSecrets(stackTrace.toString()),
    'scope': scope,
    'fatal': fatal,
    'timestamp': timestamp.toUtc().toIso8601String(),
    'context': context.toJson(),
  };
}

/// Masks credential-shaped values in [text] before a report leaves the device.
///
/// The same three shapes the logger redacts: a bearer header, a `key=value`
/// field, and a URL query string. Reports leave the process, so a token in an
/// exception message would otherwise leave with them.
String redactSecrets(String text) => text
    .replaceAllMapped(_bearerPattern, (match) => '${match[1]}<redacted>')
    .replaceAllMapped(
      _assignmentPattern,
      (match) => '${match[1]}=<redacted>',
    )
    .replaceAll(_urlQueryPattern, '?<redacted>');

final RegExp _bearerPattern = RegExp(
  r'(Bearer\s+)\S+',
  caseSensitive: false,
);

final RegExp _assignmentPattern = RegExp(
  r'\b(password|passwd|refresh_token|access_token|refreshToken|accessToken|'
  r'token|secret|api[_-]?key|code|otp)\b\s*[=:]\s*[^\s,;]+',
  caseSensitive: false,
);

final RegExp _urlQueryPattern = RegExp(r'''\?[^\s'"]+''');
