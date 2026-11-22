//! Request-only recovery for the tools-free summarizer.

use super::{ChatMessage, LlmError, agent::RoundOutcome};
use crate::stream::CancelToken;

const MAX_TRIMS: usize = 4;

/// Tracks the discarded range across all rounds of one compact turn. The
/// agent retains its original messages, including any later continuation;
/// only requests to the provider see the shorter history.
pub(super) struct CompactRetry {
    start: usize,
    next: usize,
    handoff: usize,
    remaining: usize,
}

impl CompactRetry {
    /// Take the boundaries before opening hooks append their notes. The
    /// caller's final message is the handoff, not conversation history.
    pub(super) fn new(messages: &[ChatMessage]) -> Self {
        let start = messages
            .iter()
            .take_while(|message| is_instruction(message))
            .count();
        Self {
            start,
            next: start,
            handoff: messages.len().saturating_sub(1),
            remaining: MAX_TRIMS,
        }
    }

    pub(super) fn run(
        &mut self,
        messages: &[ChatMessage],
        cancel: &CancelToken,
        mut round: impl FnMut(&[ChatMessage]) -> (RoundOutcome, bool),
    ) -> RoundOutcome {
        loop {
            if cancel.is_cancelled() {
                return RoundOutcome::Cancelled;
            }
            let (outcome, pre_output_provider_error) = if self.start == self.next {
                round(messages)
            } else {
                let shortened = [&messages[..self.start], &messages[self.next..]].concat();
                round(&shortened)
            };
            if !pre_output_provider_error
                || !matches!(&outcome, RoundOutcome::Failed(error) if is_input_overflow(error))
            {
                return outcome;
            }
            if cancel.is_cancelled() {
                return RoundOutcome::Cancelled;
            }
            let Some(end) = messages
                .get(self.next..self.handoff)
                .filter(|_| self.remaining > 0)
                .and_then(oldest_unit_end)
                .filter(|end| self.next + end < self.handoff)
            else {
                // Do not replace the conversation with a summary of no history.
                return outcome;
            };
            self.next += end;
            self.remaining -= 1;
        }
    }
}

fn is_instruction(message: &ChatMessage) -> bool {
    matches!(message.role.as_str(), "system" | "developer")
}

/// A plain message is one unit; a tool-call assistant and every immediately
/// following result are one indivisible unit (the derived context's layout).
fn oldest_unit_end(history: &[ChatMessage]) -> Option<usize> {
    let first = history.first()?;
    if is_instruction(first) || first.role == "tool" || first.tool_call_id.is_some() {
        return None;
    }
    let end = 1 + first.tool_calls.len();
    let results = history.get(1..end)?;
    if results.iter().zip(&first.tool_calls).any(|(result, call)| {
        result.role != "tool" || result.tool_call_id.as_deref() != Some(call.id.as_str())
    }) || history
        .get(end)
        .is_some_and(|next| next.role == "tool" || next.tool_call_id.is_some())
    {
        return None;
    }
    Some(end)
}

/// Recognize explicit input-limit rejections only. Status 0 is how adapters
/// represent provider error frames on HTTP 200; `stream_round` separately
/// verifies that the request reached inference and emitted no deltas.
fn is_input_overflow(error: &LlmError) -> bool {
    let LlmError::Api {
        status: 0 | 400 | 413 | 422,
        body,
    } = error
    else {
        return false;
    };
    let body = body.to_ascii_lowercase();
    [
        "context_length_exceeded",
        "context_window_exceeded",
        "maximum context length",
        "prompt is too long",
        "prompt too long",
        "input is too long for the model",
        "input exceeds the context window",
        "is loaded with a context window too small for this conversation",
    ]
    .iter()
    .any(|marker| body.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overflow() -> (RoundOutcome, bool) {
        (
            RoundOutcome::Failed(LlmError::Api {
                status: 400,
                body: "context_length_exceeded".into(),
            }),
            true,
        )
    }

    #[test]
    fn trim_budget_spans_rounds_and_never_drops_all_history() {
        let cancel = CancelToken::new();
        let messages: Vec<_> = (0..10)
            .map(|index| ChatMessage::user(index.to_string()))
            .collect();
        let mut retry = CompactRetry::new(&messages);
        let mut lengths = Vec::new();
        let outcome = retry.run(&messages, &cancel, |request| {
            lengths.push(request.len());
            if lengths.len() <= 2 {
                overflow()
            } else {
                (
                    RoundOutcome::Complete {
                        text: String::new(),
                    },
                    false,
                )
            }
        });
        assert!(matches!(outcome, RoundOutcome::Complete { .. }));
        let mut continued = messages.clone();
        continued.push(ChatMessage::user("continue"));
        let outcome = retry.run(&continued, &cancel, |request| {
            lengths.push(request.len());
            assert_eq!(request.last(), continued.last());
            overflow()
        });
        assert!(matches!(outcome, RoundOutcome::Failed(_)));
        assert_eq!(lengths, [10, 9, 8, 9, 8, 7]);

        let short = &messages[..2];
        let outcome = CompactRetry::new(short).run(short, &cancel, |request| {
            assert_eq!(request, short);
            overflow()
        });
        assert!(matches!(outcome, RoundOutcome::Failed(_)));
    }

    #[test]
    fn cancellation_stops_before_another_request() {
        let messages = [
            ChatMessage::user("old"),
            ChatMessage::user("new"),
            ChatMessage::user("handoff"),
        ];
        let cancel = CancelToken::new();
        let outcome = CompactRetry::new(&messages).run(&messages, &cancel, |request| {
            assert_eq!(request, messages);
            cancel.cancel();
            overflow()
        });
        assert!(matches!(outcome, RoundOutcome::Cancelled));
    }
}
