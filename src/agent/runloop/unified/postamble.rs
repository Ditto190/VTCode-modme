use std::borrow::Cow;
use std::io::{self, Write as _};
use std::time::Duration;
use vtcode_commons::ansi_codes::{BOLD, DIM, RESET, fg_256};
use vtcode_commons::color256_theme::rgb_to_ansi256_for_theme;
use vtcode_core::utils::ansi::{AnsiRenderer, MessageStyle};
use vtcode_ui::tui::ui::theme;

use crate::agent::runloop::unified::state::FirstCallComposition;

/// Zero-allocation exit data — all borrowed, no clones.
/// All token fields (`prompt_tokens`, `completion_tokens`, `cached_tokens`,
/// `cache_creation_tokens`, `cache_hit_rate_percent`) are sourced from the
/// same `session_stats.total_usage()` snapshot so they share one normalized
/// basis. Zero-valued fields are omitted from display.
pub(crate) struct ExitData<'a> {
    pub app_name: &'static str,
    pub version: &'static str,
    pub model: &'a str,
    pub provider: &'a str,
    pub trust_label: &'a str,
    pub reasoning: &'a str,
    pub session_duration: Duration,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub cached_tokens: u64,
    /// Cache-creation (cache-write) tokens accumulated this session.
    pub cache_creation_tokens: u64,
    /// Cache hit rate as a percentage (0-100), when at least one input token
    /// has been recorded this session.
    pub cache_hit_rate_percent: Option<f64>,
    pub code_additions: u64,
    pub code_deletions: u64,
    pub final_response: Option<&'a str>,
    pub resume_identifier: Option<&'a str>,
    pub budget_limit: Option<(f64, f64)>,
    /// Cache-aware estimated session spend when pricing is known. Shown even
    /// without a configured USD budget so cancelled/failed runs still report
    /// what they cost. `None` means unknown pricing, not free.
    pub total_cost_usd: Option<f64>,
    /// Terminal outcome label for the stats line (`completed` / `cancelled` /
    /// `error` / …). Always shown so a cancelled or errored run is labeled.
    pub end_reason_label: &'static str,
    /// First assembled request composition (harness-tax breakdown). When
    /// present, the stats line surfaces the per-call fixed overhead.
    pub first_call_composition: Option<FirstCallComposition>,
    /// How the session ended. Controls exit feedback:
    /// - `Completed` re-prints the final response so it survives in the main
    ///   scrollback after the alternate buffer is cleared.
    /// - `Exit`/`Cancelled`/`Error` suppress the full dump (it is already
    ///   visible in the TUI transcript) and print a concise notice instead,
    ///   avoiding fullscreen noise on Ctrl+C.
    pub session_end_reason: vtcode_core::hooks::SessionEndReason,
}

pub(crate) fn print_exit_summary(data: ExitData<'_>) {
    // Belt-and-braces before any stdout write: the canonical restore in
    // `finalize_session` already disabled raw mode, but if it was skipped or
    // failed (emergency path, partial init) output processing (`ONLCR`) stays
    // off and every `println!` staircases. Unlike `restore_tui()` this always
    // attempts the disable, so the postamble starts from a cooked tty.
    vtcode_ui::tui::panic_hook::ensure_raw_mode_disabled();
    // Arm the graceful-exit window before the first write so a late double
    // Ctrl+C during teardown cannot hard-exit between postamble writes.
    crate::agent::runloop::unified::session_setup::mark_exit_postamble_armed();
    // Completed sessions re-print the answer through the markdown renderer so
    // it survives in the main scrollback after the inline frame is torn down.
    if matches!(data.session_end_reason, vtcode_core::hooks::SessionEndReason::Completed)
        && let Some(response) = data.final_response.filter(|response| !response.trim().is_empty())
    {
        render_final_response(response);
    }
    // One buffered write for the notice + metrics block: the shell sees the
    // whole summary or nothing, never a truncated first line.
    write_postamble(&build_exit_postamble(&data));
}

