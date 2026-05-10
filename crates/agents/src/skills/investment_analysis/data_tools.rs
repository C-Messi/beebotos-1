//! Investment Analysis Data Tools
//!
//! Wraps existing MCP crypto skills as SkillTool implementations for use
//! in the UnifiedReActExecutor. Also provides fallback tools via web_fetch.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tracing::{debug, error, info};

use crate::error::AgentError;
use crate::mcp::client::MCPClient;
use crate::mcp::skill_bridge::parse_mcp_skill_id;
use crate::skills::tool_set::SkillTool;

/// Build the default set of investment analysis data tools
///
/// Maps MCP skills to local tool names that the LLM can reference.
pub async fn build_analysis_tools(
    mcp_manager: Option<&crate::mcp::MCPManager>,
) -> HashMap<String, Box<dyn SkillTool>> {
    let mut tools: HashMap<String, Box<dyn SkillTool>> = HashMap::new();

    // Try to discover MCP crypto tools and wrap them
    if let Some(mgr) = mcp_manager {
        if let Some(client_arc) = mgr.get_client("alpaca").await {
            // crypto_price → alpaca/get_crypto_snapshot
            tools.insert(
                "crypto_price".to_string(),
                Box::new(McpDataTool::new(
                    client_arc.clone(),
                    "alpaca",
                    "get_crypto_snapshot",
                    "获取指定加密货币的实时快照数据（价格、涨跌幅、成交量等）。参数: symbols (string, 如 BTC/USD), loc (string, 固定值 us)",
                    serde_json::json!({
                        "type": "object",
                        "properties": {
                            "symbols": { "type": "string", "description": "交易对，如 BTC/USD, ETH/USD" },
                            "loc": { "type": "string", "enum": ["us"], "description": "地区代码，固定为 us" }
                        },
                        "required": ["symbols", "loc"]
                    }),
                )),
            );

            // fetch_ohlcv → alpaca/get_crypto_bars
            tools.insert(
                "fetch_ohlcv".to_string(),
                Box::new(McpDataTool::new(
                    client_arc.clone(),
                    "alpaca",
                    "get_crypto_bars",
                    "获取加密货币K线数据。参数: symbols (string, 如 BTC/USD), timeframe (string: 1Min/5Min/15Min/1Hour/4Hour/1Day), limit (integer, default 50)",
                    serde_json::json!({
                        "type": "object",
                        "properties": {
                            "symbols": { "type": "string", "description": "交易对，如 BTC/USD" },
                            "timeframe": { "type": "string", "enum": ["1Min","5Min","15Min","1Hour","4Hour","1Day"], "description": "时间框架，注意大小写（如 1Hour, 1Day）" },
                            "limit": { "type": "integer", "default": 50, "description": "返回条数" }
                        },
                        "required": ["symbols", "timeframe"]
                    }),
                )),
            );

            // get_orderbook → alpaca/get_crypto_latest_orderbook
            tools.insert(
                "get_orderbook".to_string(),
                Box::new(McpDataTool::new(
                    client_arc.clone(),
                    "alpaca",
                    "get_crypto_latest_orderbook",
                    "获取加密货币最新订单簿数据（买卖盘、价差）。参数: symbols (string, 如 BTC/USD), loc (string, 固定值 us)",
                    serde_json::json!({
                        "type": "object",
                        "properties": {
                            "symbols": { "type": "string", "description": "交易对，如 BTC/USD" },
                            "loc": { "type": "string", "enum": ["us"], "description": "地区代码，固定为 us" }
                        },
                        "required": ["symbols", "loc"]
                    }),
                )),
            );

            // get_latest_trade → alpaca/get_crypto_latest_trade
            tools.insert(
                "get_latest_trade".to_string(),
                Box::new(McpDataTool::new(
                    client_arc,
                    "alpaca",
                    "get_crypto_latest_trade",
                    "获取加密货币最新成交数据。参数: symbols (string, 如 BTC/USD), loc (string, 固定值 us)",
                    serde_json::json!({
                        "type": "object",
                        "properties": {
                            "symbols": { "type": "string", "description": "交易对，如 BTC/USD" },
                            "loc": { "type": "string", "enum": ["us"], "description": "地区代码，固定为 us" }
                        },
                        "required": ["symbols", "loc"]
                    }),
                )),
            );

            info!(
                "Registered {} MCP crypto tools for ReAct analysis",
                tools.len()
            );
        } else {
            debug!("MCP alpaca client not available, skipping MCP tool registration");
        }
    }

    // Fallback / computed tools (LLM can use web_fetch or calculate from OHLCV)
    tools.insert(
        "calculate_rsi".to_string(),
        Box::new(ComputedTool::new(
            "calculate_rsi",
            "基于已获取的OHLCV数据计算RSI指标。参数: symbol (string), period (integer, default 14)",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "symbol": { "type": "string" },
                    "period": { "type": "integer", "default": 14 }
                },
                "required": ["symbol"]
            }),
        )),
    );

    tools.insert(
        "calculate_macd".to_string(),
        Box::new(ComputedTool::new(
            "calculate_macd",
            "基于已获取的OHLCV数据计算MACD指标。参数: symbol (string), fast (int, default 12), \
             slow (int, default 26), signal (int, default 9)",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "symbol": { "type": "string" },
                    "fast": { "type": "integer", "default": 12 },
                    "slow": { "type": "integer", "default": 26 },
                    "signal": { "type": "integer", "default": 9 }
                },
                "required": ["symbol"]
            }),
        )),
    );

    tools.insert(
        "get_fear_greed_index".to_string(),
        Box::new(WebFetchToolWrapper::new(
            "get_fear_greed_index",
            "获取加密货币恐惧贪婪指数。无需参数，直接调用即可。",
            "https://api.alternative.me/fng/?limit=1",
        )),
    );

    tools
}

