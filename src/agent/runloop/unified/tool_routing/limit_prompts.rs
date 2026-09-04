use std::sync::Arc;

use anyhow::Result;
use tokio::sync::Notify;
use vtcode_core::core::interfaces::ui::UiSession;
use vtcode_ui::tui::app::{InlineHandle, ListOverlayRequest, TransientRequest, TransientSubmission};

use crate::agent::runloop::unified::overlay_prompt::{OverlayWaitOutcome, show_overlay_and_wait};
use crate::agent::runloop::unified::state::CtrlCState;

pub(super) async fn prompt_session_limit_increase<S: UiSession + ?Sized>(
    handle: &InlineHandle,
    session: &mut S,
    ctrl_c_state: &Arc<CtrlCState>,
    ctrl_c_notify: &Arc<Notify>,
    max_limit: usize,
    agent_name: Option<&str>,
) -> Result<Option<usize>> {
    use vtcode_ui::tui::app::{InlineListItem, InlineListSelection};

    let description_lines = vec![
        format!("Session tool limit reached: {}", max_limit),
        format!("Current agent: {}", agent_name.unwrap_or("unknown")),
        "Grant an increase to retry the pending tool call in this turn.".to_string(),
        "Deny stops the call; reuse the outputs already gathered for the next response.".to_string(),
        "".to_string(),
        "Use ↑↓ or Tab to navigate • Enter to select • Esc to deny".to_string(),
    ];

    let options = vec![
        InlineListItem {
            title: "+100 tool calls".to_string(),
            subtitle: Some("Increase the session limit by 100".to_string()),
            badge: None,
            indent: 0,
            selection: Some(InlineListSelection::SessionLimitIncrease(100)),
            search_value: Some("increase 100 hundred plus more".to_string()),
        },
        InlineListItem {
            title: "+50 tool calls".to_string(),
            subtitle: Some("Increase the session limit by 50".to_string()),
            badge: None,
            indent: 0,
            selection: Some(InlineListSelection::SessionLimitIncrease(50)),
            search_value: Some("increase 50 fifty plus more".to_string()),
        },
        InlineListItem {
            title: "".to_string(),
            subtitle: None,
            badge: None,
            indent: 0,
            selection: None,
            search_value: None,
        },
        InlineListItem {
            title: "Deny".to_string(),
            subtitle: Some("Do not increase limit (stops tool execution)".to_string()),
            badge: None,
            indent: 0,
            selection: Some(InlineListSelection::ToolApproval(false)),
            search_value: Some("deny no exit stop cancel".to_string()),
        },
    ];

    prompt_limit_increase_modal(
        handle,
        session,
        ctrl_c_state,
        ctrl_c_notify,
        "Session Limit Reached".to_string(),
        description_lines,
        options,
        100,
    )
    .await
}

pub(super) async fn prompt_tool_loop_limit_increase<S: UiSession + ?Sized>(
    handle: &InlineHandle,
    session: &mut S,
    ctrl_c_state: &Arc<CtrlCState>,
    ctrl_c_notify: &Arc<Notify>,
    max_limit: usize,
    agent_name: Option<&str>,
) -> Result<Option<usize>> {
    use vtcode_ui::tui::app::{InlineListItem, InlineListSelection};

    let description_lines = vec![
        format!("Maximum tool loops reached: {}", max_limit),
        format!("Current agent: {}", agent_name.unwrap_or("unknown")),
        "Grant more loops to continue this turn with the current agent.".to_string(),
        "Stop synthesizes from the outputs already gathered.".to_string(),
        "".to_string(),
        "Use ↑↓ or Tab to navigate • Enter to select • Esc to stop".to_string(),
    ];

    let options = vec![
        InlineListItem {
            title: "+50 tool loops".to_string(),
            subtitle: Some("Continue with 50 more tool loops".to_string()),
            badge: None,
            indent: 0,
            selection: Some(InlineListSelection::SessionLimitIncrease(50)),
            search_value: Some("increase 50 fifty plus more continue".to_string()),
        },
        InlineListItem {
            title: "+20 tool loops".to_string(),
            subtitle: Some("Continue with 20 more tool loops".to_string()),
            badge: None,
            indent: 0,
            selection: Some(InlineListSelection::SessionLimitIncrease(20)),
            search_value: Some("increase 20 twenty plus more continue".to_string()),
        },
        InlineListItem {
            title: "+10 tool loops".to_string(),
            subtitle: Some("Continue with 10 more tool loops".to_string()),
            badge: None,
            indent: 0,
            selection: Some(InlineListSelection::SessionLimitIncrease(10)),
            search_value: Some("increase 10 ten plus more continue".to_string()),
        },
        InlineListItem {
            title: "".to_string(),
            subtitle: None,
            badge: None,
            indent: 0,
            selection: None,
            search_value: None,
        },
        InlineListItem {
            title: "Stop".to_string(),
            subtitle: Some("Stop the current turn and wait for input".to_string()),
            badge: None,
            indent: 0,
            selection: Some(InlineListSelection::ToolApproval(false)),
            search_value: Some("stop no exit cancel done".to_string()),
        },
    ];

    prompt_limit_increase_modal(
        handle,
        session,
        ctrl_c_state,
        ctrl_c_notify,
        "Tool Loop Limit Reached".to_string(),
        description_lines,
        options,
        20,
    )
    .await
}

