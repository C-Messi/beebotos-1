//! Investment Analysis Module
//!
//! Provides autonomous crypto investment analysis via the UnifiedReActExecutor.
//!
//! # Architecture
//! - `types`: Structured output schema for the final analysis report
//! - `prompt`: System Prompt builder for the ReAct loop
//! - `post_processor`: Safety validation and formatting of the LLM output
//! - `data_tools`: MCP skill wrappers that expose crypto data as SkillTool
//!   instances
//!
//! # Usage
//! ```rust,ignore
//! let tools = build_analysis_tools(mcp_manager).await;
//! let prompt = build_investment_analysis_prompt(&tools, user_context);
//! let result = executor.execute(prompt, &user_request, &tools).await?;
//! let report = post_process_final_answer(&result, user_risk_level)?;
//! ```

pub mod data_tools;
pub mod post_processor;
pub mod prompt;
pub mod types;

// Re-export key types for convenience
pub use data_tools::build_analysis_tools;
pub use post_processor::{format_report_for_user, post_process_final_answer};
pub use prompt::{
    build_forced_final_prompt, build_initial_round_prompt, build_investment_analysis_prompt,
    build_round_prompt,
};
pub use types::InvestmentAnalysisReport;

/// Internal record of a single ReAct round (used by prompt builder)
#[derive(Debug, Clone)]
pub struct RoundRecord {
    pub round_number: usize,
    pub thought: String,
    pub action: ReActAction,
    pub observation: Option<String>,
}

/// Internal action representation (used by prompt builder)
#[derive(Debug, Clone)]
pub enum ReActAction {
    CallTool {
        tool_name: String,
        arguments: serde_json::Map<String, serde_json::Value>,
        reasoning: String,
    },
    FinalAnswer {
        content: String,
    },
}
