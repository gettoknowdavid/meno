/// Features that exist in the code but are not shipping in this build.
///
/// One place to flip, rather than commented-out widgets scattered across the
/// files that offer them. Guards are always `if (FeatureFlags.x)` — never an
/// assert or a build mode — so a debug build behaves like the release build.
abstract final class FeatureFlags {
  /// Recording a broadcast, plus everything that only makes sense with one.
  ///
  /// Nothing behind this exists yet: no LiveKit egress is started, nothing
  /// writes `recording_url`, there is no publish endpoint and no player. The
  /// editor's record switch, the ended screen's "Publish Broadcast" / "Listen
  /// Back" and the details sheet's "Listen Later" all promise that flow, so
  /// they are hidden rather than offered as dead ends.
  static const bool recording = false;

  /// Copying a finished broadcast into a new one.
  ///
  /// There is no API for it and `POST /broadcasts/{id}/live` refuses an ended
  /// broadcast, so the disabled control is hidden instead of shown.
  static const bool restreaming = false;
}