/// Builds the exit postamble (interrupt notice + metrics block) as a single
/// string.
///
/// Every row carries a leading `\r` so the block starts at column 0 even when
/// output processing (`ONLCR`) is off, and the block opens on a fresh row so
/// it never overwrites the leftover inline TUI frame that the canonical
/// restore left behind.
fn build_exit_postamble(data: &ExitData<'_>) -> String {
    let mut out = String::with_capacity(640);
    // Fresh row below any leftover inline frame content.
    out.push_str("\r\n");

    if matches!(
        data.session_end_reason,
        vtcode_core::hooks::SessionEndReason::Exit | vtcode_core::hooks::SessionEndReason::Cancelled
    ) {
        // Interrupted exits get concise feedback, not a full transcript dump.
        // The in-TUI answer (if any) stays in the alternate buffer history;
        // re-printing it here is the fullscreen noise reported on Ctrl+C.
        out.push_str(&format!("\r{DIM}Interrupted — session exited. Transcript saved; resume to continue.{RESET}\n"));
    }

    out.push_str("\r\n");
    out.push_str(&format!(
        "\r{BOLD}{title_style}> {} ({}){RESET}\n",
        data.app_name,
        data.version,
        title_style = fg_256(rgb_to_ansi256_for_theme(0xAE, 0xA4, 0x7F, is_light_theme()))
    ));

    let trust = build_trust_label(data.trust_label);
    if !trust.is_empty() {
        out.push_str(&format!("\r{DIM}{trust}{RESET}\n"));
    }

    let model_line = build_model_line(data);
    if !model_line.is_empty() {
        out.push_str(&format!("\r{DIM}{model_line}{RESET}\n"));
    }

    out.push_str(&format!("\r{DIM}{}{RESET}\n", build_stats_line(data)));

    if let Some((max_budget_usd, actual_cost_usd)) = data.budget_limit {
        out.push_str(&format!("\r{DIM}Budget at ${actual_cost_usd:.2} / ${max_budget_usd:.2}{RESET}\n"));
    }

    if let Some(session_id) = data.resume_identifier {
        out.push_str(&format!(
            "\r{DIM}Resume: {resume_style}vtcode --resume {session_id}{RESET}\n",
            resume_style = fg_256(rgb_to_ansi256_for_theme(0x98, 0xBB, 0x74, is_light_theme()))
        ));
    }

    out.push_str("\r\n");
    out
}

fn is_light_theme() -> bool {
    theme::is_light_theme(&theme::active_theme_id())
}

/// Writes the postamble in one syscall so no concurrent exit path (emergency
/// double-Ctrl+C cleanup) can interleave between rows.
fn write_postamble(postamble: &str) {
    let mut stdout = io::stdout().lock();
    if let Err(error) = stdout.write_all(postamble.as_bytes()).and_then(|()| stdout.flush()) {
        tracing::warn!(%error, "failed to write the exit postamble");
    }
}

fn render_final_response(response: &str) {
    let mut renderer = AnsiRenderer::stdout();
    if let Err(error) = renderer.line(MessageStyle::Response, response) {
        tracing::warn!(%error, "failed to render final response during session exit");
    }
}

/// Builds the pipe-delimited exit stats line (session duration, token
/// counts, cache read/creation/hit-rate, code diff) with no ANSI styling, so
/// it stays independently testable. `print_exit_summary` wraps the result in
/// the dim/reset styling used for the rest of the exit summary.
///
/// Takes `&ExitData` rather than individual fields since every value it
/// needs already lives on that struct — a single reference avoids an
/// eight-parameter call.
fn build_stats_line(data: &ExitData<'_>) -> String {
    let mut stats = Vec::new();
    stats.push(format!("Session {}", format_duration(data.session_duration)));

    if data.prompt_tokens > 0 || data.completion_tokens > 0 {
        stats
            .push(format!("{} in / {} out", format_number(data.prompt_tokens), format_number(data.completion_tokens),));
    }

    if data.cached_tokens > 0 {
        let mut cache_stat = format!("Cache {} read", format_number(data.cached_tokens));
        if let Some(hit_rate) = data.cache_hit_rate_percent {
            cache_stat.push_str(&format!(" ({hit_rate:.1}% hit rate)"));
        }
        if data.cache_creation_tokens > 0 {
            cache_stat.push_str(&format!(", {} creation", format_number(data.cache_creation_tokens)));
        }
        stats.push(cache_stat);
    }

    if data.code_additions > 0 || data.code_deletions > 0 {
        stats.push(format!("Code +{} / -{}", data.code_additions, data.code_deletions));
    }

    // Always surface estimated spend when pricing is known, including
    // cancelled runs — a 17M-token interrupt must not look free.
    if let Some(cost) = data.total_cost_usd {
        stats.push(format!("Cost ${cost:.2}"));
    }

    if !data.end_reason_label.is_empty() {
        stats.push(format!("End {}", data.end_reason_label));
    }

    if let Some(overhead) = data.first_call_composition {
        let fixed = overhead.fixed_overhead_tokens();
        if fixed > 0 {
            stats.push(format!(
                "First-call overhead {} (system {} + tools {})",
                format_number(fixed as u64),
                format_number(overhead.system_prompt_tokens as u64),
                format_number(overhead.tool_schema_tokens as u64),
            ));
        }
    }

    stats.join(" | ")
}

