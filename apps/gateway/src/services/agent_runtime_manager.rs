//! Agent Runtime Manager
//!
//! Bridges the Gateway application layer with the `beebotos_agents` runtime.
//! Uses unified `AgentStateManager` as the single source of truth for both
//! agent state and agent instances, eliminating state synchronization issues.
//!
//! 🔒 P0 FIX: Removed duplicate in-memory HashMap, now fully relying on
//! AgentStateManager for both state and instance management.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tracing::{error, info, warn};

use crate::config::BeeBotOSConfig;
use crate::error::AppError;

/// Gateway wrapper that implements the agents crate's LLMCallInterface
/// using the gateway's own LlmService.
pub struct GatewayLLMInterface {
    llm_service: Arc<crate::services::llm_service::LlmService>,
    react_trace_sink: Option<Arc<dyn beebotos_agents::ReActTraceSink>>,
}

impl GatewayLLMInterface {
    pub fn new(llm_service: Arc<crate::services::llm_service::LlmService>) -> Self {
        Self {
            llm_service,
            react_trace_sink: None,
        }
    }

    pub fn with_react_trace_sink(mut self, sink: Arc<dyn beebotos_agents::ReActTraceSink>) -> Self {
        self.react_trace_sink = Some(sink);
        self
    }

    fn convert_messages(
        messages: &[beebotos_agents::communication::Message],
    ) -> Vec<beebotos_agents::llm::Message> {
        use beebotos_agents::llm::{Content, Message as LLMMessage, Role};

        let mut system_parts = Vec::new();
        let mut llm_messages = Vec::new();

        for (idx, msg) in messages.iter().enumerate() {
            let content = msg.content.trim();
            let role = msg.metadata.get("role").map(|s| s.as_str());

            match role {
                Some("system") => system_parts.push(content.to_string()),
                Some("assistant") => {
                    let mut out = LLMMessage::assistant(content.to_string());
                    if let Some(tool_calls_json) = msg.metadata.get("tool_calls_json") {
                        if let Ok(tool_calls) = serde_json::from_str(tool_calls_json) {
                            out = out.with_tool_calls(tool_calls);
                        }
                    }
                    if let Some(reasoning_content) = msg.metadata.get("reasoning_content") {
                        out.reasoning_content = Some(reasoning_content.clone());
                    }
                    llm_messages.push(out);
                }
                Some("tool") => {
                    llm_messages.push(LLMMessage {
                        role: Role::Tool,
                        content: vec![Content::Text {
                            text: msg.content.clone(),
                        }],
                        name: None,
                        tool_calls: None,
                        tool_call_id: msg.metadata.get("tool_call_id").cloned(),
                        reasoning_content: None,
                    });
                }
                Some("user") => llm_messages.push(LLMMessage::user(content.to_string())),
                _ if idx == 0 => system_parts.push(content.to_string()),
                _ if content.starts_with("[系统提示")
                    || content.starts_with("以下是与当前对话相关的历史记忆") =>
                {
                    system_parts.push(content.to_string())
                }
                _ if content.starts_with("用户:") => {
                    if let Some(rest) = content.strip_prefix("用户:") {
                        llm_messages.push(LLMMessage::user(rest.trim().to_string()));
                    }
                }
                _ if content.starts_with("助手:") => {
                    if let Some(rest) = content.strip_prefix("助手:") {
                        let trimmed = rest.trim();
                        if !trimmed.is_empty() {
                            llm_messages.push(LLMMessage::assistant(trimmed.to_string()));
                        }
                    }
                }
                _ if content.starts_with("系统:") => {
                    if let Some(rest) = content.strip_prefix("系统:") {
                        system_parts.push(rest.trim().to_string());
                    }
                }
                _ => llm_messages.push(LLMMessage::user(content.to_string())),
            }
        }

        if !system_parts.is_empty() {
            let system_text = system_parts.join("\n\n");
            let mut final_messages = vec![LLMMessage::system(system_text)];
            final_messages.extend(llm_messages);
            final_messages
        } else {
            llm_messages
        }
    }

