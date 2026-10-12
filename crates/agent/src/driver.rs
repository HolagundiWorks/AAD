//! Connects the [`Assistant`] state machine to a language model, for hosts that can simply wait
//! (the command-line `ask`). The desktop app does not use [`ask`]: it runs the same steps but sends
//! each generation to a background worker (`cadcraft_llm::worker`), using [`to_messages`] and
//! [`params_for`] from here.

use std::sync::atomic::{AtomicBool, Ordering};

use cadcraft_engine::Session;
use cadcraft_llm::{LlmError, Message, Model, Params, Role as LlmRole};

use crate::GenerationProfile;
use crate::assistant::{Assistant, Msg, Role, Step};
use crate::registry::Risk;

/// The conversation in the model's message type.
pub fn to_messages(msgs: &[Msg]) -> Vec<Message> {
    msgs.iter()
        .map(|m| {
            let role = match m.role {
                Role::System => LlmRole::System,
                Role::User => LlmRole::User,
                Role::Assistant => LlmRole::Assistant,
                Role::Tool => LlmRole::Tool,
            };
            Message::new(role, m.content.clone())
        })
        .collect()
}

/// Generation settings for one step.
pub fn params_for(profile: &GenerationProfile, max_new_tokens: u32) -> Params {
    Params {
        n_ctx: profile.n_ctx,
        max_new_tokens,
        temperature: profile.temperature,
        top_p: profile.top_p,
        top_k: i32::try_from(profile.top_k).unwrap_or(20),
        min_p: profile.min_p,
        enable_thinking: profile.enable_thinking,
        ..Params::default()
    }
}

/// What is being asked of the user before a tool runs.
pub struct Confirmation<'a> {
    pub tool: &'a str,
    pub risk: Risk,
    pub summary: &'a str,
}

/// Run one request to the end: generate, run the tool the model chose (asking `confirm` first when
/// the policy says so), generate again with the result, until there is an answer.
pub fn ask(
    model: &Model,
    assistant: &mut Assistant,
    session: &mut Session,
    text: &str,
    confirm: &mut dyn FnMut(&Confirmation) -> bool,
    cancel: &AtomicBool,
    on_piece: &mut dyn FnMut(&str),
) -> Result<String, String> {
    let mut step = assistant.start(text);
    loop {
        if cancel.load(Ordering::Relaxed) {
            assistant.cancel();
            return Err("stopped".into());
        }
        step = match step {
            Step::Generate { messages, max_new_tokens } => {
                let params = params_for(&assistant.profile, max_new_tokens);
                let done = model.chat(&to_messages(&messages), &params, cancel, on_piece).map_err(|e: LlmError| e.to_string())?;
                assistant.on_generated(session, &done.text)
            }
            Step::NeedsConfirmation { tool, risk, summary } => {
                let yes = confirm(&Confirmation { tool: &tool, risk, summary: &summary });
                assistant.confirm(session, yes)
            }
            Step::Answer(a) => return Ok(a),
            Step::Failed(e) => return Err(e),
        };
    }
}