async fn prompt_limit_increase_modal<S: UiSession + ?Sized>(
    handle: &InlineHandle,
    session: &mut S,
    ctrl_c_state: &Arc<CtrlCState>,
    ctrl_c_notify: &Arc<Notify>,
    title: String,
    description_lines: Vec<String>,
    options: Vec<vtcode_ui::tui::app::InlineListItem>,
    default_increment: usize,
) -> Result<Option<usize>> {
    use vtcode_ui::tui::app::InlineListSelection;

    // A bridge submission is deferred while the modal owns the input surface.
    // Re-show this limit prompt and continue waiting so that a transient
    // bridge event cannot accidentally become a denial of the grant.
    loop {
        let outcome = show_overlay_and_wait(
            handle,
            session,
            TransientRequest::List(ListOverlayRequest {
                title: title.clone(),
                lines: description_lines.clone(),
                footer_hint: None,
                items: options.clone(),
                selected: Some(InlineListSelection::SessionLimitIncrease(default_increment)),
                search: None,
                hotkeys: Vec::new(),
            }),
            ctrl_c_state,
            ctrl_c_notify,
            |submission| match submission {
                TransientSubmission::Selection(InlineListSelection::SessionLimitIncrease(inc)) => Some(inc),
                // All grant options are strictly positive; zero is the
                // explicit Deny/Stop selection sentinel.
                TransientSubmission::Selection(InlineListSelection::ToolApproval(false)) => Some(0),
                TransientSubmission::Selection(_) => None,
                _ => None,
            },
        )
        .await?;

        match outcome {
            OverlayWaitOutcome::Submitted(0) => return Ok(None),
            OverlayWaitOutcome::Submitted(increment) => return Ok(Some(increment)),
            // Esc/Cancel is the user's explicit denial. Interrupt and Exit
            // remain distinct control-flow outcomes but also deny the grant.
            OverlayWaitOutcome::Cancelled | OverlayWaitOutcome::Interrupted | OverlayWaitOutcome::Exit => {
                return Ok(None);
            }
            OverlayWaitOutcome::Deferred => continue,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::VecDeque;
    use tokio::sync::{Notify, mpsc};
    use vtcode_ui::tui::app::{InlineCommand, InlineEvent, InlineListSelection, TransientEvent, TransientSubmission};

    struct TestSession {
        handle: InlineHandle,
        events: VecDeque<InlineEvent>,
    }

    #[async_trait]
    impl UiSession for TestSession {
        fn inline_handle(&self) -> &InlineHandle {
            &self.handle
        }

        async fn next_event(&mut self) -> Option<InlineEvent> {
            self.events.pop_front()
        }
    }

    #[tokio::test]
    async fn deferred_bridge_event_reshows_limit_prompt_until_grant() {
        let (command_sender, mut command_receiver) = mpsc::unbounded_channel();
        let handle = InlineHandle::new_for_tests(command_sender);
        let mut session = TestSession {
            handle: handle.clone(),
            events: VecDeque::from([
                // A leaked mode-switch event belongs to the active turn and
                // must not dismiss the prompt.
                InlineEvent::CyclePrimaryAgent,
                InlineEvent::WebmcpSubmit("continue".into()),
                InlineEvent::Transient(TransientEvent::Submitted(TransientSubmission::Selection(
                    InlineListSelection::SessionLimitIncrease(50),
                ))),
            ]),
        };
        let ctrl_c_state = Arc::new(CtrlCState::new());
        let ctrl_c_notify = Arc::new(Notify::new());

        let result =
            prompt_session_limit_increase(&handle, &mut session, &ctrl_c_state, &ctrl_c_notify, 100, Some("build"))
                .await
                .expect("limit prompt should remain available after deferred input");

        assert_eq!(result, Some(50));
        let mut shown = 0;
        while let Ok(command) = command_receiver.try_recv() {
            if matches!(command, InlineCommand::ShowTransient { .. }) {
                shown += 1;
            }
        }
        assert_eq!(shown, 2, "the deferred bridge input should cause a re-show");
    }

    #[tokio::test]
    async fn cancel_is_an_explicit_denial() {
        let (command_sender, _command_receiver) = mpsc::unbounded_channel();
        let handle = InlineHandle::new_for_tests(command_sender);
        let mut session = TestSession {
            handle: handle.clone(),
            events: VecDeque::from([InlineEvent::Cancel]),
        };
        let ctrl_c_state = Arc::new(CtrlCState::new());
        let ctrl_c_notify = Arc::new(Notify::new());

        let result =
            prompt_session_limit_increase(&handle, &mut session, &ctrl_c_state, &ctrl_c_notify, 100, Some("build"))
                .await
                .expect("cancel should be handled as a denial");

        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn deny_selection_is_an_explicit_denial() {
        let (command_sender, _command_receiver) = mpsc::unbounded_channel();
        let handle = InlineHandle::new_for_tests(command_sender);
        let mut session = TestSession {
            handle: handle.clone(),
            events: VecDeque::from([InlineEvent::Transient(TransientEvent::Submitted(
                TransientSubmission::Selection(InlineListSelection::ToolApproval(false)),
            ))]),
        };
        let ctrl_c_state = Arc::new(CtrlCState::new());
        let ctrl_c_notify = Arc::new(Notify::new());

        let result =
            prompt_session_limit_increase(&handle, &mut session, &ctrl_c_state, &ctrl_c_notify, 100, Some("build"))
                .await
                .expect("deny selection should be handled as a denial");

        assert_eq!(result, None);
    }
}
