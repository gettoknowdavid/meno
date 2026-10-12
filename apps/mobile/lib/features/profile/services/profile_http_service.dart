import 'dart:io' show Platform;

import 'package:dio/dio.dart' show CancelToken, FormData, MultipartFile;
import 'package:meno/_core/_core.dart';
import 'package:meno/_shared/_shared.dart';
import 'package:meno/features/profile/model/_model.dart';

/// Remote data source for the profile feature.
///
/// Wraps every API call the profile domain needs:
/// - Own profile: get, edit
/// - Browse profiles: paginated list
/// - Subscribers / Subscriptions: paginated lists
/// - Subscribe / Unsubscribe actions
/// - Favorites: list, add, remove
/// - Recordings: list
///
/// All methods throw [MenoException] on failure — the repository layer
/// catches and converts to `Either`.
final class ProfileHttpService {
  const ProfileHttpService(this._client);

  final HttpClient _client;

  // =========================================================================
  // OWN PROFILE
  // =========================================================================

  /// Fetches the full profile for [userId].
  Future<Profile> getProfile(Id userId, {CancelToken? cancelToken}) {
    return _client.get(
      '/users/${userId.getOrCrash()}/profile',
      fromJson: (json) => ProfileDto.fromJson(json).toDomain,
      cancelToken: cancelToken,
    );
  }

  /// Updates the authenticated user's profile.
  ///
  /// Sends a `multipart/form-data` PUT so an optional [image] file can be
  /// included in the same request as the text fields.
  ///
  /// Returns the updated [Profile] reflecting the server's persisted state.
  Future<Profile> editProfile({
    required Id userId,
    SingleLineString? fullName,
    MultiLineString? bio,
    ImageInput? image,
    CancelToken? cancelToken,
  }) async {
    final form = FormData();

    if (fullName != null) {
      form.fields.add(MapEntry('fullName', fullName.getOrCrash()));
    }

    if (bio != null) {
      form.fields.add(MapEntry('bio', bio.getOrCrash()));
    }

    final imageFile = image?.getFile();
    if (imageFile != null) {
      form.files.add(
        MapEntry(
          'image',
          await MultipartFile.fromFile(
            imageFile.path,
            filename: imageFile.path.split(Platform.pathSeparator).last,
          ),
        ),
      );
    }

    return _client.put(
      '/users/${userId.getOrCrash()}/profile',
      data: form,
      fromJson: (json) => ProfileDto.fromJson(json).toDomain,
      cancelToken: cancelToken,
    );
  }

  // =========================================================================
  // PROFILE BROWSE
  // =========================================================================

  /// Returns a paginated list of user profiles.
  Future<PagedList<Profile?>> getProfiles({
    OrderBy orderBy = OrderBy.asc,
    SortBy sortBy = SortBy.fullName,
    PaginationParams pagination = const PaginationParams(),
    String? keywords,
    bool includeSubscribed = true,
    CancelToken? cancelToken,
  }) {
    final queryParameters = <String, dynamic>{
      'orderBy': orderBy.value,
      'sortBy': sortBy.value,
      'page': pagination.page,
      'size': pagination.size,
      if (keywords != null && keywords.isNotEmpty) 'keywords': keywords,
      if (includeSubscribed) 'include': 'subscribed',
    };

    return _client.get(
      '/users/profiles',
      queryParameters: queryParameters,
      fromJson: (json) => PagedList<Profile?>.fromJson(
        json,
        (jsonT) => ProfileDto.fromJson(jsonT).toDomain,
        listKey: 'users',
      ),
      cancelToken: cancelToken,
    );
  }

  // =========================================================================
  // SUBSCRIBERS / SUBSCRIPTIONS
  // =========================================================================

  /// Returns a paginated list of users who subscribe to `subscriptionId`.
  Future<PagedList<Profile?>> getSubscribers({
    required Id subscriptionId,
    PaginationParams pagination = const PaginationParams(),
    String? keywords,
    bool includeSubscribed = true,
    CancelToken? cancelToken,
  }) {
    final queryParameters = <String, dynamic>{
      'subscriptionId': subscriptionId.getOrCrash(),
      'page': pagination.page,
      'size': pagination.size,
      if (keywords != null && keywords.isNotEmpty) 'keywords': keywords,
      if (includeSubscribed) 'include': 'subscribed',
    };

    return _client.get(
      '/subscribers',
      queryParameters: queryParameters,
      fromJson: (json) => PagedList<Profile?>.fromJson(
        json,
        (jsonT) => ProfileDto.fromJson(jsonT).toDomain,
        listKey: 'subscribers',
      ),
      cancelToken: cancelToken,
    );
  }

  /// Returns a paginated list of users that `subscriberId` follows.
  Future<PagedList<Profile?>> getSubscriptions({
    required Id subscriberId,
    PaginationParams pagination = const PaginationParams(),
    String? keywords,
    bool includeSubscribed = true,
    CancelToken? cancelToken,
  }) {
    final queryParameters = <String, dynamic>{
      'subscriberId': subscriberId.getOrCrash(),
      'page': pagination.page,
      'size': pagination.size,
      if (keywords != null && keywords.isNotEmpty) 'keywords': keywords,
      if (includeSubscribed) 'include': 'subscribed',
    };

    return _client.get(
      '/subscribers',
      queryParameters: queryParameters,
      fromJson: (json) => PagedList<Profile?>.fromJson(
        json,
        (jsonT) => ProfileDto.fromJson(jsonT).toDomain,
        listKey: 'subscribers',
      ),
      cancelToken: cancelToken,
    );
  }

  /// Subscribes the authenticated user to [targetUserId].
  Future<void> subscribe(Id targetUserId, {CancelToken? cancelToken}) {
    return _client.postUnit(
      '/subscribers',
      data: {'userId': targetUserId.getOrCrash()},
      cancelToken: cancelToken,
    );
  }

  /// Unsubscribes the authenticated user from [targetUserId].
  Future<void> unsubscribe(Id targetUserId, {CancelToken? cancelToken}) {
    return _client.deleteUnit(
      '/subscribers',
      data: {'userId': targetUserId.getOrCrash()},
      cancelToken: cancelToken,
    );
  }
}