/// Builds the model/provider/reasoning row (unstyled ends are supplied by the
/// caller's DIM wrapper). Empty when neither model nor provider is known.
fn build_model_line(data: &ExitData<'_>) -> String {
    let model = data.model.trim();
    let provider = data.provider.trim();
    let reasoning = data.reasoning.trim();

    let show_model = !model.is_empty();
    let show_provider = !provider.is_empty();
    let show_reasoning = !reasoning.is_empty();

    let model_style = fg_256(rgb_to_ansi256_for_theme(0xCC, 0x8A, 0x3E, is_light_theme()));

    let mut line = match (show_model, show_provider) {
        (true, true) => format!("Model: {BOLD}{model_style}{model}{RESET}{DIM} via {provider}"),
        (true, false) => format!("Model: {BOLD}{model_style}{model}{RESET}"),
        (false, true) => format!("Provider: {provider}"),
        (false, false) => String::new(),
    };

    if show_reasoning {
        let suffix = format!(" · {reasoning}");
        if line.is_empty() {
            line = format!("Reasoning:{suffix}");
        } else {
            line.push_str(&suffix);
        }
    }

    line
}

/// Returns a borrowed trust label — empty string if unknown, no allocation.
fn build_trust_label(trust_label: &str) -> Cow<'static, str> {
    let t = trust_label.trim().to_ascii_lowercase().replace('_', " ");
    if t.contains("full auto") {
        Cow::Borrowed("Full-auto trust")
    } else if t.contains("tools policy") {
        Cow::Borrowed("Safe tools")
    } else if t.is_empty() || t == "unknown" {
        Cow::Borrowed("")
    } else {
        Cow::Owned(format!("Trust: {t}"))
    }
}

fn format_duration(d: Duration) -> String {
    let s = d.as_secs();
    let h = s / 3600;
    let m = (s % 3600) / 60;
    let sec = s % 60;
    if h > 0 {
        format!("{h}h {m}m {sec}s")
    } else if m > 0 {
        format!("{m}m {sec}s")
    } else {
        format!("{sec}s")
    }
}

