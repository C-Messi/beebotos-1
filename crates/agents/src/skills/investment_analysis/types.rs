//! Investment Analysis structured output types
//!
//! Defines the JSON schema for the final_answer output from the ReAct
//! investment analysis loop. These types are validated by the post-processor.

use serde::{Deserialize, Serialize};

/// Root investment analysis report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvestmentAnalysisReport {
    pub version: String,
    pub symbol: String,
    pub analysis_summary: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub technical_analysis: Option<TechnicalAnalysis>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub sentiment_analysis: Option<SentimentAnalysis>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub onchain_macro: Option<OnchainMacro>,

    pub verdict: Verdict,

    #[serde(default)]
    pub suggested_actions: Vec<SuggestedAction>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_levels: Option<KeyLevels>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_specific: Option<UserSpecificNotes>,

    #[serde(default)]
    pub data_sources: Vec<DataSource>,

    #[serde(default)]
    pub risk_warnings: Vec<String>,

    pub disclaimer: String,
}

/// Technical analysis section
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechnicalAnalysis {
    pub price: f64,
    pub change_24h_pct: f64,

    #[serde(default)]
    pub key_indicators: Vec<Indicator>,

    #[serde(default)]
    pub support_levels: Vec<f64>,

    #[serde(default)]
    pub resistance_levels: Vec<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub trend_assessment: Option<String>,
}

/// Individual indicator reading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Indicator {
    pub name: String,
    pub value: String,
    pub signal: String,
}

/// Sentiment analysis section
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentimentAnalysis {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fear_greed_index: Option<i32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub fear_greed_label: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub funding_rate: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub orderbook_pressure: Option<String>,
}

/// On-chain and macro context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnchainMacro {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exchange_netflow: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub btc_dominance: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub stablecoin_inflow: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub whale_activity: Option<String>,
}

/// Investment verdict
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Verdict {
    pub action: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_horizon: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
}

/// Suggested action item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedAction {
    pub action: String,
    pub rationale: String,

    #[serde(default)]
    pub conditions: Vec<String>,
}

/// Key price levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyLevels {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_zone: Option<Vec<f64>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_loss: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub take_profit: Option<Vec<f64>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_reward: Option<String>,
}

/// User-specific notes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSpecificNotes {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub portfolio_impact: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub emotional_guidance: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_reminder: Option<String>,
}

/// Data source attribution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSource {
    pub tool: String,
    pub round: usize,
}

/// Valid verdict actions
pub const VERDICT_ACTIONS: &[&str] = &[
    "strong_buy",
    "buy",
    "hold",
    "sell",
    "strong_sell",
    "uncertain",
];

/// Risk level thresholds
pub const HIGH_RISK_THRESHOLD: f64 = 7.0;