    fn max_tokens_from_context(context: &Option<HashMap<String, String>>) -> Option<u32> {
        context
            .as_ref()
            .and_then(|c| c.get("max_tokens"))
            .and_then(|t| t.parse::<u32>().ok())
    }

    fn model_from_context(context: &Option<HashMap<String, String>>) -> Option<String> {
        context.as_ref().and_then(|c| c.get("model")).cloned()
    }

    fn tool_choice_from_context(context: &Option<HashMap<String, String>>) -> Option<String> {
        context.as_ref().and_then(|c| c.get("tool_choice")).cloned()
    }

    fn tools_from_context(
        context: &Option<HashMap<String, String>>,
    ) -> Option<Vec<beebotos_agents::llm::Tool>> {
        context
            .as_ref()
            .and_then(|c| c.get("tools_json"))
            .and_then(|json_str| {
                serde_json::from_str::<Vec<beebotos_agents::communication::ToolDefinition>>(
                    json_str,
                )
                .ok()
            })
            .map(Self::convert_tools)
    }

    async fn emit_tool_call_messages(
        &self,
        context: &Option<HashMap<String, String>>,
        tool_calls: &[beebotos_agents::llm::ToolCall],
    ) {
        let Some(sink) = &self.react_trace_sink else {
            return;
        };
        let Some(context) = context.as_ref() else {
            return;
        };
        let Some(session_id) = context
            .get("session_id")
            .or_else(|| context.get("channel_id"))
            .cloned()
        else {
            return;
        };
        let agent_id = context
            .get("agent_id")
            .cloned()
            .unwrap_or_else(|| "gateway".to_string());
        let run_id = context
            .get("react_run_id")
            .cloned()
            .unwrap_or_else(|| format!("gateway-{}", uuid::Uuid::new_v4()));
        let round = context
            .get("react_round")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(1);
        let max_rounds = context
            .get("max_tool_rounds")
            .and_then(|v| v.parse::<u32>().ok())
            .map(beebotos_agents::clamp_react_max_tool_rounds)
            .unwrap_or(beebotos_agents::DEFAULT_REACT_MAX_TOOL_ROUNDS);

        for tool_call in tool_calls {
            let event = beebotos_agents::ReActTraceEvent::new(
                run_id.clone(),
                session_id.clone(),
                agent_id.clone(),
                round,
                max_rounds,
                beebotos_agents::ReActTracePhase::AssistantToolCall,
            )
            .with_tool(tool_call.function.name.clone(), tool_call.id.clone())
            .with_arguments_preview(&tool_call.function.arguments);
            if let Err(e) = sink.emit(event).await {
                warn!("Failed to emit LLM tool_call message: {}", e);
            }
        }
    }

    fn convert_tools(
        defs: Vec<beebotos_agents::communication::ToolDefinition>,
    ) -> Vec<beebotos_agents::llm::Tool> {
        defs.into_iter()
            .map(|d| beebotos_agents::llm::Tool {
                r#type: "function".to_string(),
                function: beebotos_agents::llm::FunctionDefinition {
                    name: d.name,
                    description: Some(d.description),
                    parameters: d.parameters,
                },
            })
            .collect()
    }
}

#[async_trait]
impl beebotos_agents::communication::LLMCallInterface for GatewayLLMInterface {
    fn supports_native_tools(&self) -> bool {
        true
    }

    async fn call_llm(
        &self,
        messages: Vec<beebotos_agents::communication::Message>,
        _context: Option<std::collections::HashMap<String, String>>,
    ) -> beebotos_agents::error::Result<String> {
        let final_messages = Self::convert_messages(&messages);

        let max_tokens_override = Self::max_tokens_from_context(&_context);
        let model_override = Self::model_from_context(&_context);
        let tools = Self::tools_from_context(&_context);
        let tool_choice = Self::tool_choice_from_context(&_context);

        self.llm_service
            .chat(
                final_messages,
                max_tokens_override,
                tools,
                tool_choice,
                model_override,
            )
            .await
            .map_err(|e| {
                beebotos_agents::error::AgentError::Execution(format!("LLM call failed: {}", e))
            })
    }

