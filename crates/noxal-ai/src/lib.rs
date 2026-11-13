//! NoxalAi: local assistant service. Local only, no network, no telemetry.
//! No models are downloaded by this project; users point the registry at
//! GGUF files they already have (Qwen family, small quantised models).

pub mod backend;
pub mod convo;
pub mod gate;
pub mod models;
pub mod prompt;
pub mod tools;
