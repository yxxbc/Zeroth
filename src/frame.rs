//! Frame scheduling — coalesce many redraw requests into rate-limited draws.
//!
//! Ported from openai/codex's `FrameRequester`/`FrameScheduler`, with one
//! addition: a keypress that edits the composer carries its own urgency, so it
//! does not queue behind the animation floor while a reply streams
//! (`docs/architecture/input-latency.md`). The pure parts
//! ([`FrameRateLimiter`], [`soonest`]) are unit-tested with injected `Instant`s;
//! the async [`run_scheduler`] task that glues them to real time is covered by
//! `scripts/smoke.sh`, like the rest of the I/O boundary.

use std::time::{Duration, Instant};

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::time::sleep_until;

/// The shortest gap between two emitted frames — 120 fps. Ordinary redraws
/// (streamed text, status animations, navigation keys) can never produce frames
/// closer together than this.
pub const MIN_FRAME_INTERVAL: Duration = Duration::from_nanos(8_333_334);

/// The shortest gap before an **urgent** frame — a keypress that edits the
/// composer. It may jump ahead of the 120 fps floor, but still cannot repaint
/// the terminal without any gap at all (~500 fps), which is what stops a held
/// key or an unrecognised paste from saturating the pty. Matches the burst
/// window in `docs/architecture/input-latency.md`.
pub const URGENT_FRAME_INTERVAL: Duration = Duration::from_millis(2);

/// One request for a draw, with the urgency that decides which floor it gets.
#[derive(Debug, Clone, Copy)]
pub struct FrameRequest {
    /// When the requester asked for the draw (a relaxed request carries its
    /// requested time in the future instead — [`FrameRequester::schedule_frame_in`]).
    pub requested: Instant,
    /// A keypress that edits the composer: clamp against
    /// [`URGENT_FRAME_INTERVAL`] rather than [`MIN_FRAME_INTERVAL`].
    pub urgent: bool,
}

/// The scheduler's idle sleep when nothing is pending — effectively "never" until
/// the next request wakes it (mirrors codex's `ONE_YEAR` placeholder).
const IDLE_SLEEP: Duration = Duration::from_secs(60 * 60 * 24 * 365);

/// Rate-limits emitted frames: ordinary requests to one per
/// [`MIN_FRAME_INTERVAL`], urgent ones to one per [`URGENT_FRAME_INTERVAL`].
///
/// Pure: it makes decisions from injected `Instant`s and remembers only when the
/// last frame was emitted, so it is testable with no clock or runtime.
#[derive(Debug, Default)]
pub struct FrameRateLimiter {
    /// When the most recent frame was emitted; `None` until the first one.
    last_emitted_at: Option<Instant>,
}

impl FrameRateLimiter {
    /// A fresh limiter that has emitted nothing yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The earliest an ordinary frame requested at `requested` may actually be
    /// emitted: never sooner than [`MIN_FRAME_INTERVAL`] after the previous
    /// frame. With no previous frame the request stands as-is (an idle keystroke
    /// draws at once).
    #[must_use]
    pub fn clamp_deadline(&self, requested: Instant) -> Instant {
        self.clamp_request(FrameRequest {
            requested,
            urgent: false,
        })
    }

    /// Clamp a request against the floor its urgency earns. A keypress that
    /// edits the composer waits only [`URGENT_FRAME_INTERVAL`] after the last
    /// frame, so a 32 ms status animation or a streaming reply cannot hold it
    /// for a whole 8.33 ms beat; [`soonest`] then pulls the pending deadline
    /// forward. Every other requester keeps the 120 fps coalescing floor.
    #[must_use]
    pub fn clamp_request(&self, request: FrameRequest) -> Instant {
        let floor = if request.urgent {
            URGENT_FRAME_INTERVAL
        } else {
            MIN_FRAME_INTERVAL
        };
        match self.last_emitted_at {
            Some(last) => request.requested.max(last + floor),
            None => request.requested,
        }
    }

    /// Record that a frame was emitted for deadline `at`, so the next request is
    /// clamped against its floor from there.
    pub fn mark_emitted(&mut self, at: Instant) {
        self.last_emitted_at = Some(at);
    }
}