    async fn call_llm_tool_turn(
        &self,
        messages: Vec<beebotos_agents::communication::Message>,
        tools: Vec<beebotos_agents::communication::ToolDefinition>,
        context: Option<HashMap<String, String>>,
    ) -> beebotos_agents::error::Result<beebotos_agents::communication::ToolAwareResponse> {
        let final_messages = Self::convert_messages(&messages);
        let llm_tools = Self::convert_tools(tools);
        let max_tokens_override = Self::max_tokens_from_context(&context);
        let model_override = Self::model_from_context(&context);
        let tool_choice = Self::tool_choice_from_context(&context);

        let turn = self
            .llm_service
            .chat_turn(
                final_messages,
                max_tokens_override,
                Some(llm_tools),
                tool_choice,
                model_override,
            )
            .await
            .map_err(|e| {
                beebotos_agents::error::AgentError::Execution(format!("LLM call failed: {}", e))
            })?;

        if !turn.tool_calls.is_empty() {
            self.emit_tool_call_messages(&context, &turn.tool_calls)
                .await;
        }

        Ok(beebotos_agents::communication::ToolAwareResponse {
            content: turn.content,
            tool_calls: turn.tool_calls,
            reasoning_content: turn.reasoning_content,
        })
    }

    async fn call_llm_stream(
        &self,
        messages: Vec<beebotos_agents::communication::Message>,
        _context: Option<std::collections::HashMap<String, String>>,
    ) -> beebotos_agents::error::Result<tokio::sync::mpsc::Receiver<String>> {
        let final_messages = Self::convert_messages(&messages);

        let max_tokens_override = Self::max_tokens_from_context(&_context);
        let model_override = Self::model_from_context(&_context);

        self.llm_service
            .chat_stream(final_messages, max_tokens_override, model_override)
            .await
            .map_err(|e| {
                beebotos_agents::error::AgentError::Execution(format!("LLM stream failed: {}", e))
            })
    }
}

