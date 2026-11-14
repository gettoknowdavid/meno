import 'dart:convert';
import 'dart:typed_data';

import 'package:dio/dio.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:mocktail/mocktail.dart';

class MockMenoLogger extends Mock implements MenoLogger;

/// The API's envelope around [data], as a response body.
ResponseBody envelope({Map<String, dynamic>? data, int status = 200}) {
  return ResponseBody.fromString(
    jsonEncode({
      'code': status < 300 ? 'OK' : MenoErrorCode.internal,
      'message': '',
      'status': status < 300,
      'data': ?data,
    }),
    status,
    headers: {
      Headers.contentTypeHeader: [Headers.jsonContentType],
    },
  );
}

/// A dio adapter that answers from a script rather than the network.
///
/// The app's own [HttpClient] is used on top of it, so a test exercises the
/// real envelope parsing, error mapping and logging. An answer past the end of
/// [answers] is an empty success, which keeps requests a test does not care
/// about from failing it.
final class StubApi implements HttpClientAdapter {
  new([List<Future<ResponseBody> Function()>? answers])
    : answers = answers ?? <Future<ResponseBody> Function()>[];

  final List<Future<ResponseBody> Function()> answers;

  /// Every request that reached the adapter, in order.
  final List<RequestOptions> requests = [];

  @override
  Future<ResponseBody> fetch(
    RequestOptions options,
    Stream<Uint8List>? requestStream,
    Future<void>? cancelFuture,
  ) {
    requests.add(options);
    final index = requests.length - 1;
    if (index < answers.length) return answers[index]();
    return Future<ResponseBody>.value(envelope());
  }

  @override
  void close({bool force = false}) {}
}

/// The app's client over [api], with no interceptors.
HttpClient stubClient(StubApi api, {MenoLogger? logger}) => HttpClient(
  baseUrl: 'https://api.test',
  logger: logger ?? MockMenoLogger(),
  dio: Dio()..httpClientAdapter = api,
);
