use std::sync::Arc;

use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::config::VoiceMarketingConfig;
use crate::error::GatewayError;
use crate::services::llm_service::LlmService;

type HmacSha256 = Hmac<Sha256>;

const ALIYUN_ALGORITHM: &str = "ACS3-HMAC-SHA256";
const VOICE_SINGLE_CALL_ACTION: &str = "VoiceSingleCall";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductMarketingInput {
    pub product_name: String,
    pub product_description: String,
    #[serde(default)]
    pub price: Option<String>,
    #[serde(default)]
    pub offer: Option<String>,
    #[serde(default)]
    pub audience: Option<String>,
    #[serde(default)]
    pub goal: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketingScript {
    pub script: String,
    pub follow_up_text: String,
    pub template_params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVoiceMarketingCall {
    pub phone_number: String,
    pub script: String,
    #[serde(default)]
    pub template_params: Value,
    #[serde(default)]
    pub tts_code: Option<String>,
    #[serde(default)]
    pub caller_id_number: Option<String>,
    #[serde(default)]
    pub task_name: Option<String>,
    #[serde(default)]
    pub confirmed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceMarketingCallRecord {
    pub id: String,
    pub phone_number: String,
    pub status: String,
    pub provider: String,
    pub provider_request_id: Option<String>,
    pub provider_code: Option<String>,
    pub provider_message: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceSingleCallRequest {
    pub called_number: String,
    pub tts_code: String,
    pub tts_param: Value,
    pub caller_id_number: Option<String>,
    pub country_id: Option<String>,
    pub out_id: Option<String>,
    pub task_name: Option<String>,
    pub play_times: Option<u8>,
    pub volume: Option<u8>,
    pub speed: Option<i16>,
}

impl VoiceSingleCallRequest {
    pub fn to_query_params(&self) -> Vec<(String, String)> {
        let mut params = vec![
            ("CalledNumber".to_string(), self.called_number.clone()),
            ("TtsCode".to_string(), self.tts_code.clone()),
            ("TtsParam".to_string(), self.tts_param.to_string()),
        ];

        push_optional(
            &mut params,
            "CallerIdNumber",
            self.caller_id_number.as_deref(),
        );
        push_optional(&mut params, "CountryId", self.country_id.as_deref());
        push_optional(&mut params, "OutId", self.out_id.as_deref());
        push_optional(&mut params, "TaskName", self.task_name.as_deref());
        push_optional_value(&mut params, "PlayTimes", self.play_times);
        push_optional_value(&mut params, "Volume", self.volume);
        push_optional_value(&mut params, "Speed", self.speed);
        params
    }
}

fn push_optional(params: &mut Vec<(String, String)>, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        if !value.trim().is_empty() {
            params.push((key.to_string(), value.trim().to_string()));
        }
    }
}

fn push_optional_value<T: ToString>(
    params: &mut Vec<(String, String)>,
    key: &str,
    value: Option<T>,
) {
    if let Some(value) = value {
        params.push((key.to_string(), value.to_string()));
    }
}

#[derive(Debug, Clone)]
pub struct AliyunSignedRequest {
    method: String,
    host: String,
    action: String,
    version: String,
    date: String,
    nonce: String,
    query_params: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliyunSignedOutput {
    pub authorization: String,
    pub canonical_query: String,
    pub headers: Vec<(String, String)>,
}

impl AliyunSignedRequest {
    pub fn new(
        method: impl Into<String>,
        host: impl Into<String>,
        action: impl Into<String>,
        version: impl Into<String>,
        date: impl Into<String>,
        nonce: impl Into<String>,
        query_params: Vec<(String, String)>,
    ) -> Self {
        Self {
            method: method.into(),
            host: host.into(),
            action: action.into(),
            version: version.into(),
            date: date.into(),
            nonce: nonce.into(),
            query_params,
        }
    }

    pub fn sign(&self, access_key_id: &str, access_key_secret: &str) -> AliyunSignedOutput {
        let hashed_payload = sha256_hex("");
        let mut headers = vec![
            ("host".to_string(), self.host.clone()),
            ("x-acs-action".to_string(), self.action.clone()),
            ("x-acs-content-sha256".to_string(), hashed_payload.clone()),
            ("x-acs-date".to_string(), self.date.clone()),
            ("x-acs-signature-nonce".to_string(), self.nonce.clone()),
            ("x-acs-version".to_string(), self.version.clone()),
        ];
        headers.sort_by(|a, b| a.0.cmp(&b.0));

        let canonical_query = canonical_query(&self.query_params);
        let canonical_headers = headers
            .iter()
            .map(|(key, value)| format!("{}:{}\n", key.to_ascii_lowercase(), value.trim()))
            .collect::<String>();
        let signed_headers = headers
            .iter()
            .map(|(key, _)| key.to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join(";");
        let canonical_request = format!(
            "{}\n/\n{}\n{}\n{}\n{}",
            self.method.to_ascii_uppercase(),
            canonical_query,
            canonical_headers,
            signed_headers,
            hashed_payload
        );
        let string_to_sign = format!("{}\n{}", ALIYUN_ALGORITHM, sha256_hex(&canonical_request));
        let signature = hmac_sha256_hex(access_key_secret.as_bytes(), &string_to_sign);
        let authorization = format!(
            "{} Credential={},SignedHeaders={},Signature={}",
            ALIYUN_ALGORITHM, access_key_id, signed_headers, signature
        );

        AliyunSignedOutput {
            authorization,
            canonical_query,
            headers,
        }
    }
}

fn canonical_query(params: &[(String, String)]) -> String {
    let mut encoded = params
        .iter()
        .map(|(key, value)| (percent_encode(key), percent_encode(value)))
        .collect::<Vec<_>>();
    encoded.sort();
    encoded
        .into_iter()
        .map(|(key, value)| format!("{}={}", key, value))
        .collect::<Vec<_>>()
        .join("&")
}

fn percent_encode(value: &str) -> String {
    urlencoding::encode(value).into_owned()
}

fn sha256_hex(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hex::encode(hasher.finalize())
}

fn hmac_sha256_hex(key: &[u8], value: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(value.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

#[derive(Clone)]
pub struct AliyunVoiceClient {
    config: VoiceMarketingConfig,
    http: reqwest::Client,
}

#[derive(Debug, Clone, Deserialize)]
struct AliyunVoiceSingleCallResponse {
    #[serde(rename = "RequestId")]
    request_id: Option<String>,
    #[serde(rename = "Code")]
    code: Option<String>,
    #[serde(rename = "Message")]
    message: Option<String>,
    #[serde(rename = "Success")]
    success: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct AliyunVoiceCallResult {
    pub request_id: Option<String>,
    pub code: Option<String>,
    pub message: Option<String>,
    pub success: bool,
    pub raw_response: Value,
}

impl AliyunVoiceClient {
    pub fn new(config: VoiceMarketingConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
        }
    }

    pub async fn voice_single_call(
        &self,
        request: VoiceSingleCallRequest,
    ) -> Result<AliyunVoiceCallResult, GatewayError> {
        let access_key_id = self.config.access_key_id.as_deref().ok_or_else(|| {
            GatewayError::service_unavailable("aliyun_voice", "missing access key id")
        })?;
        let access_key_secret = self.config.access_key_secret.as_deref().ok_or_else(|| {
            GatewayError::service_unavailable("aliyun_voice", "missing access key secret")
        })?;

        let date = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let nonce = Uuid::new_v4().simple().to_string();
        let signed = AliyunSignedRequest::new(
            "POST",
            &self.config.endpoint,
            VOICE_SINGLE_CALL_ACTION,
            &self.config.api_version,
            date,
            nonce,
            request.to_query_params(),
        )
        .sign(access_key_id, access_key_secret);

        let url = Url::parse(&format!(
            "https://{}/?{}",
            self.config.endpoint, signed.canonical_query
        ))
        .map_err(|e| GatewayError::internal(format!("Invalid Aliyun endpoint: {}", e)))?;
        let mut builder = self
            .http
            .post(url)
            .header("Authorization", signed.authorization)
            .header("accept", "application/json");

        for (key, value) in &signed.headers {
            builder = builder.header(key, value);
        }
        if let Some(token) = self.config.security_token.as_deref() {
            builder = builder.header("x-acs-security-token", token);
        }

        let response = builder
            .send()
            .await
            .map_err(|e| GatewayError::internal(format!("Aliyun voice request failed: {}", e)))?;
        let status = response.status();
        let raw_response = response
            .json::<Value>()
            .await
            .map_err(|e| GatewayError::internal(format!("Invalid Aliyun voice response: {}", e)))?;
        let parsed: AliyunVoiceSingleCallResponse = serde_json::from_value(raw_response.clone())
            .unwrap_or(AliyunVoiceSingleCallResponse {
                request_id: None,
                code: None,
                message: None,
                success: None,
            });
        let success = status.is_success()
            && parsed.success.unwrap_or(false)
            && parsed.code.as_deref().unwrap_or_default() == "SUCCESS";

        Ok(AliyunVoiceCallResult {
            request_id: parsed.request_id,
            code: parsed.code,
            message: parsed.message,
            success,
            raw_response,
        })
    }
}

#[derive(Clone)]
pub struct VoiceMarketingService {
    db: SqlitePool,
    config: VoiceMarketingConfig,
    llm_service: Arc<LlmService>,
    aliyun_client: AliyunVoiceClient,
}

impl VoiceMarketingService {
    pub async fn new(
        db: SqlitePool,
        config: VoiceMarketingConfig,
        llm_service: Arc<LlmService>,
    ) -> Result<Self, GatewayError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS voice_marketing_calls (
                id TEXT PRIMARY KEY,
                phone_number TEXT NOT NULL,
                script TEXT NOT NULL,
                template_params TEXT NOT NULL,
                status TEXT NOT NULL,
                provider TEXT NOT NULL,
                provider_request_id TEXT,
                provider_code TEXT,
                provider_message TEXT,
                provider_response TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&db)
        .await
        .map_err(|e| {
            GatewayError::internal(format!(
                "Failed to initialize voice marketing schema: {}",
                e
            ))
        })?;

        Ok(Self {
            db,
            aliyun_client: AliyunVoiceClient::new(config.clone()),
            config,
            llm_service,
        })
    }

    pub async fn generate_script(
        &self,
        input: ProductMarketingInput,
    ) -> Result<MarketingScript, GatewayError> {
        let prompt = format!(
            "Generate a concise old-customer phone marketing script in Chinese. Return strict \
             JSON with keys: script, follow_up_text, template_params. Keep script under 120 \
             Chinese characters and include keypad choices: press 1 interested, press 2 contact \
             later, press 3 do not contact. Product: {}. Description: {}. Price: {}. Offer: {}. \
             Audience: {}. Goal: {}.",
            input.product_name.trim(),
            input.product_description.trim(),
            input.price.as_deref().unwrap_or(""),
            input.offer.as_deref().unwrap_or(""),
            input.audience.as_deref().unwrap_or("old customers"),
            input.goal.as_deref().unwrap_or("confirm purchase interest")
        );
        let messages = vec![
            beebotos_agents::llm::Message::system(
                "You write compliant, factual phone scripts for customers who agreed to be \
                 contacted.",
            ),
            beebotos_agents::llm::Message::user(prompt),
        ];
        let content = self
            .llm_service
            .chat(messages, Some(800), None, None, None)
            .await?;

        Ok(
            parse_script_response(&content).unwrap_or_else(|| MarketingScript {
                script: content.trim().to_string(),
                follow_up_text: format!(
                    "{}：感谢接听，您可回复 1 了解详情，回复 3 取消后续联系。",
                    input.product_name.trim()
                ),
                template_params: json!({
                    "product": input.product_name.trim(),
                    "script": content.trim()
                }),
            }),
        )
    }

    pub async fn create_call(
        &self,
        req: CreateVoiceMarketingCall,
    ) -> Result<VoiceMarketingCallRecord, GatewayError> {
        if !req.confirmed {
            return Err(GatewayError::bad_request(
                "Voice marketing script must be confirmed before calling",
            ));
        }
        if !self.config.enabled {
            return Err(GatewayError::service_unavailable(
                "aliyun_voice",
                "voice marketing is disabled",
            ));
        }

        let id = format!("vm_{}", Uuid::new_v4().simple());
        let now = Utc::now();
        let phone_number = normalize_mainland_phone(&req.phone_number)?;
        let tts_code = req
            .tts_code
            .or_else(|| self.config.tts_code.clone())
            .ok_or_else(|| {
                GatewayError::service_unavailable("aliyun_voice", "missing TTS template code")
            })?;
        let template_params = merge_template_params(req.template_params, &req.script);

        sqlx::query(
            r#"
            INSERT INTO voice_marketing_calls
                (id, phone_number, script, template_params, status, provider, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, 'created', 'aliyun', ?5, ?6)
            "#,
        )
        .bind(&id)
        .bind(&phone_number)
        .bind(&req.script)
        .bind(template_params.to_string())
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(&self.db)
        .await
        .map_err(|e| GatewayError::internal(format!("Failed to store voice marketing call: {}", e)))?;

        let aliyun_request = VoiceSingleCallRequest {
            called_number: phone_number.clone(),
            tts_code,
            tts_param: template_params.clone(),
            caller_id_number: req
                .caller_id_number
                .or_else(|| self.config.caller_id_number.clone()),
            country_id: Some(self.config.country_id.clone()),
            out_id: Some(id.chars().take(15).collect()),
            task_name: req
                .task_name
                .or_else(|| Some("beebotos_marketing".to_string())),
            play_times: Some(self.config.play_times),
            volume: None,
            speed: None,
        };
        let result = self.aliyun_client.voice_single_call(aliyun_request).await?;
        let status = if result.success {
            "submitted"
        } else {
            "provider_failed"
        };

        sqlx::query(
            r#"
            UPDATE voice_marketing_calls
            SET status = ?1,
                provider_request_id = ?2,
                provider_code = ?3,
                provider_message = ?4,
                provider_response = ?5,
                updated_at = ?6
            WHERE id = ?7
            "#,
        )
        .bind(status)
        .bind(&result.request_id)
        .bind(&result.code)
        .bind(&result.message)
        .bind(result.raw_response.to_string())
        .bind(Utc::now().to_rfc3339())
        .bind(&id)
        .execute(&self.db)
        .await
        .map_err(|e| {
            GatewayError::internal(format!("Failed to update voice marketing call: {}", e))
        })?;

        Ok(VoiceMarketingCallRecord {
            id,
            phone_number,
            status: status.to_string(),
            provider: "aliyun".to_string(),
            provider_request_id: result.request_id,
            provider_code: result.code,
            provider_message: result.message,
            created_at: now,
        })
    }
}

fn parse_script_response(content: &str) -> Option<MarketingScript> {
    let trimmed = content.trim();
    let json_text = if trimmed.starts_with("```") {
        trimmed
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim()
    } else {
        trimmed
    };
    serde_json::from_str::<MarketingScript>(json_text).ok()
}

fn normalize_mainland_phone(value: &str) -> Result<String, GatewayError> {
    let digits = value
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect::<String>();
    let normalized = if digits.len() == 11 && digits.starts_with('1') {
        format!("86{}", digits)
    } else if digits.len() == 13 && digits.starts_with("86") {
        digits
    } else {
        return Err(GatewayError::bad_request(
            "phone_number must be a mainland China mobile number",
        ));
    };
    Ok(normalized)
}

fn merge_template_params(value: Value, script: &str) -> Value {
    match value {
        Value::Object(mut map) => {
            map.entry("script".to_string())
                .or_insert_with(|| Value::String(script.to_string()));
            Value::Object(map)
        }
        _ => json!({ "script": script }),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn voice_marketing_aliyun_v3_signature_matches_official_sample() {
        let request = AliyunSignedRequest::new(
            "POST",
            "ecs.cn-shanghai.aliyuncs.com",
            "RunInstances",
            "2014-05-26",
            "2023-10-26T10:22:32Z",
            "3156853299f313e23d1673dc12e1703d",
            vec![
                (
                    "ImageId".to_string(),
                    "win2019_1809_x64_dtc_zh-cn_40G_alibase_20230811.vhd".to_string(),
                ),
                ("RegionId".to_string(), "cn-shanghai".to_string()),
            ],
        );

        let signed = request.sign("YourAccessKeyId", "YourAccessKeySecret");

        assert_eq!(
            signed.authorization,
            "ACS3-HMAC-SHA256 \
             Credential=YourAccessKeyId,SignedHeaders=host;x-acs-action;x-acs-content-sha256;\
             x-acs-date;x-acs-signature-nonce;x-acs-version,\
             Signature=06563a9e1b43f5dfe96b81484da74bceab24a1d853912eee15083a6f0f3283c0"
        );
    }

    #[test]
    fn voice_single_call_query_contains_template_parameters() {
        let request = VoiceSingleCallRequest {
            called_number: "8613800138000".to_string(),
            tts_code: "TTS_001".to_string(),
            tts_param: json!({"product":"BeeBotOS","action":"按1了解详情"}),
            caller_id_number: Some("01012345678".to_string()),
            country_id: Some("CN".to_string()),
            out_id: Some("call_abcd1234".to_string()),
            task_name: Some("old_customer_marketing".to_string()),
            play_times: Some(1),
            volume: None,
            speed: None,
        };

        let query = request.to_query_params();

        assert!(query.contains(&("CalledNumber".to_string(), "8613800138000".to_string())));
        assert!(query.contains(&("TtsCode".to_string(), "TTS_001".to_string())));
        assert!(query.contains(&("CallerIdNumber".to_string(), "01012345678".to_string())));
        assert!(query.contains(&("CountryId".to_string(), "CN".to_string())));
        assert!(query.contains(&("OutId".to_string(), "call_abcd1234".to_string())));
        assert!(query.contains(&("PlayTimes".to_string(), "1".to_string())));
        assert!(query
            .iter()
            .any(|(key, value)| key == "TtsParam" && value.contains("BeeBotOS")));
    }
}