/// Fold a newly-requested (already clamped) `deadline` into the `pending` one,
/// keeping the **soonest** — so many requests between two frames coalesce into a
/// single draw at the earliest of their deadlines.
#[must_use]
pub fn soonest(pending: Option<Instant>, deadline: Instant) -> Instant {
    match pending {
        Some(p) => p.min(deadline),
        None => deadline,
    }
}

/// A cloneable handle for asking the [`run_scheduler`] task to draw a frame. The
/// loop holds one and calls [`schedule_frame`] after every state change; the
/// scheduler coalesces a burst of these into a single rate-limited draw.
///
/// [`schedule_frame`]: FrameRequester::schedule_frame
#[derive(Debug, Clone)]
pub struct FrameRequester {
    tx: UnboundedSender<FrameRequest>,
}

impl FrameRequester {
    fn request(&self, urgent: bool) {
        // The scheduler outlives every requester, so a send only fails during
        // shutdown — nothing left to draw, so dropping the request is correct.
        let _ = self.tx.send(FrameRequest {
            requested: Instant::now(),
            urgent,
        });
    }

    /// Request a redraw as soon as the frame floor allows (immediately when idle).
    pub fn schedule_frame(&self) {
        self.request(false);
    }

    /// Request a redraw that must not wait behind the 120 fps floor — **only**
    /// for a keypress that edits the composer (`Session::schedule_for_key`).
    /// The scheduler keeps the soonest pending deadline, so this also pulls an
    /// already-queued animation frame earlier, and [`URGENT_FRAME_INTERVAL`]
    /// still bounds a held key to ~500 fps. Streamed text, mouse moves,
    /// resizes and relaxed paste-burst characters never take this path.
    pub fn schedule_frame_urgent(&self) {
        self.request(true);
    }

    /// Request a redraw no sooner than `after` from now — a *relaxed* request
    /// (a detected input burst's characters, the status line's next animation
    /// tick) that doesn't demand an immediate frame. The scheduler still keeps
    /// the **soonest** pending deadline, so this never postpones a frame someone
    /// else already asked for — it only avoids piling immediate demands on the
    /// rate limiter (see [`crate::paste`] and `run_scheduler`).
    pub fn schedule_frame_in(&self, after: Duration) {
        let _ = self.tx.send(FrameRequest {
            requested: Instant::now() + after,
            urgent: false,
        });
    }
}

/// Create a [`FrameRequester`] and the receiver its [`run_scheduler`] consumes.
#[must_use]
pub fn channel() -> (FrameRequester, UnboundedReceiver<FrameRequest>) {
    let (tx, rx) = unbounded_channel();
    (FrameRequester { tx }, rx)
}

