use crate::context::CollaborationModeInstructions;
use crate::context::ContextualUserFragment;
use crate::context::EnvironmentContext;
use crate::context::ModelSwitchInstructions;
use crate::context::PermissionsInstructions;
use crate::context::PersonalitySpecInstructions;
use crate::context::RealtimeEndInstructions;
use crate::context::RealtimeStartInstructions;
use crate::context::RealtimeStartWithInstructions;
use crate::session::PreviousTurnSettings;
use crate::session::turn_context::TurnContext;
use crate::shell::Shell;
use codex_execpolicy::Policy;
use codex_features::Feature;
use codex_journal::Journal;
use codex_journal::JournalContextItem;
use codex_journal::JournalContextKey;
use codex_journal::JournalEntry;
use codex_journal::JournalItem;
use codex_journal::PromptMessage;
use codex_journal::PromptMessageRole;
use codex_protocol::config_types::Personality;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::openai_models::ModelInfo;
use codex_protocol::protocol::TurnContextItem;

const PROMPT_BUNDLE_KEY_PREFIX: &str = "prompt";
const DEVELOPER_BUNDLE: &str = "developer";
const USAGE_HINT_BUNDLE: &str = "usage_hint";
const CONTEXTUAL_USER_BUNDLE: &str = "contextual_user";
const GUARDIAN_BUNDLE: &str = "guardian";

fn build_environment_update_item(
    previous: Option<&TurnContextItem>,
    next: &TurnContext,
    shell: &Shell,
) -> Option<String> {
    if !next.config.include_environment_context {
        return None;
    }

    let prev = previous?;
    let prev_context = EnvironmentContext::from_turn_context_item(prev, shell.name().to_string());
    let next_context = EnvironmentContext::from_turn_context(next, shell);
    if prev_context.equals_except_shell(&next_context) {
        return None;
    }

    Some(EnvironmentContext::diff_from_turn_context_item(prev, &next_context).render())
}

fn build_permissions_update_item(
    previous: Option<&TurnContextItem>,
    next: &TurnContext,
    exec_policy: &Policy,
) -> Option<String> {
    if !next.config.include_permissions_instructions {
        return None;
    }

    let prev = previous?;
    if prev.permission_profile() == next.permission_profile()
        && prev.approval_policy == next.approval_policy.value()
    {
        return None;
    }

    Some(
        PermissionsInstructions::from_permission_profile(
            &next.permission_profile,
            next.approval_policy.value(),
            next.config.approvals_reviewer,
            exec_policy,
            &next.cwd,
            next.features.enabled(Feature::ExecPermissionApprovals),
            next.features.enabled(Feature::RequestPermissionsTool),
        )
        .render(),
    )
}

fn build_collaboration_mode_update_item(
    previous: Option<&TurnContextItem>,
    next: &TurnContext,
) -> Option<String> {
    let prev = previous?;
    if prev.collaboration_mode.as_ref() != Some(&next.collaboration_mode) {
        // If the next mode has empty developer instructions, this returns None and we emit no
        // update, so prior collaboration instructions remain in the prompt history.
        Some(
            CollaborationModeInstructions::from_collaboration_mode(&next.collaboration_mode)?
                .render(),
        )
    } else {
        None
    }
}

pub(crate) fn build_realtime_update_item(
    previous: Option<&TurnContextItem>,
    previous_turn_settings: Option<&PreviousTurnSettings>,
    next: &TurnContext,
) -> Option<String> {
    match (
        previous.and_then(|item| item.realtime_active),
        next.realtime_active,
    ) {
        (Some(true), false) => Some(RealtimeEndInstructions::new("inactive").render()),
        (Some(false), true) | (None, true) => Some(
            if let Some(instructions) = next
                .config
                .experimental_realtime_start_instructions
                .as_deref()
            {
                RealtimeStartWithInstructions::new(instructions).render()
            } else {
                RealtimeStartInstructions.render()
            },
        ),
        (Some(true), true) | (Some(false), false) => None,
        (None, false) => previous_turn_settings
            .and_then(|settings| settings.realtime_active)
            .filter(|realtime_active| *realtime_active)
            .map(|_| RealtimeEndInstructions::new("inactive").render()),
    }
}

pub(crate) fn build_initial_realtime_item(
    previous: Option<&TurnContextItem>,
    previous_turn_settings: Option<&PreviousTurnSettings>,
    next: &TurnContext,
) -> Option<String> {
    build_realtime_update_item(previous, previous_turn_settings, next)
}

fn build_personality_update_item(
    previous: Option<&TurnContextItem>,
    next: &TurnContext,
    personality_feature_enabled: bool,
) -> Option<String> {
    if !personality_feature_enabled {
        return None;
    }
    let previous = previous?;
    if next.model_info.slug != previous.model {
        return None;
    }

    if let Some(personality) = next.personality
        && next.personality != previous.personality
    {
        let model_info = &next.model_info;
        let personality_message = personality_message_for(model_info, personality);
        personality_message.map(|message| PersonalitySpecInstructions::new(message).render())
    } else {
        None
    }
}

pub(crate) fn personality_message_for(
    model_info: &ModelInfo,
    personality: Personality,
) -> Option<String> {
    model_info
        .model_messages
        .as_ref()
        .and_then(|spec| spec.get_personality_message(Some(personality)))
        .filter(|message| !message.is_empty())
}

