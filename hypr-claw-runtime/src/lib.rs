//! Hypr-Claw Runtime Core
//!
//! A production-grade agent runtime kernel implementing deterministic control flow.

pub mod types;
pub mod interfaces;
pub mod gateway;
pub mod agent_config;
pub mod llm_client;
pub mod compactor;
pub mod agent_loop;
pub mod runtime_controller;
pub mod async_adapters;
pub mod metrics;

pub use types::{LLMResponse, Message, Role, SCHEMA_VERSION};
pub use interfaces::{RuntimeError, SessionStore, LockManager, ToolDispatcher, ToolRegistry};
pub use gateway::resolve_session;
pub use agent_config::{AgentConfig, load_agent_config};
pub use llm_client::LLMClient;
pub use compactor::{Compactor, Summarizer};
pub use agent_loop::AgentLoop;
pub use runtime_controller::RuntimeController;
pub use async_adapters::{AsyncSessionStore, AsyncLockManager};