/// Agent instance holder that wraps beebotos_agents::Agent
/// and provides lifecycle management.
///
/// 🔒 P0 FIX: This struct is now stored in AgentStateManager's metadata
/// instead of a separate HashMap, ensuring single source of truth.
pub struct AgentInstance {
    /// The underlying agent
    pub agent: beebotos_agents::Agent,
    /// When the agent was created
    #[allow(dead_code)]
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl AgentInstance {
    /// Create a new agent instance from configuration
    pub async fn new(
        agent_id: &str,
        db_agent: &crate::models::AgentRecord,
        config: &BeeBotOSConfig,
        kernel: Option<Arc<beebotos_kernel::Kernel>>,
        llm_service: Arc<crate::services::llm_service::LlmService>,
        memory_system: Option<Arc<beebotos_agents::memory::UnifiedMemorySystem>>,
        skill_registry: Option<Arc<beebotos_agents::skills::SkillRegistry>>,
        react_trace_sink: Option<Arc<dyn beebotos_agents::ReActTraceSink>>,
    ) -> Result<Self, beebotos_agents::error::AgentError> {
        let agent_config = beebotos_agents::AgentConfig {
            id: agent_id.to_string(),
            name: db_agent.name.clone(),
            description: db_agent.description.clone().unwrap_or_default(),
            version: "1.0.0".to_string(),
            capabilities: db_agent.capabilities.clone(),
            models: beebotos_agents::ModelConfig {
                provider: db_agent
                    .model_provider
                    .clone()
                    .unwrap_or_else(|| "openai".to_string()),
                model: db_agent
                    .model_name
                    .clone()
                    .unwrap_or_else(|| "gpt-4".to_string()),
                temperature: 0.7,
                max_tokens: 2048,
                top_p: 1.0,
            },
            memory: beebotos_agents::MemoryConfig {
                episodic_capacity: 1000,
                semantic_capacity: 5000,
                working_memory_size: 10,
                consolidation_interval_hours: 24,
            },
            personality: beebotos_agents::PersonalityConfig {
                openness: 0.5,
                conscientiousness: 0.5,
                extraversion: 0.5,
                agreeableness: 0.5,
                neuroticism: 0.5,
                base_mood: "neutral".to_string(),
            },
            intent_confidence_threshold: Some(0.7),
        };

        let mut agent = beebotos_agents::Agent::new(agent_config);
        if let Some(sink) = react_trace_sink.clone() {
            agent = agent.with_react_trace_sink(sink);
        }

        // Attach kernel WASM sandbox if available
        if let Some(ref kernel) = kernel {
            agent = agent.with_kernel(kernel.clone());
        }

        // Attach memory system if available
        if let Some(ref memory) = memory_system {
            agent = agent.with_memory_system(memory.clone());
            info!("Agent memory system attached for agent {}", agent_id);
        }

        // Attach skill registry (use externally provided one if available)
        if let Some(registry) = skill_registry {
            agent = agent.with_skill_registry(registry);
        }

        // Attach wallet if blockchain is enabled and a mnemonic is provided
        if config.blockchain.enabled {
            if let Some(ref mnemonic) = config.blockchain.agent_wallet_mnemonic {
                let wallet_config = beebotos_agents::wallet::WalletConfig {
                    chain_id: config.blockchain.chain_id,
                    derivation_path_prefix: "m/44'/60'/0'/0".to_string(),
                    default_account_index: 0,
                    rpc_url: config.blockchain.rpc_url.clone(),
                    default_gas_limit: 100_000,
                    max_priority_fee_gwei: None,
                    min_tx_interval_secs: 1,
                    incoming_transfer_poll_interval_secs: 15,
                };
                match beebotos_agents::wallet::AgentWallet::from_mnemonic_with_provider(
                    mnemonic,
                    wallet_config,
                )
                .await
                {
                    Ok(wallet) => {
                        agent = agent.with_wallet(Arc::new(wallet));
                        info!("Agent wallet attached for agent {}", agent_id);
                    }
                    Err(e) => {
                        warn!("Failed to initialize agent wallet for {}: {}", agent_id, e);
                    }
                }
            }
        }

        // Attach LLM interface
        let mut gateway_llm_interface = GatewayLLMInterface::new(llm_service);
        if let Some(sink) = react_trace_sink {
            gateway_llm_interface = gateway_llm_interface.with_react_trace_sink(sink);
        }
        let llm_interface: Arc<dyn beebotos_agents::communication::LLMCallInterface> =
            Arc::new(gateway_llm_interface);
        agent = agent.with_llm_interface(llm_interface);

        // Initialize the agent
        if let Err(e) = agent.initialize().await {
            error!("Failed to initialize agent {}: {}", agent_id, e);
            return Err(e);
        }

        Ok(Self {
            agent,
            created_at: chrono::Utc::now(),
        })
    }

    /// Execute a task on this agent instance
    pub async fn execute_task(
        &mut self,
        task: beebotos_agents::Task,
    ) -> Result<beebotos_agents::TaskResult, beebotos_agents::error::AgentError> {
        self.agent.execute_task(task).await
    }

    /// Shutdown the agent
    pub async fn shutdown(&mut self) -> Result<(), beebotos_agents::error::AgentError> {
        self.agent.shutdown().await
    }
}

/// Manages agent runtimes using unified state manager as single source of
/// truth.
///
/// 🔒 P0 FIX: Now fully integrated with AgentStateManager. The agent instances
/// are stored in the state manager's metadata, eliminating duplicate state.
pub struct AgentRuntimeManager {
    /// Kernel reference
    kernel: Option<Arc<beebotos_kernel::Kernel>>,
    /// Unified state manager (single source of truth)
    state_manager: beebotos_agents::StateManagerHandle,
    /// Application configuration
    config: BeeBotOSConfig,
    /// LLM service for agent operations
    llm_service: Arc<crate::services::llm_service::LlmService>,
    /// Memory system for agent memory
    memory_system: Option<Arc<beebotos_agents::memory::UnifiedMemorySystem>>,
    /// Skill registry for agent skill execution
    skill_registry: Option<Arc<beebotos_agents::skills::SkillRegistry>>,
    /// Optional sink for live ReAct/tool trace events.
    react_trace_sink: Option<Arc<dyn beebotos_agents::ReActTraceSink>>,
}

impl AgentRuntimeManager {
    /// Create a new runtime manager with the given kernel, state manager and
    /// LLM service
    pub fn new(
        kernel: Option<Arc<beebotos_kernel::Kernel>>,
        state_manager: beebotos_agents::StateManagerHandle,
        config: BeeBotOSConfig,
        llm_service: Arc<crate::services::llm_service::LlmService>,
        memory_system: Option<Arc<beebotos_agents::memory::UnifiedMemorySystem>>,
        skill_registry: Option<Arc<beebotos_agents::skills::SkillRegistry>>,
    ) -> Self {
        Self {
            kernel,
            state_manager,
            config,
            llm_service,
            memory_system,
            skill_registry,
            react_trace_sink: None,
        }
    }

