use crate::models::ContentItem;
use crate::models::ResponseItem;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

/// Stable key attached to a journal entry.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, TS, Default)]
pub struct JournalKey {
    pub parts: Vec<String>,
}

impl JournalKey {
    /// Creates a key from an ordered sequence of parts.
    pub fn new<I, S>(parts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            parts: parts.into_iter().map(Into::into).collect(),
        }
    }

    /// Returns the ordered key parts.
    pub fn parts(&self) -> &[String] {
        &self.parts
    }

    /// Returns a new key with one child part appended.
    pub fn child(&self, part: impl Into<String>) -> Self {
        let mut parts = self.parts.clone();
        parts.push(part.into());
        Self { parts }
    }

    /// Returns whether this key starts with the provided prefix.
    pub fn starts_with(&self, prefix: &JournalKey) -> bool {
        self.parts.starts_with(prefix.parts.as_slice())
    }
}

impl<S, const N: usize> From<[S; N]> for JournalKey
where
    S: Into<String>,
{
    fn from(value: [S; N]) -> Self {
        Self::new(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
#[ts(tag = "type", content = "value", rename_all = "snake_case")]
pub enum KeyFilter {
    Exact(JournalKey),
    Prefix(JournalKey),
}

impl KeyFilter {
    /// Matches one exact key.
    pub fn exact(key: impl Into<JournalKey>) -> Self {
        Self::Exact(key.into())
    }

    /// Matches any key with the provided prefix.
    pub fn prefix(prefix: impl Into<JournalKey>) -> Self {
        Self::Prefix(prefix.into())
    }

    /// Returns whether the filter matches the key.
    pub fn matches(&self, key: &JournalKey) -> bool {
        match self {
            Self::Exact(expected) => key == expected,
            Self::Prefix(prefix) => key.starts_with(prefix),
        }
    }
}

/// Stable identity for a prompt-context entry.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, TS)]
pub struct JournalContextKey {
    pub namespace: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub instance: Option<String>,
}

impl JournalContextKey {
    pub fn new(
        namespace: impl Into<String>,
        name: impl Into<String>,
        instance: Option<String>,
    ) -> Self {
        Self {
            namespace: namespace.into(),
            name: name.into(),
            instance,
        }
    }
}

/// Minimal message block that can be projected into a model-visible prompt item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct PromptMessage {
    pub role: PromptMessageRole,
    pub content: Vec<ContentItem>,
}

impl PromptMessage {
    pub fn new(role: PromptMessageRole, content: Vec<ContentItem>) -> Self {
        Self { role, content }
    }

    pub fn developer_text(text: impl Into<String>) -> Self {
        Self::text(PromptMessageRole::Developer, text)
    }

    pub fn user_text(text: impl Into<String>) -> Self {
        Self::text(PromptMessageRole::User, text)
    }

    pub fn assistant_text(text: impl Into<String>) -> Self {
        Self::text(PromptMessageRole::Assistant, text)
    }

    fn text(role: PromptMessageRole, text: impl Into<String>) -> Self {
        let text = text.into();
        let content = match role {
            PromptMessageRole::Developer | PromptMessageRole::User => {
                vec![ContentItem::InputText { text }]
            }
            PromptMessageRole::Assistant => vec![ContentItem::OutputText { text }],
        };
        Self::new(role, content)
    }
}

impl From<PromptMessage> for ResponseItem {
    fn from(value: PromptMessage) -> Self {
        ResponseItem::Message {
            id: None,
            role: value.role.as_str().to_string(),
            content: value.content,
            phase: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS, Default)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum PromptMessageRole {
    #[default]
    Developer,
    User,
    Assistant,
}

impl PromptMessageRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Developer => "developer",
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }
}

/// Durable history item. Unlike prompt context, history keeps original ordering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct JournalHistoryItem {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub turn_id: Option<String>,
    pub item: ResponseItem,
}

impl JournalHistoryItem {
    pub fn new(item: ResponseItem) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            turn_id: None,
            item,
        }
    }

    pub fn with_turn_id(mut self, turn_id: impl Into<String>) -> Self {
        self.turn_id = Some(turn_id.into());
        self
    }
}

