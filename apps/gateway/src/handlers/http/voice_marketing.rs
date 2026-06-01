use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::error::GatewayError;
use crate::services::voice_marketing::{
    CreateVoiceMarketingCall, MarketingScript, ProductMarketingInput, VoiceMarketingCallRecord,
    VoiceMarketingService,
};
use crate::AppState;

#[derive(Debug, Serialize)]
pub struct VoiceMarketingScriptResponse {
    pub data: MarketingScript,
}

#[derive(Debug, Serialize)]
pub struct VoiceMarketingCallResponse {
    pub data: VoiceMarketingCallRecord,
}

pub async fn generate_script(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ProductMarketingInput>,
) -> Result<Json<VoiceMarketingScriptResponse>, GatewayError> {
    let service = service_from_state(state).await?;
    let data = service.generate_script(req).await?;
    Ok(Json(VoiceMarketingScriptResponse { data }))
}

pub async fn create_call(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateVoiceMarketingCall>,
) -> Result<Json<VoiceMarketingCallResponse>, GatewayError> {
    let service = service_from_state(state).await?;
    let data = service.create_call(req).await?;
    Ok(Json(VoiceMarketingCallResponse { data }))
}

async fn service_from_state(state: Arc<AppState>) -> Result<VoiceMarketingService, GatewayError> {
    VoiceMarketingService::new(
        state.db.clone(),
        state.config.voice_marketing.clone(),
        state.llm_service.clone(),
    )
    .await
}
