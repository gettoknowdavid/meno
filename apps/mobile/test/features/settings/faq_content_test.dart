import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/features/settings/model/_model.dart';

void main() {
  group('the bundled FAQ content', () {
    test('has enough to be worth a screen', () {
      final questions = [
        for (final section in faqSections)
          for (final entry in section.entries) entry.question,
      ];

      // A help screen with five entries is a stub. This is the guard against
      // someone deleting their way to one.
      expect(questions.length, greaterThanOrEqualTo(30));
      expect(faqSections.length, greaterThanOrEqualTo(8));
    });

    test('never asks the same question twice', () {
      final questions = [
        for (final section in faqSections)
          for (final entry in section.entries) entry.question,
      ];

      expect(
        questions.toSet().length,
        questions.length,
        reason: 'a duplicate question is two answers that will disagree',
      );
    });

    test('has a real answer for every question', () {
      for (final section in faqSections) {
        expect(section.title.trim(), isNotEmpty);
        for (final entry in section.entries) {
          expect(entry.question.trim(), isNotEmpty);
          // Long enough to answer something; short enough for a phone.
          expect(entry.answer.trim().length, greaterThan(40));
        }
      }
    });

    test('does not promise features the build hides', () {
      // Recording and listen-later are gated off (`FeatureFlags`). The entry
      // that raises them has to say so; an answer that simply describes them
      // is the dead end this screen exists to remove.
      const gated = {'record', 'recording', 'replay', 'listen later'};

      for (final section in faqSections) {
        for (final entry in section.entries) {
          final raisesGatedFeature = entry.keywords.any(gated.contains);
          if (!raisesGatedFeature) continue;

          final answer = entry.answer.toLowerCase();
          expect(
            answer.contains('not yet') ||
                answer.contains('not in this build') ||
                answer.contains('hidden'),
            isTrue,
            reason: '${entry.question} implies a gated feature works',
          );
        }
      }
    });
  });

  group('searching the FAQ content', () {
    test('an empty query returns everything', () {
      expect(searchFaqSections('').length, faqSections.length);
      expect(faqHasResults(''), isTrue);
    });

    test('matches the question, the answer and the keywords', () {
      final byQuestion = searchFaqSections('change my password');
      expect(byQuestion, isNotEmpty);

      // "otp" appears nowhere in the question or answer text.
      final byKeyword = searchFaqSections('otp');
      expect(byKeyword, isNotEmpty);

      final byAnswer = searchFaqSections('spam folder');
      expect(byAnswer, isNotEmpty);
    });

    test('is case-insensitive and trims the query', () {
      List<String> questionsFor(String query) => [
        for (final section in searchFaqSections(query))
          for (final entry in section.entries) entry.question,
      ];

      expect(
        questionsFor('  PASSWORD  '),
        questionsFor('password'),
      );
    });

    test('drops sections that have no match', () {
      final sections = searchFaqSections('co-host');

      expect(sections, isNotEmpty);
      for (final section in sections) {
        expect(section.entries, isNotEmpty);
      }
      // The point of dropping empties: no heading over nothing.
      expect(
        sections.length,
        lessThan(faqSections.length),
      );
    });

    test('a query nothing matches is empty, not everything', () {
      expect(searchFaqSections('zzzzz'), isEmpty);
      expect(faqHasResults('zzzzz'), isFalse);
    });
  });
}