pub(crate) fn build_model_instructions_update_item(
    previous_turn_settings: Option<&PreviousTurnSettings>,
    next: &TurnContext,
) -> Option<String> {
    let previous_turn_settings = previous_turn_settings?;
    if previous_turn_settings.model == next.model_info.slug {
        return None;
    }

    let model_instructions = next.model_info.get_model_instructions(next.personality);
    if model_instructions.is_empty() {
        return None;
    }

    Some(ModelSwitchInstructions::new(model_instructions).render())
}

pub(crate) fn developer_context_entry(
    name: &str,
    prompt_order: i64,
    text: String,
) -> Option<JournalEntry> {
    context_entry(
        DEVELOPER_BUNDLE,
        name,
        prompt_order,
        PromptMessage::developer_text(text),
    )
}

pub(crate) fn usage_hint_context_entry(
    name: &str,
    prompt_order: i64,
    text: String,
) -> Option<JournalEntry> {
    context_entry(
        USAGE_HINT_BUNDLE,
        name,
        prompt_order,
        PromptMessage::developer_text(text),
    )
}

pub(crate) fn contextual_user_context_entry(
    name: &str,
    prompt_order: i64,
    text: String,
) -> Option<JournalEntry> {
    context_entry(
        CONTEXTUAL_USER_BUNDLE,
        name,
        prompt_order,
        PromptMessage::user_text(text),
    )
}

pub(crate) fn guardian_context_entry(
    name: &str,
    prompt_order: i64,
    text: String,
) -> Option<JournalEntry> {
    context_entry(
        GUARDIAN_BUNDLE,
        name,
        prompt_order,
        PromptMessage::developer_text(text),
    )
}

fn context_entry(
    bundle: &str,
    name: &str,
    prompt_order: i64,
    message: PromptMessage,
) -> Option<JournalEntry> {
    if message.content.is_empty()
        || message.content.iter().all(|item| match item {
            ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                text.trim().is_empty()
            }
            ContentItem::InputImage { .. } => false,
        })
    {
        return None;
    }

    Some(JournalEntry::new(
        [PROMPT_BUNDLE_KEY_PREFIX, bundle, name],
        JournalContextItem::new(JournalContextKey::new(bundle, name, None), message)
            .with_prompt_order(prompt_order),
    ))
}

pub(crate) fn render_context_entries(entries: Vec<JournalEntry>) -> Vec<ResponseItem> {
    let flattened = match Journal::from_entries(entries).flatten() {
        Ok(flattened) => flattened,
        Err(error) => unreachable!("context-only journal entries should flatten: {error}"),
    };

    let mut rendered = Vec::new();
    let mut current_bundle: Option<Vec<String>> = None;
    let mut current_role: Option<PromptMessageRole> = None;
    let mut current_content = Vec::new();

    for entry in flattened.entries() {
        let JournalItem::Context(item) = &entry.item else {
            continue;
        };
        let bundle = entry
            .key
            .parts()
            .iter()
            .take(2)
            .cloned()
            .collect::<Vec<_>>();

        if current_bundle.as_ref() != Some(&bundle) || current_role != Some(item.message.role) {
            flush_prompt_message(&mut rendered, &mut current_role, &mut current_content);
            current_bundle = Some(bundle);
            current_role = Some(item.message.role);
        }

        current_content.extend(item.message.content.clone());
    }

    flush_prompt_message(&mut rendered, &mut current_role, &mut current_content);
    rendered
}

fn flush_prompt_message(
    rendered: &mut Vec<ResponseItem>,
    role: &mut Option<PromptMessageRole>,
    content: &mut Vec<codex_protocol::models::ContentItem>,
) {
    let Some(role) = role.take() else {
        return;
    };
    if content.is_empty() {
        return;
    }
    rendered.push(ResponseItem::from(PromptMessage::new(
        role,
        std::mem::take(content),
    )));
}

pub(crate) fn build_settings_update_entries(
    previous: Option<&TurnContextItem>,
    previous_turn_settings: Option<&PreviousTurnSettings>,
    next: &TurnContext,
    shell: &Shell,
    exec_policy: &Policy,
    personality_feature_enabled: bool,
) -> Vec<JournalEntry> {
    // TODO(ccunningham): build_settings_update_items still does not cover every
    // model-visible item emitted by build_initial_context. Persist the remaining
    // inputs or add explicit replay events so fork/resume can diff everything
    // deterministically.
    let mut entries = Vec::with_capacity(6);

    if let Some(item) =
        build_model_instructions_update_item(previous_turn_settings, next).and_then(|text| {
            // Keep model-switch instructions first so model-specific guidance is read before
            // any other context diffs on this turn.
            developer_context_entry("model_switch", 10, text)
        })
    {
        entries.push(item);
    }
    if let Some(item) = build_permissions_update_item(previous, next, exec_policy)
        .and_then(|text| developer_context_entry("permissions", 20, text))
    {
        entries.push(item);
    }
    if let Some(item) = build_collaboration_mode_update_item(previous, next)
        .and_then(|text| developer_context_entry("collaboration_mode", 30, text))
    {
        entries.push(item);
    }
    if let Some(item) = build_realtime_update_item(previous, previous_turn_settings, next)
        .and_then(|text| developer_context_entry("realtime", 40, text))
    {
        entries.push(item);
    }
    if let Some(item) = build_personality_update_item(previous, next, personality_feature_enabled)
        .and_then(|text| developer_context_entry("personality", 50, text))
    {
        entries.push(item);
    }
    if let Some(item) = build_environment_update_item(previous, next, shell)
        .and_then(|text| contextual_user_context_entry("environment", 60, text))
    {
        entries.push(item);
    }

    entries
}
