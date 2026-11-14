import 'dart:io' show File;

import 'package:dio/dio.dart' show FormData, MultipartFile;
import 'package:meno_mobile/_core/_core.dart';
import 'package:meno_mobile/_shared/_shared.dart';
import 'package:meno_mobile/features/profile/model/_model.dart';

/// The profile module's endpoints, one method each.
///
/// Only the four the profile screens need. Browsing, followers, favourites and
/// recordings are not here: they belong to the screens that show them, and a
/// service that owns three endpoints cannot drift from the API the way one that
/// owns a dozen can.
final class ProfileHttpService {
  /// Creates a service over [_client].
  const new(this._client);

  final HttpClient _client;

  /// `GET /users/me` — the signed-in account's own record.
  Future<MyProfile> me() => _client.get('/users/me', fromJson: _me);

  /// `GET /users/{id}` — another account's public profile.
  Future<Profile> profile(Id userId) {
    return _client.get(
      '/users/${userId.getOrCrash}',
      fromJson: (data) =>
          ProfileDto.fromJson(data as Map<String, dynamic>).toDomain,
    );
  }

  /// `GET /users` — one page of accounts matching [query].
  ///
  /// The search is the API's: it matches names and biographies, and it refuses
  /// a term shorter than its own minimum rather than answering everything. The
  /// result carries the viewer's standing ([Profile.isFollowing]) but not the
  /// counters' history, which is all the invite sheet needs.
  Future<CursorPage<Profile>> searchUsers(
    String query, {
    String? cursor,
    int limit = 20,
  }) {
    return _client.get(
      '/users',
      queryParameters: {'q': query, 'cursor': ?cursor, 'limit': limit},
      fromJson: (data) =>
          CursorPage.fromJson(data, _searchResult, logger: _client.logger),
    );
  }

  /// `POST /subscribers/{id}` — follows [userId].
  ///
  /// Answers with no body: the row the API wrote is the whole result, and the
  /// client already knows what it asked for. A caller that wants the *new*
  /// follower count reads the profile again.
  Future<void> follow(Id userId) =>
      _client.postUnit('/subscribers/${userId.getOrCrash}');

  /// `DELETE /subscribers/{id}` — stops following [userId].
  ///
  /// Not following them is a success, so this is idempotent from a client's
  /// point of view; the undoable follow command decides from the *previous*
  /// state, not from a second read.
  Future<void> unfollow(Id userId) =>
      _client.deleteUnit('/subscribers/${userId.getOrCrash}');

  /// `PATCH /users/me` — a partial update, answering with the fresh record.
  ///
  /// A field left null is untouched by the API; [bio] set to the empty string
  /// clears it, which is the only way to say "remove this" (the column is
  /// written with `COALESCE`, so SQL null cannot mean it). The API refuses a
  /// patch that carries neither field, so callers send only what changed.
  Future<MyProfile> updateMe({String? fullName, String? bio}) {
    return _client.patch(
      '/users/me',
      data: {'full_name': ?fullName, 'bio': ?bio},
      fromJson: _me,
    );
  }

  /// `POST /users/me/avatar` — stores [file] as the account's avatar.
  ///
  /// The file part is named `file`: the API's multipart extractor separates
  /// text parts from files by that name, and a part without a filename would be
  /// read as text. What the bytes *are* is decided server-side, by sniffing
  /// them — the part's declared type is not trusted, which is why none is sent.
  Future<AvatarUpload> uploadAvatar(File file) {
    return _client.upload(
      '/users/me/avatar',
      formData: FormData()
        ..files.add(
          MapEntry(
            'file',
            MultipartFile.fromFileSync(
              file.path,
              filename: file.uri.pathSegments.last,
            ),
          ),
        ),
      fromJson: (data) =>
          AvatarDto.fromJson(data as Map<String, dynamic>).toDomain,
    );
  }

  /// Reads the record both `/users/me` endpoints answer with.
  static MyProfile _me(dynamic data) =>
      MeDto.fromJson(data as Map<String, dynamic>).toDomain;

  /// Reads one hit of `GET /users` (`ProfileSearchResult` in the API).
  static Profile _searchResult(Map<String, dynamic> json) {
    final bio = json['bio'] as String?;
    return Profile(
      id: Id.fromWire(json['id'] as String? ?? ''),
      fullName: SingleLineString(json['fullName'] as String? ?? ''),
      bio: bio == null ? null : MultiLineString(bio),
      avatarUrl: ImageInput.fromUrl(json['avatarUrl'] as String?),
      isFollowing: json['isFollowing'] as bool? ?? false,
      stats: ProfileStats(
        broadcasts: json['broadcasts'] as int? ?? 0,
        followers: json['followers'] as int? ?? 0,
        following: json['following'] as int? ?? 0,
      ),
      createdAt: DateTime.tryParse(json['createdAt'] as String? ?? ''),
    );
  }
}