    pub fn with_react_trace_sink(mut self, sink: Arc<dyn beebotos_agents::ReActTraceSink>) -> Self {
        self.react_trace_sink = Some(sink);
        self
    }

    /// Create a new runtime manager with a default state manager and shared LLM
    /// service
    pub async fn new_with_default_state_manager(
        kernel: Option<Arc<beebotos_kernel::Kernel>>,
        config: BeeBotOSConfig,
        llm_service: Arc<crate::services::llm_service::LlmService>,
        memory_system: Option<Arc<beebotos_agents::memory::UnifiedMemorySystem>>,
        skill_registry: Option<Arc<beebotos_agents::skills::SkillRegistry>>,
    ) -> Result<Self, crate::error::AppError> {
        let state_manager = Arc::new(beebotos_agents::AgentStateManager::new(None));
        Ok(Self::new(
            kernel,
            state_manager,
            config,
            llm_service,
            memory_system,
            skill_registry,
        ))
    }

    /// Get the LLM service
    pub fn llm_service(&self) -> Arc<crate::services::llm_service::LlmService> {
        self.llm_service.clone()
    }

    /// Get the state manager handle
    pub fn state_manager(&self) -> beebotos_agents::StateManagerHandle {
        self.state_manager.clone()
    }

    /// Register a new agent runtime after it has been persisted to the
    /// database.
    pub async fn register_agent(
        &self,
        agent_id: &str,
        db_agent: &crate::models::AgentRecord,
    ) -> Result<(), AppError> {
        // Check if already registered in state manager
        if self.state_manager.is_registered(agent_id).await {
            warn!("Agent {} is already registered in state manager", agent_id);
            return Ok(());
        }

        // Build metadata with agent instance holder
        let mut metadata = HashMap::new();
        metadata.insert("name".to_string(), db_agent.name.clone());
        metadata.insert(
            "model_provider".to_string(),
            db_agent.model_provider.clone().unwrap_or_default(),
        );
        metadata.insert(
            "model_name".to_string(),
            db_agent.model_name.clone().unwrap_or_default(),
        );
        metadata.insert("has_instance".to_string(), "false".to_string());

        // Register in state manager first
        self.state_manager
            .register_agent(agent_id, metadata)
            .await
            .map_err(|e| {
                AppError::Internal(format!("Failed to register agent in state manager: {}", e))
            })?;

        // Transition to Initializing
        self.state_manager
            .transition(agent_id, beebotos_agents::StateTransition::Start)
            .await
            .map_err(|e| AppError::Internal(format!("State transition failed: {}", e)))?;

        // Create and initialize agent instance
        let instance = AgentInstance::new(
            agent_id,
            db_agent,
            &self.config,
            self.kernel.clone(),
            self.llm_service.clone(),
            self.memory_system.clone(),
            self.skill_registry.clone(),
            self.react_trace_sink.clone(),
        )
        .await
        .map_err(|e| {
            let _ = self.state_manager.transition(
                agent_id,
                beebotos_agents::StateTransition::Error {
                    message: e.to_string(),
                },
            );
            AppError::Internal(format!("Agent initialization failed: {}", e))
        })?;

        // Store instance in state manager metadata
        // Note: Since AgentInstance is not Send, we use a separate storage
        // This is a compromise - in production, consider using a proper
        // instance registry with Arc<Mutex<AgentInstance>>
        let _instance_key = format!("instance:{}", agent_id);
        let instance_arc = Arc::new(tokio::sync::Mutex::new(instance));

        // Store in a global instance registry
        INSTANCE_REGISTRY
            .lock()
            .await
            .insert(agent_id.to_string(), instance_arc.clone());

        // Update metadata
        self.state_manager
            .update_metadata(agent_id, "has_instance", "true")
            .await
            .ok();

        // Transition to Idle
        self.state_manager
            .transition(
                agent_id,
                beebotos_agents::StateTransition::InitializationComplete,
            )
            .await
            .map_err(|e| AppError::Internal(format!("State transition failed: {}", e)))?;

        info!("Registered beebotos_agents::Agent runtime for {}", agent_id);
        Ok(())
    }

