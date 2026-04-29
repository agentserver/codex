//! Pure history utilities for normalizing, classifying, rewriting, and estimating prompt-ready
//! history items.

mod classify;
mod estimate;
mod normalize;
mod transform;

#[cfg(test)]
mod tests;

pub use classify::is_api_message;
pub use classify::is_codex_generated_item;
pub use classify::is_model_generated_item;
pub use classify::is_user_turn_boundary;
pub use classify::user_turn_boundary_positions;
pub use estimate::ORIGINAL_IMAGE_MAX_PATCHES;
pub use estimate::RESIZED_IMAGE_BYTES_ESTIMATE;
pub use estimate::estimate_item_token_count;
pub use estimate::estimate_response_item_model_visible_bytes;
pub use normalize::ensure_call_outputs_present;
pub use normalize::remove_corresponding_for;
pub use normalize::remove_orphan_outputs;
pub use normalize::strip_images_when_unsupported;
pub use transform::replace_last_turn_images;
pub use transform::truncate_history_item;
