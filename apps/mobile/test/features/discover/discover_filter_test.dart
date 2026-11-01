import 'package:flutter_test/flutter_test.dart';
import 'package:meno_mobile/_routing/_routing.dart';
import 'package:meno_mobile/features/discover/discover.dart';

void main() {
  group('Filter', () {
    test('every tab has a chip label and a screen of its own', () {
      expect(Filter.values.map((filter) => filter.label), [
        'All',
        'Now Live',
        'Recently Live',
      ]);

      // The chip navigates, so two tabs sharing an address would be one screen
      // with two chips.
      expect(Filter.values.map((filter) => filter.route).toSet(), hasLength(3));
    });

    test('a location names the tab it is, and nothing else names one', () {
      for (final filter in Filter.values) {
        expect(Filter.fromLocation(filter.route), filter);
      }

      expect(Filter.fromLocation(MRoutes.discover), isNull);
      expect(Filter.fromLocation(MRoutes.discoverSearch), isNull);
      expect(Filter.fromLocation(MRoutes.home), isNull);
    });

    test('the tab addresses hang under discover', () {
      for (final filter in Filter.values) {
        expect(filter.route, startsWith('${MRoutes.discover}/'));
      }
    });
  });
}
