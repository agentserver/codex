//! Centralized feature flags and metadata.
//!
//! This crate defines the feature registry plus the logic used to resolve an
//! effective feature set from config-like inputs.

mod feature_configs;
mod legacy;
mod machinery;
mod registry;

pub use feature_configs::AppsMcpPathOverrideConfigToml;
pub use feature_configs::MultiAgentV2ConfigToml;
pub use legacy::legacy_feature_keys;
pub use machinery::CommonFeatureConfigToml;
pub use machinery::Feature;
pub use machinery::FeatureConfigSource;
pub use machinery::FeatureConfigTable;
pub use machinery::FeatureOverrides;
pub use machinery::FeatureToml;
pub use machinery::Features;
pub use machinery::FeaturesToml;
pub use machinery::LegacyFeatureUsage;
pub use machinery::NoExtraFeatureConfigToml;
pub use machinery::RawFeatureConfigExtras;
pub use machinery::Stage;
pub use machinery::canonical_feature_for_key;
pub use machinery::feature_for_key;
pub use machinery::is_known_feature_key;
pub use machinery::unstable_features_warning_event;
pub use registry::FEATURES;
pub use registry::FeatureSpec;

#[cfg(test)]
mod tests;