impl From<ResponseItem> for JournalHistoryItem {
    fn from(value: ResponseItem) -> Self {
        Self::new(value)
    }
}

impl From<JournalHistoryItem> for ResponseItem {
    fn from(value: JournalHistoryItem) -> Self {
        value.item
    }
}

impl From<JournalContextItem> for ResponseItem {
    fn from(value: JournalContextItem) -> Self {
        value.message.into()
    }
}

/// Prompt-context entry with stable identity and filtering metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct JournalContextItem {
    pub key: JournalContextKey,
    pub message: PromptMessage,
    #[serde(default)]
    pub prompt_order: i64,
    #[serde(default)]
    pub audience: JournalContextAudience,
    #[serde(default)]
    pub on_fork: JournalContextForkBehavior,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source: Option<String>,
}

impl JournalContextItem {
    pub fn new(key: JournalContextKey, message: PromptMessage) -> Self {
        Self {
            key,
            message,
            prompt_order: 0,
            audience: JournalContextAudience::default(),
            on_fork: JournalContextForkBehavior::default(),
            tags: Vec::new(),
            source: None,
        }
    }

    pub fn with_prompt_order(mut self, prompt_order: i64) -> Self {
        self.prompt_order = prompt_order;
        self
    }

    pub fn with_audience(mut self, audience: JournalContextAudience) -> Self {
        self.audience = audience;
        self
    }

    pub fn with_on_fork(mut self, on_fork: JournalContextForkBehavior) -> Self {
        self.on_fork = on_fork;
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS, Default)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
#[ts(tag = "type", content = "value", rename_all = "snake_case")]
pub enum JournalContextAudience {
    #[default]
    All,
    RootOnly,
    SubAgentsOnly,
    AgentPathPrefix(String),
    AgentRole(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS, Default)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum JournalContextForkBehavior {
    #[default]
    Keep,
    Drop,
    Regenerate,
}

/// Cursor into the effective history at the point a checkpoint is applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
#[ts(tag = "type", content = "value", rename_all = "snake_case")]
pub enum JournalHistoryCursor {
    Start,
    AfterItem(String),
    End,
}

/// Replace the current history prefix through the resolved cursor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct JournalReplacePrefixCheckpoint {
    pub through: JournalHistoryCursor,
    pub replacement: Vec<JournalHistoryItem>,
}

/// Keep only the current history prefix through the resolved cursor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct JournalTruncateHistoryCheckpoint {
    pub through: JournalHistoryCursor,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
#[ts(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum JournalCheckpointItem {
    ReplacePrefix(JournalReplacePrefixCheckpoint),
    TruncateHistory(JournalTruncateHistoryCheckpoint),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
#[ts(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum JournalItem {
    History(JournalHistoryItem),
    Context(JournalContextItem),
    Checkpoint(JournalCheckpointItem),
}

impl From<ResponseItem> for JournalItem {
    fn from(value: ResponseItem) -> Self {
        Self::History(value.into())
    }
}

impl From<JournalHistoryItem> for JournalItem {
    fn from(value: JournalHistoryItem) -> Self {
        Self::History(value)
    }
}

impl From<JournalContextItem> for JournalItem {
    fn from(value: JournalContextItem) -> Self {
        Self::Context(value)
    }
}

impl From<JournalCheckpointItem> for JournalItem {
    fn from(value: JournalCheckpointItem) -> Self {
        Self::Checkpoint(value)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct JournalEntry {
    pub key: JournalKey,
    pub item: JournalItem,
}

impl JournalEntry {
    /// Creates a keyed journal entry.
    pub fn new(key: impl Into<JournalKey>, item: impl Into<JournalItem>) -> Self {
        Self {
            key: key.into(),
            item: item.into(),
        }
    }
}

impl<K, T> From<(K, T)> for JournalEntry
where
    K: Into<JournalKey>,
    T: Into<JournalItem>,
{
    fn from(value: (K, T)) -> Self {
        Self::new(value.0, value.1)
    }
}