    /// Execute a task on a registered agent runtime.
    pub async fn execute_task(
        &self,
        agent_id: &str,
        task: beebotos_agents::Task,
    ) -> Result<beebotos_agents::TaskResult, AppError> {
        // Check state before executing
        let state = self
            .state_manager
            .get_state(agent_id)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to get agent state: {}", e)))?;

        if state != beebotos_agents::AgentState::Idle {
            return Err(AppError::Internal(format!(
                "Agent {} is not idle (current state: {})",
                agent_id, state
            )));
        }

        // Get agent instance from registry
        let registry = INSTANCE_REGISTRY.lock().await;
        let instance_arc = registry
            .get(agent_id)
            .cloned()
            .ok_or_else(|| AppError::not_found("Agent runtime", agent_id))?;
        drop(registry);

        // Transition to Working
        let task_id = task.id.clone();
        self.state_manager
            .transition(
                agent_id,
                beebotos_agents::StateTransition::BeginTask { task_id },
            )
            .await
            .map_err(|e| AppError::Internal(format!("State transition failed: {}", e)))?;

        // Execute task
        let mut instance = instance_arc.lock().await;
        let result = instance.execute_task(task).await;
        drop(instance);

        // Handle result and transition
        let success = result.is_ok();
        let transition = if success {
            beebotos_agents::StateTransition::CompleteTask { success: true }
        } else {
            beebotos_agents::StateTransition::CompleteTask { success: false }
        };

        if let Err(e) = self.state_manager.transition(agent_id, transition).await {
            warn!("Failed to transition agent {} state: {}", agent_id, e);
        }

        // Update stats
        if let Ok(ref r) = result {
            self.state_manager
                .update_stats(agent_id, r.execution_time_ms)
                .await
                .ok();
        }

        result.map_err(|e| {
            error!("Agent {} task execution failed: {}", agent_id, e);
            AppError::Internal(format!("Agent task execution failed: {}", e))
        })
    }