/// The scheduler task: receive requested draws, coalesce them and rate-limit
/// each to the floor its urgency earns, and emit exactly one tick on `draw_tx`
/// per due frame. Returns when every [`FrameRequester`] has been dropped (or the
/// draw consumer hung up), so the runtime can shut down cleanly.
///
/// The timing is real, so this is exercised by `scripts/smoke.sh`; the pure
/// decisions it leans on ([`FrameRateLimiter::clamp_request`], [`soonest`]) are
/// unit-tested above.
pub async fn run_scheduler(
    mut req_rx: UnboundedReceiver<FrameRequest>,
    draw_tx: UnboundedSender<()>,
) {
    let mut limiter = FrameRateLimiter::new();
    let mut pending: Option<Instant> = None;
    loop {
        let target = pending.unwrap_or_else(|| Instant::now() + IDLE_SLEEP);
        let sleep = sleep_until(tokio::time::Instant::from_std(target));
        tokio::pin!(sleep);
        tokio::select! {
            request = req_rx.recv() => {
                match request {
                    // Fold the (rate-limit-clamped) request into the pending
                    // deadline, keeping the soonest — an urgent keypress can
                    // therefore pull an already-queued animation frame earlier.
                    Some(request) => {
                        let clamped = limiter.clamp_request(request);
                        pending = Some(soonest(pending, clamped));
                    }
                    None => break, // all requesters dropped → nothing more to draw
                }
            }
            // Only fires while a frame is pending (idle, the year-long sleep never
            // wins). Emit one tick, record it for the next clamp, and go idle.
            () = &mut sleep, if pending.is_some() => {
                limiter.mark_emitted(target);
                pending = None;
                if draw_tx.send(()).is_err() {
                    break; // the loop hung up → stop scheduling
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_with_no_prior_frame_keeps_the_request() {
        let limiter = FrameRateLimiter::new();
        let t = Instant::now();
        assert_eq!(limiter.clamp_deadline(t), t, "idle: draw when asked");
    }

    #[test]
    fn clamp_pushes_a_too_soon_request_to_the_frame_floor() {
        let mut limiter = FrameRateLimiter::new();
        let base = Instant::now();
        limiter.mark_emitted(base);
        // Requested only 2ms later, but the floor is 8.33ms past the last frame.
        let requested = base + Duration::from_millis(2);
        assert_eq!(
            limiter.clamp_deadline(requested),
            base + MIN_FRAME_INTERVAL,
            "a request inside the frame interval is delayed to the floor"
        );
    }

    #[test]
    fn clamp_leaves_a_late_enough_request_alone() {
        let mut limiter = FrameRateLimiter::new();
        let base = Instant::now();
        limiter.mark_emitted(base);
        // Requested well past the interval — no need to delay it.
        let requested = base + Duration::from_millis(20);
        assert_eq!(limiter.clamp_deadline(requested), requested);
    }

    #[test]
    fn soonest_takes_the_pending_deadline_when_it_is_earlier() {
        let base = Instant::now();
        let earlier = base;
        let later = base + Duration::from_millis(5);
        assert_eq!(soonest(Some(earlier), later), earlier);
    }

    #[test]
    fn soonest_takes_the_new_deadline_when_it_is_earlier() {
        let base = Instant::now();
        let later = base + Duration::from_millis(5);
        assert_eq!(soonest(Some(later), base), base);
    }

    #[test]
    fn soonest_with_nothing_pending_is_the_new_deadline() {
        let t = Instant::now();
        assert_eq!(soonest(None, t), t);
    }

    #[test]
    fn urgent_request_uses_its_own_shorter_floor() {
        let mut limiter = FrameRateLimiter::new();
        let base = Instant::now();
        limiter.mark_emitted(base);
        let requested = base + Duration::from_millis(1);
        assert_eq!(
            limiter.clamp_request(FrameRequest {
                requested,
                urgent: true
            }),
            base + URGENT_FRAME_INTERVAL,
            "a keypress must not queue behind the 8.33 ms animation floor"
        );
        assert_eq!(
            limiter.clamp_request(FrameRequest {
                requested,
                urgent: false
            }),
            base + MIN_FRAME_INTERVAL,
            "an ordinary redraw still coalesces at 120 fps"
        );
    }

    #[test]
    fn urgent_request_with_no_prior_frame_draws_at_once() {
        let limiter = FrameRateLimiter::new();
        let t = Instant::now();
        assert_eq!(
            limiter.clamp_request(FrameRequest {
                requested: t,
                urgent: true
            }),
            t
        );
    }

    #[test]
    fn urgent_request_late_enough_is_left_alone() {
        let mut limiter = FrameRateLimiter::new();
        let base = Instant::now();
        limiter.mark_emitted(base);
        let requested = base + Duration::from_millis(20);
        assert_eq!(
            limiter.clamp_request(FrameRequest {
                requested,
                urgent: true
            }),
            requested
        );
    }

    #[test]
    fn soonest_pulls_a_pending_animation_deadline_forward_for_a_keypress() {
        let mut limiter = FrameRateLimiter::new();
        let base = Instant::now();
        limiter.mark_emitted(base);
        let requested = base + Duration::from_millis(1);
        let animation = limiter.clamp_request(FrameRequest {
            requested,
            urgent: false,
        });
        let keypress = limiter.clamp_request(FrameRequest {
            requested,
            urgent: true,
        });
        let pending = soonest(None, animation);
        assert_eq!(
            soonest(Some(pending), keypress),
            base + URGENT_FRAME_INTERVAL,
            "a keypress already queued behind an animation tick pulls the draw earlier"
        );
    }
}
