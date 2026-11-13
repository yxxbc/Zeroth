# Credential lifecycle: refresh, recovery and logout

Date: 2026-11-14

## Decision

Keep `/login` as the connection entry point, add **Manage connections / sign out**
at its root, and make `/logout` open the same searchable page. It lists connected
accounts and configured API keys, including keys inherited from the environment.
Selecting a row asks for confirmation, distinguishing **Sign out** from **Remove
API key**. This is local disconnection, not provider-side token revocation.

Logout removes the selected connection's saved credential (including legacy
aliases), invalidates its cached authentication, cancels work using that connection,
and persists a disabled marker. That marker outranks environment variables and
generic key fallbacks, survives restart, and prevents an in-flight refresh from
restoring a removed credential. Login clears the marker as part of saving the new
credential. Parent-shell configuration is never rewritten.

## Refresh transactions

All credential-store edits take a bounded cross-process advisory lock on a
separate stable lock file; atomic replacement of the store cannot replace the lock.
A rotating provider also holds a per-credential lock through reading the latest
refresh token, the exchange, and persistence. A process with an older in-memory
configuration therefore presents the token another process most recently saved.
Failure to acquire a lock fails closed rather than replaying a rotating token.

Logout only takes the short store-edit lock, never the network refresh lock.
Refresh writes are conditional on the connection still being enabled and its
credential still belonging to the chain being refreshed. Persistence failures are
reported visibly without printing any credential; the live rotation remains in
memory so the current process can continue.

## Unauthorized recovery

An OAuth inference request rejected with HTTP 401 before any response content may
invalidate its cached access token and refresh once. A second 401 is final. API
keys, 403 responses, revoked/invalid/reused refresh tokens, and failures after
streamed content do not enter this recovery path. This cannot re-execute tools.

## Validation

Use offline tests for actual process exclusion, rotation from a stale configuration,
logout during refresh, disabled environment fallbacks, credential-file integrity,
and bounded 401 recovery. Exercise both logout entry points and API/account removal
with the real binary and local smoke fixtures. No real credential is required.
Long-duration live verification remains opt-in and cannot establish a service-level
guarantee for the providers' unofficial or unversioned endpoints.
