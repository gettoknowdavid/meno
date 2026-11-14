/// The outcome of one serialized vault mutation.
///
/// A native sealed class: every caller gets an exhaustive `switch`, and no
/// code generation is involved.
sealed class MutationResult {
  const new();
}

/// The change was written and is now the committed in-memory truth.
final class MutationCommitted extends MutationResult {
  const new(this.envelopeRevision);

  /// The envelope revision that is now stored.
  final int envelopeRevision;
}

/// The record the caller expected (account, epoch, revision) is no longer
/// current, so nothing was written.
final class MutationStale extends MutationResult {
  const new();
}

/// The write threw. Memory is untouched, so the caller can show one retry
/// state and try again.
final class MutationStorageFailed extends MutationResult {
  const new();
}

/// The mutation never ran: the service was not initialised, was closing, or was
/// asked for an invalid change.
final class MutationRejected extends MutationResult {
  const new(this.reason);

  final MutationRejection reason;
}

enum MutationRejection {
  notInitialized,
  closing,
  invalidEnvelope,
  unknownAccount,
}
