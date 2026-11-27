//! `/loop` and `/goal` command parsing and pure state
//! (`crates/lm-engine/src/schedule/parse.rs` style).

/// Minimum wakeup / loop interval in seconds (clamped so tight loops cannot
/// spin uncontrollably).
pub const WAKEUP_MIN_SECS: u64 = 60;

/// Maximum automatic rounds for a single `/goal` run before pausing.
pub const MAX_GOAL_ROUNDS: usize = 20;

/// Built-in prompt for a bare `/loop` (autonomous self-paced check-in).
pub const AUTONOMOUS_LOOP_PROMPT: &str = "Autonomous loop check-in: inspect any ongoing work, background tasks, workflows, or repository state, make progress if needed, and call schedule_wakeup when you want to check again (or stop=true when finished).";

/// Parsed `/loop` invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopCommand {
    /// `/loop` with no arguments: autonomous self-paced loop.
    Autonomous,
    /// `/loop <interval> <prompt>` / `/loop every <interval> <prompt>` / `/loop <prompt> every <interval>`.
    Fixed { interval_secs: u64, prompt: String },
    /// `/loop <prompt>`: dynamic loop where the model calls `schedule_wakeup` each turn.
    Dynamic { prompt: String },
    /// `/loop off|stop|clear`.
    Off,
}

/// Active loop mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopMode {
    Autonomous,
    Fixed,
    Dynamic,
}

/// Active `/loop` state tracked by [`super::App`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopState {
    pub mode: LoopMode,
    pub prompt: String,
    pub interval_secs: Option<u64>,
    pub wakeup_called_this_turn: bool,
}

/// Parsed `/goal` invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoalCommand {
    /// `/goal <objective>`.
    Start(String),
    /// `/goal off|stop|clear`.
    Off,
    /// `/goal` with no arguments.
    Help,
}

/// Active `/goal` state tracked by [`super::App`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalState {
    pub objective: String,
    pub round: usize,
    pub stopped_by_model: bool,
}

impl GoalState {
    /// The prompt sent for the initial or subsequent round of a `/goal`.
    #[must_use]
    pub fn round_prompt(&self) -> String {
        if self.round <= 1 {
            format!(
                "Goal: {}\n\nWork step by step toward this goal. When it is completely achieved or genuinely blocked, call `schedule_wakeup` with `stop: true` and summarize.",
                self.objective
            )
        } else {
            format!(
                "Continue working toward the goal (round {}/{}): {}\n\nVerify what has been done and take the next step. If the goal is completely achieved or genuinely blocked, call `schedule_wakeup` with `stop: true` and summarize.",
                self.round, MAX_GOAL_ROUNDS, self.objective
            )
        }
    }
}

/// Parse `/loop` arguments (`""`, `"off|stop|clear"`, `"<interval> <prompt>"`,
/// `"every <interval> <prompt>"`, `"<prompt> every <interval>"`, or `"<prompt>"`).
#[must_use]
pub fn parse_loop_command(args: &str) -> LoopCommand {
    let trimmed = args.trim();
    if trimmed.is_empty() {
        return LoopCommand::Autonomous;
    }
    if matches!(
        trimmed.to_ascii_lowercase().as_str(),
        "off" | "stop" | "clear"
    ) {
        return LoopCommand::Off;
    }

    let words: Vec<&str> = trimmed.split_whitespace().collect();
    // `/loop every <interval> <prompt...>`
    if words.len() >= 3
        && words[0].eq_ignore_ascii_case("every")
        && let Some(secs) = parse_interval_secs(words[1])
    {
        return LoopCommand::Fixed {
            interval_secs: secs.max(WAKEUP_MIN_SECS),
            prompt: words[2..].join(" "),
        };
    }
    // `/loop <interval> <prompt...>`
    if words.len() >= 2
        && let Some(secs) = parse_interval_secs(words[0])
    {
        return LoopCommand::Fixed {
            interval_secs: secs.max(WAKEUP_MIN_SECS),
            prompt: words[1..].join(" "),
        };
    }
    // `/loop <prompt...> every <interval>`
    if words.len() >= 3
        && words[words.len() - 2].eq_ignore_ascii_case("every")
        && let Some(secs) = parse_interval_secs(words[words.len() - 1])
    {
        return LoopCommand::Fixed {
            interval_secs: secs.max(WAKEUP_MIN_SECS),
            prompt: words[..words.len() - 2].join(" "),
        };
    }

    LoopCommand::Dynamic {
        prompt: trimmed.to_string(),
    }
}

/// Parse `/goal` arguments.
#[must_use]
pub fn parse_goal_command(args: &str) -> GoalCommand {
    let trimmed = args.trim();
    if trimmed.is_empty() {
        return GoalCommand::Help;
    }
    if matches!(
        trimmed.to_ascii_lowercase().as_str(),
        "off" | "stop" | "clear"
    ) {
        return GoalCommand::Off;
    }
    GoalCommand::Start(trimmed.to_string())
}

/// Parse an interval token like `30s`, `5m`, `2h`, `1d`.
#[must_use]
pub fn parse_interval_secs(token: &str) -> Option<u64> {
    let token = token.trim();
    let split = token.find(|c: char| !c.is_ascii_digit())?;
    let (num, unit) = token.split_at(split);
    let value: u64 = num.parse().ok()?;
    if value == 0 {
        return None;
    }
    let mult = match unit.to_ascii_lowercase().as_str() {
        "s" | "sec" | "secs" => 1,
        "m" | "min" | "mins" => 60,
        "h" | "hr" | "hrs" => 3600,
        "d" | "day" | "days" => 86_400,
        _ => return None,
    };
    value.checked_mul(mult)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_loop_variants_match_litemiyu_syntax() {
        assert_eq!(parse_loop_command(""), LoopCommand::Autonomous);
        assert_eq!(parse_loop_command("OFF"), LoopCommand::Off);
        assert_eq!(parse_loop_command("stop"), LoopCommand::Off);
        assert_eq!(
            parse_loop_command("10m check CI"),
            LoopCommand::Fixed {
                interval_secs: 600,
                prompt: "check CI".into(),
            }
        );
        assert_eq!(
            parse_loop_command("every 30s check release"),
            LoopCommand::Fixed {
                interval_secs: 60, // clamped to WAKEUP_MIN_SECS
                prompt: "check release".into(),
            }
        );
        assert_eq!(
            parse_loop_command("watch build every 2h"),
            LoopCommand::Fixed {
                interval_secs: 7200,
                prompt: "watch build".into(),
            }
        );
        assert_eq!(
            parse_loop_command("monitor deployment"),
            LoopCommand::Dynamic {
                prompt: "monitor deployment".into(),
            }
        );
    }

    #[test]
    fn parse_goal_variants() {
        assert_eq!(parse_goal_command(""), GoalCommand::Help);
        assert_eq!(parse_goal_command("off"), GoalCommand::Off);
        assert_eq!(
            parse_goal_command("make all tests green"),
            GoalCommand::Start("make all tests green".into())
        );
    }
}