/// Wraps an MCP client tool as a SkillTool
pub struct McpDataTool {
    client: Arc<MCPClient>,
    server_name: String,
    tool_name: String,
    description: String,
    params_schema: Value,
}

impl McpDataTool {
    pub fn new(
        client: Arc<MCPClient>,
        server_name: &str,
        tool_name: &str,
        description: &str,
        params_schema: Value,
    ) -> Self {
        Self {
            client,
            server_name: server_name.to_string(),
            tool_name: tool_name.to_string(),
            description: description.to_string(),
            params_schema,
        }
    }
}

#[async_trait]
impl SkillTool for McpDataTool {
    fn name(&self) -> &str {
        &self.tool_name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> Value {
        self.params_schema.clone()
    }

    async fn execute(&self, params: &Value) -> Result<String, String> {
        debug!(
            "Executing MCP tool {}::{} with params: {}",
            self.server_name, self.tool_name, params
        );

        let args = params.as_object().cloned().unwrap_or_default();

        match self.client.call_tool(&self.tool_name, Some(args)).await {
            Ok(result) => {
                let mut texts = Vec::new();
                for content in &result.content {
                    if let crate::mcp::types::ToolContent::Text { text } = content {
                        texts.push(text.clone());
                    }
                }
                let output = if texts.is_empty() {
                    serde_json::to_string(&result).unwrap_or_default()
                } else {
                    texts.join("\n")
                };
                // Truncate very long outputs
                if output.len() > 4000 {
                    Ok(format!(
                        "{}...[truncated {} chars]",
                        &output[..4000],
                        output.len() - 4000
                    ))
                } else {
                    Ok(output)
                }
            }
            Err(e) => {
                error!(
                    "MCP tool {}::{} failed: {}",
                    self.server_name, self.tool_name, e
                );
                Err(format!("MCP tool error: {}", e))
            }
        }
    }
}

/// Placeholder tool for computed indicators (RSI, MACD)
/// In a full implementation, this would calculate from cached OHLCV data.
/// For now, it returns a message instructing the LLM to calculate from
/// the OHLCV data it already fetched.
pub struct ComputedTool {
    name: String,
    description: String,
    params_schema: Value,
}

impl ComputedTool {
    pub fn new(name: &str, description: &str, params_schema: Value) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            params_schema,
        }
    }
}

#[async_trait]
impl SkillTool for ComputedTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> Value {
        self.params_schema.clone()
    }

    async fn execute(&self, params: &Value) -> Result<String, String> {
        let symbol = params["symbol"].as_str().unwrap_or("unknown");
        let period = params["period"].as_u64().unwrap_or(14);

        // In a production system, this would:
        // 1. Retrieve cached OHLCV data for the symbol
        // 2. Calculate the indicator
        // 3. Return the computed value
        //
        // For now, return a descriptive message so the LLM knows
        // it needs to calculate this from previously fetched data.
        Ok(format!(
            "[计算型工具] 请基于之前获取的 {} OHLCV 数据，计算 \
             {}（周期={}）。该工具在此版本中为占位符，实际计算由分析引擎在后续版本中实现。\
             当前建议：基于已有K线数据自行估算该指标。",
            symbol, self.name, period
        ))
    }
}

/// Wrapper around web_fetch for data sources that don't have MCP tools
pub struct WebFetchToolWrapper {
    name: String,
    description: String,
    url: String,
}

impl WebFetchToolWrapper {
    pub fn new(name: &str, description: &str, url: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            url: url.to_string(),
        }
    }
}

#[async_trait]
impl SkillTool for WebFetchToolWrapper {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn execute(&self, _params: &Value) -> Result<String, String> {
        use crate::skills::tool_set::WebFetchTool;

        let tool = WebFetchTool;
        let fetch_params = serde_json::json!({
            "url": self.url,
            "max_length": 2000
        });

        tool.execute(&fetch_params).await
    }
}
