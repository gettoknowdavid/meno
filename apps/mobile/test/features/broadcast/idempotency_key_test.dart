import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/services/http_client.dart';
import 'package:meno_mobile/features/broadcast/broadcast.dart';

import '../../_support/stub_api.dart';

/// The stable logical-operation key the API deduplicates a mutation by
/// (audit H20).
///
/// Three things make a key idempotency rather than decoration, and each is
/// asserted here: it belongs to the *operation* (so a retry carries the same
/// one), it is stored with the draft (so it survives a restart), and the
/// transport never invents one (a key generated per call would change on every
/// manual retry).
///
/// The stub adapter records request headers before any body is parsed, which is
/// what lets a call answer with a 400 and still prove what it sent.
void main() {
  const draftKey = '018f2b1e-0000-7000-8000-00000000c0de';
  const otherKey = '018f2b1e-0000-7000-8000-00000000beef';

  Id broadcastId() => Id.fromString('018f2b1e-0000-7000-8000-0000000000aa');

  /// The key header of the request at [index], or null when it carried none.
  String? sentKey(StubApi api, int index) {
    final headers = api.requests[index].headers;
    return headers[HttpClient.idempotencyHeader] as String?;
  }

  group('the transport forwards, never invents', () {
    test('a caller-supplied key is the header the API receives', () async {
      final api = StubApi([() async => envelope(status: 400)]);
      final http = BroadcastHttpService(stubClient(api));

      await expectLater(
        http.create(title: 'Sunday', idempotencyKey: draftKey),
        throwsA(isA<MenoException>()),
      );

      expect(api.requests, hasLength(1));
      expect(sentKey(api, 0), draftKey);
    });

    test('two attempts at one create carry the same key', () async {
      // What a retry after an ambiguous timeout looks like: the draft supplies
      // the key both times, so the second request replays rather than
      // creating again.
      final api = StubApi([
        () async => envelope(status: 500),
        () async => envelope(status: 400),
      ]);
      final http = BroadcastHttpService(stubClient(api));

      for (var attempt = 0; attempt < 2; attempt++) {
        await expectLater(
          http.create(title: 'Sunday', idempotencyKey: draftKey),
          throwsA(isA<MenoException>()),
        );
      }

      expect(api.requests, hasLength(2));
      expect(sentKey(api, 0), draftKey);
      expect(sentKey(api, 1), draftKey);
    });

    test('a second, distinct draft carries a different key', () async {
      final api = StubApi([
        () async => envelope(status: 400),
        () async => envelope(status: 400),
      ]);
      final http = BroadcastHttpService(stubClient(api));

      await expectLater(
        http.create(title: 'One', idempotencyKey: draftKey),
        throwsA(isA<MenoException>()),
      );
      await expectLater(
        http.create(title: 'Two', idempotencyKey: otherKey),
        throwsA(isA<MenoException>()),
      );

      expect(sentKey(api, 0), draftKey);
      expect(sentKey(api, 1), otherKey);
      expect(sentKey(api, 0), isNot(sentKey(api, 1)));
    });

    test('a mutation that names no key sends none', () async {
      // Not every write is protected, and a key added "for symmetry" would look
      // like protection the endpoint does not have.
      final api = StubApi([() async => envelope(status: 400)]);
      final http = BroadcastHttpService(stubClient(api));

      await expectLater(
        http.remove(broadcastId()),
        throwsA(isA<MenoException>()),
      );

      expect(sentKey(api, 0), isNull);
    });

    test('live, join and end forward their key; resume does not', () async {
      final api = StubApi([
        () async => envelope(status: 400),
        () async => envelope(status: 400),
        () async => envelope(status: 400),
        () async => envelope(status: 400),
      ]);
      final http = BroadcastHttpService(stubClient(api));

      await expectLater(
        http.goLive(broadcastId(), idempotencyKey: draftKey),
        throwsA(isA<MenoException>()),
      );
      await expectLater(
        http.join(broadcastId(), idempotencyKey: draftKey),
        throwsA(isA<MenoException>()),
      );
      await expectLater(
        http.end(broadcastId(), idempotencyKey: draftKey),
        throwsA(isA<MenoException>()),
      );
      // Resume answers with a freshly signed room token, so its body must never
      // be replayed: no key, therefore no cached answer.
      await expectLater(
        http.resume(broadcastId()),
        throwsA(isA<MenoException>()),
      );

      expect(sentKey(api, 0), draftKey);
      expect(sentKey(api, 1), draftKey);
      expect(sentKey(api, 2), draftKey);
      expect(sentKey(api, 3), isNull);
    });
  });

  group('the draft stores the key', () {
    test('a key written onto the draft survives the storage row', () {
      // The row is what ObjectBox holds and what a restart reads back; a key
      // that only lived in the manager would be gone after a crash between the
      // two.
      final draft = BroadcastDraft(
        id: Id.fromString('018f2b1e-0000-7000-8000-0000000000bb'),
        ownerId: Id.fromString('018f2b1e-0000-7000-8000-000000000040'),
        title: 'Sunday Service',
        idempotencyKey: draftKey,
      );

      final restored = draft.toRow.toDomain();

      expect(restored.idempotencyKey, draftKey);
      expect(restored.title, 'Sunday Service');
    });

    test('a draft written before the field existed reads as "no key yet"', () {
      // A row saved by the previous build has no key. It must read as empty
      // rather than as a value the manager would mistake for one it can reuse.
      final row = BroadcastDraftRow(
        draftId: '018f2b1e-0000-7000-8000-0000000000cc',
        ownerId: '018f2b1e-0000-7000-8000-000000000040',
      );

      expect(row.idempotencyKey, isEmpty);
      expect(row.toDomain().idempotencyKey, isEmpty);
    });
  });
}
