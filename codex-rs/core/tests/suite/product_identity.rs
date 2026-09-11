use anyhow::Result;
use codex_core::TurnInputRequest;
use codex_features::Feature;
use codex_protocol::config_types::Personality;
use codex_protocol::protocol::EventMsg;
use codex_protocol::user_input::UserInput;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_sse_once;
use core_test_support::responses::sse;
use core_test_support::responses::start_mock_server;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex::test_codex;
use core_test_support::wait_for_event;
use pretty_assertions::assert_eq;
use std::fs;

const MOEDEX_IDENTITY: &str =
    "You are operating in Moedex, an agentic coding interface based on the Codex CLI.";
const FRIENDLY_PERSONALITY_MARKER: &str = "You have a vivid inner life as Codex:";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn moedex_identity_is_added_once_without_rewriting_resumed_history() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_codex().with_config(|config| {
        config
            .features
            .enable(Feature::Personality)
            .expect("enable personality");
        config.personality = Some(Personality::Friendly);
    });
    let initial = builder.build_with_auto_env(&server).await?;
    let initial_mock = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-initial"),
            ev_assistant_message("msg-initial", "historical assistant bytes"),
            ev_completed("resp-initial"),
        ]),
    )
    .await;

    initial
        .codex
        .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
            text: "historical user bytes".into(),
            text_elements: Vec::new(),
        }]))
        .await?;
    wait_for_event(&initial.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;

    let initial_request = initial_mock.single_request();
    let initial_visible_context = format!(
        "{}\n{}",
        initial_request.instructions_text(),
        serde_json::to_string(&initial_request.input())?
    );
    assert_eq!(initial_visible_context.matches(MOEDEX_IDENTITY).count(), 1);
    assert_eq!(
        initial_visible_context
            .matches(FRIENDLY_PERSONALITY_MARKER)
            .count(),
        1
    );
    assert_eq!(
        initial_visible_context
            .matches("<personality_spec>")
            .count(),
        0
    );

    let rollout_path = initial.codex.rollout_path().expect("rollout path");
    let historical_rollout = fs::read(&rollout_path)?;
    let resumed_mock = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-resumed"),
            ev_assistant_message("msg-resumed", "resumed assistant bytes"),
            ev_completed("resp-resumed"),
        ]),
    )
    .await;
    let resumed = builder.restart(&server, &initial).await?;

    let rollout_after_resume = fs::read(&rollout_path)?;
    assert!(rollout_after_resume.starts_with(&historical_rollout));

    resumed
        .codex
        .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
            text: "new user bytes".into(),
            text_elements: Vec::new(),
        }]))
        .await?;
    wait_for_event(&resumed.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;

    let resumed_request = resumed_mock.single_request();
    let resumed_visible_context = format!(
        "{}\n{}",
        resumed_request.instructions_text(),
        serde_json::to_string(&resumed_request.input())?
    );
    assert_eq!(resumed_visible_context.matches(MOEDEX_IDENTITY).count(), 1);
    assert_eq!(
        resumed_visible_context
            .matches(FRIENDLY_PERSONALITY_MARKER)
            .count(),
        1
    );
    assert_eq!(
        resumed_visible_context
            .matches("<personality_spec>")
            .count(),
        1
    );
    assert!(resumed_visible_context.contains("historical user bytes"));
    assert!(resumed_visible_context.contains("historical assistant bytes"));

    Ok(())
}