    /// Remove an agent runtime and trigger graceful shutdown.
    pub async fn unregister_agent(&self, agent_id: &str) {
        // Check if registered
        if !self.state_manager.is_registered(agent_id).await {
            return;
        }

        // Transition to ShuttingDown
        if let Err(e) = self
            .state_manager
            .transition(agent_id, beebotos_agents::StateTransition::Shutdown)
            .await
        {
            warn!(
                "Failed to transition agent {} to shutting_down: {}",
                agent_id, e
            );
        }

        // Shutdown agent instance
        let mut registry = INSTANCE_REGISTRY.lock().await;
        if let Some(instance_arc) = registry.remove(agent_id) {
            drop(registry); // Release lock before async operation

            let mut instance = instance_arc.lock().await;
            if let Err(e) = instance.shutdown().await {
                warn!("Error shutting down agent {}: {}", agent_id, e);
            } else {
                info!("Shut down agent runtime {}", agent_id);
            }
        }

        // Transition to Stopped
        if let Err(e) = self
            .state_manager
            .transition(agent_id, beebotos_agents::StateTransition::Stopped)
            .await
        {
            warn!("Failed to transition agent {} to stopped: {}", agent_id, e);
        }

        // Unregister from state manager
        if let Err(e) = self.state_manager.unregister_agent(agent_id).await {
            warn!(
                "Failed to unregister agent {} from state manager: {}",
                agent_id, e
            );
        }
    }

    /// Check whether a runtime is registered.
    pub async fn is_registered(&self, agent_id: &str) -> bool {
        self.state_manager.is_registered(agent_id).await
    }

    /// Get agent state
    pub async fn get_agent_state(
        &self,
        agent_id: &str,
    ) -> Result<beebotos_agents::AgentState, AppError> {
        self.state_manager
            .get_state(agent_id)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to get agent state: {}", e)))
    }

    /// Get agent state record
    pub async fn get_agent_record(
        &self,
        agent_id: &str,
    ) -> Result<beebotos_agents::AgentStateRecord, AppError> {
        self.state_manager
            .get_record(agent_id)
            .await
            .map_err(|_e| AppError::not_found("Agent", agent_id))
    }

    /// Update kernel task ID for an agent
    pub async fn set_kernel_task_id(&self, agent_id: &str, task_id: u64) -> Result<(), AppError> {
        self.state_manager
            .set_kernel_task_id(agent_id, task_id)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to set kernel task ID: {}", e)))
    }

    /// Get kernel task ID for an agent
    pub async fn get_kernel_task_id(&self, agent_id: &str) -> Result<Option<u64>, AppError> {
        self.state_manager
            .get_kernel_task_id(agent_id)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to get kernel task ID: {}", e)))
    }

    /// List all registered agent IDs
    pub async fn list_agents(&self) -> Vec<String> {
        self.state_manager.list_agents().await
    }

    /// List agents in a specific state
    pub async fn list_agents_in_state(&self, state: beebotos_agents::AgentState) -> Vec<String> {
        self.state_manager.list_agents_in_state(state).await
    }

    /// Subscribe to state changes
    pub async fn subscribe_to_state_changes(
        &self,
    ) -> tokio::sync::mpsc::UnboundedReceiver<beebotos_agents::StateChangeEvent> {
        self.state_manager.subscribe().await
    }
}

use std::collections::HashMap as StdHashMap;

use tokio::sync::Mutex;

/// Global instance registry for agent instances
///
/// 🔒 P0 FIX: This replaces the per-Manager HashMap, ensuring all instances
/// are managed in one place and can be accessed across the application.
static INSTANCE_REGISTRY: once_cell::sync::Lazy<
    Mutex<StdHashMap<String, Arc<Mutex<AgentInstance>>>>,
> = once_cell::sync::Lazy::new(|| Mutex::new(StdHashMap::new()));

/// Get a global instance of the runtime manager
#[allow(dead_code)]
pub async fn get_global_instance_registry(
) -> &'static Mutex<StdHashMap<String, Arc<Mutex<AgentInstance>>>> {
    &INSTANCE_REGISTRY
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_instance_registry() {
        let registry = get_global_instance_registry().await;
        let map = registry.lock().await;
        assert!(map.is_empty());
    }
}
