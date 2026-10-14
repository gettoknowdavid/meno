/// One page of a cursor-paginated endpoint.
///
/// Every list endpoint answers with this shape — see `CursorPage` in
/// `crates/core/src/pagination.rs`:
///
/// ```json
/// { "data": [...], "next_cursor": "opaque", "has_next_page": true }
/// ```
///
/// The cursor is opaque: the client stores it and hands it back, and never
/// builds one. Whether another page exists is stated separately rather than
/// inferred from the cursor being null, so a feed does not have to guess.
final class CursorPage<T> {
  const new({
    required this.items,
    required this.nextCursor,
    required this.hasNextPage,
  });

  /// Reads a page, decoding each entry of `data` with [itemFromJson].
  factory fromJson(
    dynamic json,
    T Function(Map<String, dynamic> item) itemFromJson,
  ) {
    if (json is! Map<String, dynamic>) {
      throw const FormatException(
        'Expected a cursor page but got a different value.',
      );
    }

    final data = json['data'];

    final items = <T>[];

    if (data is List) {
      for (final item in data) {
        if (item is Map<String, dynamic>) {
          items.add(itemFromJson(item));
        }
      }
    }

    return CursorPage<T>(
      items: items,
      nextCursor: json['next_cursor'] as String?,
      hasNextPage: json['has_next_page'] as bool? ?? false,
    );
  }

  /// The page's items, oldest first.
  final List<T> items;

  /// The cursor that asks for the next page, or null on the last one.
  final String? nextCursor;

  /// Whether a next page exists.
  final bool hasNextPage;

  /// An empty page, for a feed that has not loaded yet.
  static CursorPage<T> empty<T>() {
    return CursorPage<T>(items: const [], nextCursor: null, hasNextPage: false);
  }
}