fn format_number(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}m", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trust_line_full_auto() {
        assert_eq!(build_trust_label("full auto").as_ref(), "Full-auto trust");
        assert_eq!(build_trust_label("full_auto").as_ref(), "Full-auto trust");
    }

    #[test]
    fn trust_line_safe_tools() {
        assert_eq!(build_trust_label("tools policy").as_ref(), "Safe tools");
        assert_eq!(build_trust_label("tools_policy").as_ref(), "Safe tools");
    }

    #[test]
    fn trust_line_empty_or_unknown() {
        assert_eq!(build_trust_label("").as_ref(), "");
        assert_eq!(build_trust_label("unknown").as_ref(), "");
    }

    #[test]
    fn trust_line_fallback() {
        assert_eq!(build_trust_label("some_other").as_ref(), "Trust: some other");
    }

    #[test]
    fn formats_duration() {
        assert_eq!(format_duration(Duration::from_secs(55)), "55s");
        assert_eq!(format_duration(Duration::from_secs(95)), "1m 35s");
        assert_eq!(format_duration(Duration::from_secs(3670)), "1h 1m 10s");
    }

    #[test]
    fn formats_numbers() {
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(12_345), "12.3k");
        assert_eq!(format_number(8_900_000), "8.9m");
    }

    /// Builds an `ExitData` with only the stats-line-relevant fields set;
    /// the rest are dummy values `build_stats_line` never reads.
    fn stats_test_data(
        session_duration: Duration,
        prompt_tokens: u64,
        completion_tokens: u64,
        cached_tokens: u64,
        cache_creation_tokens: u64,
        cache_hit_rate_percent: Option<f64>,
        code_additions: u64,
        code_deletions: u64,
    ) -> ExitData<'static> {
        ExitData {
            app_name: "VT Code",
            version: "0.0.0",
            model: "",
            provider: "",
            trust_label: "",
            reasoning: "",
            session_duration,
            prompt_tokens,
            completion_tokens,
            cached_tokens,
            cache_creation_tokens,
            cache_hit_rate_percent,
            code_additions,
            code_deletions,
            final_response: None,
            resume_identifier: None,
            budget_limit: None,
            total_cost_usd: None,
            end_reason_label: "",
            first_call_composition: None,
            session_end_reason: vtcode_core::hooks::SessionEndReason::Completed,
        }
    }

    #[test]
    fn stats_line_with_zero_input_omits_token_and_cache_segments() {
        let data = stats_test_data(Duration::from_secs(30), 0, 0, 0, 0, None, 0, 0);
        let line = build_stats_line(&data);
        assert_eq!(line, "Session 30s");
    }

    #[test]
    fn stats_line_includes_first_call_overhead_when_present() {
        let data = ExitData {
            first_call_composition: Some(FirstCallComposition {
                system_prompt_tokens: 1_100,
                tool_schema_tokens: 2_100,
                message_history_tokens: 40,
                on_wire_tools: 4,
            }),
            ..stats_test_data(Duration::from_secs(30), 0, 0, 0, 0, None, 0, 0)
        };
        let line = build_stats_line(&data);
        assert!(
            line.contains("First-call overhead 3.2k (system 1.1k + tools 2.1k)"),
            "stats line missing first-call overhead: {line}"
        );
    }

    #[test]
    fn stats_line_omits_zero_first_call_overhead() {
        let data = ExitData {
            first_call_composition: Some(FirstCallComposition {
                system_prompt_tokens: 0,
                tool_schema_tokens: 0,
                message_history_tokens: 0,
                on_wire_tools: 0,
            }),
            ..stats_test_data(Duration::from_secs(30), 0, 0, 0, 0, None, 0, 0)
        };
        let line = build_stats_line(&data);
        assert_eq!(line, "Session 30s");
    }

    #[test]
    fn stats_line_includes_cost_when_pricing_is_known() {
        let data = ExitData {
            total_cost_usd: Some(0.42),
            end_reason_label: "cancelled",
            ..stats_test_data(Duration::from_secs(30), 1_000, 100, 0, 0, None, 0, 0)
        };
        let line = build_stats_line(&data);
        assert!(line.contains("Cost $0.42"), "missing cost: {line}");
        assert!(line.contains("End cancelled"), "missing end reason: {line}");
    }

    #[test]
    fn stats_line_omits_cost_when_pricing_is_unknown() {
        // Unknown pricing must not render as free / $0.
        let data = stats_test_data(Duration::from_secs(30), 1_000, 100, 0, 0, None, 0, 0);
        let line = build_stats_line(&data);
        assert!(!line.contains("Cost"), "unknown pricing must not show cost: {line}");
    }

    #[test]
    fn stats_line_with_cache_includes_hit_rate_and_creation_tokens() {
        let data = stats_test_data(Duration::from_secs(95), 1_000, 200, 800, 50, Some(80.0), 10, 2);
        let line = build_stats_line(&data);
        assert_eq!(
            line,
            "Session 1m 35s | 1.0k in / 200 out | Cache 800 read (80.0% hit rate), 50 creation | Code +10 / -2"
        );
    }

    #[test]
    fn stats_line_with_cache_but_no_creation_omits_creation_segment() {
        let data = stats_test_data(Duration::from_secs(10), 500, 100, 400, 0, Some(80.0), 0, 0);
        let line = build_stats_line(&data);
        assert_eq!(line, "Session 10s | 500 in / 100 out | Cache 400 read (80.0% hit rate)");
    }

    #[test]
    fn final_response_is_rendered_before_exit_metrics() {
        // Completed sessions re-print the answer through the markdown renderer
        // (stdout writes), so only the block shape is assertable here: the
        // metrics rows always follow the response branch.
        let data = ExitData {
            final_response: Some("final response"),
            session_end_reason: vtcode_core::hooks::SessionEndReason::Completed,
            ..stats_test_data(Duration::from_secs(30), 0, 0, 0, 0, None, 0, 0)
        };
        let postamble = build_exit_postamble(&data);

        assert!(postamble.contains("> VT Code (0.0.0)"), "title row missing: {postamble:?}");
        assert!(postamble.contains("Session 30s"), "stats row missing: {postamble:?}");
        assert!(
            !postamble.contains("Interrupted — session exited"),
            "completed exits must not carry the interrupt notice: {postamble:?}"
        );
    }

    #[test]
    fn interrupt_postamble_keeps_full_summary_block() {
        // Regression: a late double Ctrl+C during teardown used to hard-exit
        // between postamble writes, leaving only the notice on screen.
        let data = ExitData {
            final_response: Some("long transcript body that must not leak"),
            session_end_reason: vtcode_core::hooks::SessionEndReason::Exit,
            resume_identifier: Some("session-test-1234"),
            ..stats_test_data(Duration::from_secs(95), 1_000, 200, 800, 50, Some(80.0), 10, 2)
        };
        let postamble = build_exit_postamble(&data);
        let rows: Vec<&str> = postamble.split('\n').collect();

        let notice_row = rows
            .iter()
            .position(|row| row.contains("Interrupted — session exited"))
            .expect("interrupt notice row");
        let title_row = rows.iter().position(|row| row.contains("> VT Code")).expect("title row");
        let stats_row = rows.iter().position(|row| row.contains("Session 1m 35s")).expect("stats row");
        let resume_row = rows
            .iter()
            .position(|row| row.contains("vtcode --resume session-test-1234"))
            .expect("resume id row");

        assert!(notice_row < title_row, "notice must precede the block: {postamble:?}");
        assert!(title_row < stats_row, "block order broken: {postamble:?}");
        assert!(stats_row < resume_row, "resume id must follow the stats row: {postamble:?}");
        assert!(
            !postamble.contains("long transcript body"),
            "interrupt exits must suppress the final response dump: {postamble:?}"
        );
    }

    #[test]
    fn postamble_rows_are_carriage_prefixed_and_open_on_a_fresh_row() {
        let data = ExitData {
            session_end_reason: vtcode_core::hooks::SessionEndReason::Exit,
            ..stats_test_data(Duration::from_secs(30), 0, 0, 0, 0, None, 0, 0)
        };
        let postamble = build_exit_postamble(&data);

        assert!(postamble.starts_with("\r\n"), "postamble must open on a fresh row: {postamble:?}");
        for row in postamble.split('\n').filter(|row| !row.is_empty()) {
            assert!(row.starts_with('\r'), "every postamble row must return the carriage: {row:?}");
        }
    }

    #[test]
    fn exit_suppresses_final_response_dump() {
        // Ctrl+C exits must not re-print the full TUI transcript: that is the
        // fullscreen noise reported on control+c handling.
        for reason in [
            vtcode_core::hooks::SessionEndReason::Exit,
            vtcode_core::hooks::SessionEndReason::Cancelled,
            vtcode_core::hooks::SessionEndReason::Error,
        ] {
            let data = ExitData {
                final_response: Some("long transcript body that must not leak"),
                session_end_reason: reason,
                ..stats_test_data(Duration::from_secs(30), 0, 0, 0, 0, None, 0, 0)
            };
            let postamble = build_exit_postamble(&data);

            assert!(
                !postamble.contains("long transcript body"),
                "exit reason {reason:?} must suppress final response dump: {postamble:?}"
            );
            assert!(postamble.contains("Session 30s"), "exit metrics must still render for {reason:?}: {postamble:?}");
        }
    }
}
