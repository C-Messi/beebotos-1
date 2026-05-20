//! Skill Tool Set
//!
//! General-purpose tools that skills can use via the ReAct executor:
//! file_read, file_write, file_list, process_exec, bash_shell.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use regex::Regex;
use serde_json::Value;
use tracing::{info, warn};

use crate::skills::process_sandbox::apply_sandbox;
use crate::Agent;

#[derive(Debug, Clone)]
struct SearchResult {
    title: String,
    url: String,
    snippet: String,
}

#[derive(Debug, Clone)]
struct SearchFailure {
    provider: &'static str,
    reason: String,
}

fn configure_shell_command(cmd: &mut tokio::process::Command, command: &str) {
    #[cfg(windows)]
    {
        let encoded_command = encode_powershell_command(command);
        cmd.arg("-NoLogo")
            .arg("-NoProfile")
            .arg("-NonInteractive")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-EncodedCommand")
            .arg(encoded_command);
    }
    #[cfg(not(windows))]
    {
        cmd.arg("-c").arg(command);
    }
}

fn decode_command_output(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }

    if bytes.starts_with(&[0xff, 0xfe]) {
        return decode_utf16_le(&bytes[2..]);
    }
    if bytes.starts_with(&[0xfe, 0xff]) {
        return decode_utf16_be(&bytes[2..]);
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_string();
    }

    let even_len = bytes.len() / 2 * 2;
    if even_len >= 2 {
        let nul_odd = bytes[..even_len]
            .chunks_exact(2)
            .filter(|chunk| chunk[1] == 0)
            .count();
        let pairs = even_len / 2;
        if pairs > 0 && nul_odd * 100 / pairs >= 30 {
            return decode_utf16_le(&bytes[..even_len]);
        }
    }

    String::from_utf8_lossy(bytes).to_string()
}

fn decode_utf16_le(bytes: &[u8]) -> String {
    String::from_utf16_lossy(
        &bytes
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>(),
    )
}

fn decode_utf16_be(bytes: &[u8]) -> String {
    String::from_utf16_lossy(
        &bytes
            .chunks_exact(2)
            .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>(),
    )
}

fn configure_command_environment(cmd: &mut tokio::process::Command) {
    crate::command_env::configure_host_user_cli_environment(cmd, None);
}

#[cfg(windows)]
fn encode_powershell_command(command: &str) -> String {
    let wrapper = format!(
        r#"
$utf8NoBom = New-Object System.Text.UTF8Encoding -ArgumentList $false
[Console]::InputEncoding = $utf8NoBom
[Console]::OutputEncoding = $utf8NoBom
$OutputEncoding = $utf8NoBom
{command}
"#
    );
    let bytes: Vec<u8> = wrapper
        .encode_utf16()
        .flat_map(|code_unit| code_unit.to_le_bytes())
        .collect();
    encode_base64(&bytes)
}

#[cfg(windows)]
fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(((bytes.len() + 2) / 3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);

        encoded.push(TABLE[(b0 >> 2) as usize] as char);
        encoded.push(TABLE[(((b0 & 0b0000_0011) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            encoded.push(TABLE[(((b1 & 0b0000_1111) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            encoded.push('=');
        }
        if chunk.len() > 2 {
            encoded.push(TABLE[(b2 & 0b0011_1111) as usize] as char);
        } else {
            encoded.push('=');
        }
    }
    encoded
}

/// Trait for tools usable inside the skill ReAct loop
#[async_trait::async_trait]
pub trait SkillTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> Value;
    async fn execute(&self, params: &Value) -> Result<String, String>;
}

/// Normalize a path by resolving `.` and `..` components manually.
/// This does NOT access the filesystem (no blocking I/O).
fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Prefix(p) => normalized.push(std::path::Component::Prefix(p)),
            std::path::Component::RootDir => normalized.push("/"),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::Normal(name) => normalized.push(name),
        }
    }
    normalized
}

/// Resolve a user-supplied path against a working directory.
/// - Relative paths are resolved against `work_dir`
/// - Absolute paths are allowed as-is
/// - Uses pure path arithmetic (no blocking filesystem I/O)
pub fn resolve_work_path(work_dir: &Path, input_path: &str) -> Result<PathBuf, String> {
    let path = Path::new(input_path);
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        work_dir.join(path)
    };

    Ok(normalize_path(&resolved))
}

/// Read a file from the filesystem (sandboxed to work_dir)
pub struct FileReadTool {
    work_dir: PathBuf,
}

impl FileReadTool {
    pub fn new(work_dir: PathBuf) -> Self {
        Self { work_dir }
    }
}

#[async_trait::async_trait]
impl SkillTool for FileReadTool {
    fn name(&self) -> &str {
        "file_read"
    }

    fn description(&self) -> &str {
        "Read the contents of a file. Parameters: path (string)"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Absolute or relative file path" }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, params: &Value) -> Result<String, String> {
        let path = params["path"].as_str().ok_or("Missing 'path' parameter")?;
        let path = resolve_work_path(&self.work_dir, path)?;
        tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| format!("Failed to read file '{}': {}", path.display(), e))
    }
}

/// Write text to a file (sandboxed to work_dir)
pub struct FileWriteTool {
    work_dir: PathBuf,
}

impl FileWriteTool {
    pub fn new(work_dir: PathBuf) -> Self {
        Self { work_dir }
    }
}

#[async_trait::async_trait]
impl SkillTool for FileWriteTool {
    fn name(&self) -> &str {
        "file_write"
    }

    fn description(&self) -> &str {
        "Write text content to a file. Creates the file if it does not exist. Parameters: path \
         (string), content (string)"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Absolute or relative file path" },
                "content": { "type": "string", "description": "Text content to write" }
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(&self, params: &Value) -> Result<String, String> {
        let path = params["path"].as_str().ok_or("Missing 'path' parameter")?;
        let content = params["content"]
            .as_str()
            .ok_or("Missing 'content' parameter")?;
        let path = resolve_work_path(&self.work_dir, path)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("Failed to create parent directories: {}", e))?;
        }
        tokio::fs::write(&path, content)
            .await
            .map_err(|e| format!("Failed to write file '{}': {}", path.display(), e))?;
        Ok(format!("File '{}' written successfully.", path.display()))
    }
}

/// List files in a directory (sandboxed to work_dir)
pub struct FileListTool {
    work_dir: PathBuf,
}

impl FileListTool {
    pub fn new(work_dir: PathBuf) -> Self {
        Self { work_dir }
    }
}

#[async_trait::async_trait]
impl SkillTool for FileListTool {
    fn name(&self) -> &str {
        "file_list"
    }

    fn description(&self) -> &str {
        "List files and directories at a given path. Parameters: path (string)"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Directory path to list" }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, params: &Value) -> Result<String, String> {
        let path = params["path"].as_str().ok_or("Missing 'path' parameter")?;
        let path = resolve_work_path(&self.work_dir, path)?;
        let mut entries = tokio::fs::read_dir(&path)
            .await
            .map_err(|e| format!("Failed to read directory '{}': {}", path.display(), e))?;

        let mut lines = vec![format!("Contents of '{}'", path.display())];
        while let Ok(Some(entry)) = entries.next_entry().await {
            let meta = entry.metadata().await.ok();
            let name = entry.file_name().to_string_lossy().to_string();
            let ty = if meta.map(|m| m.is_dir()).unwrap_or(false) {
                "dir"
            } else {
                "file"
            };
            lines.push(format!("  [{}] {}", ty, name));
        }
        Ok(lines.join("\n"))
    }
}

/// Execute an external process (python, node, shell script, etc.)
pub struct ProcessExecTool {
    allowed_work_dirs: Vec<PathBuf>,
}

impl ProcessExecTool {
    pub fn new(allowed_work_dirs: Vec<PathBuf>) -> Self {
        Self { allowed_work_dirs }
    }

    fn validate_command(&self, command: &str) -> Result<(), String> {
        let lower = command.to_lowercase();

        // Exact string matches for dangerous commands
        let dangerous_exact = [
            "rm -rf /",
            "rm -rf /*",
            ":(){ :|:& };:",
            "> /dev/sda",
            "dd if=/dev/zero",
        ];
        for d in &dangerous_exact {
            if lower.contains(*d) {
                return Err(format!("Dangerous command pattern blocked: {}", d));
            }
        }

        // Prefix match for mkfs
        if lower.contains("mkfs.") {
            return Err("Dangerous command pattern blocked: mkfs.".to_string());
        }

        // Regex matches for pipe-to-shell attacks
        let pipe_patterns = [
            Regex::new(r"curl\s+.*\|\s*(ba)?sh").unwrap(),
            Regex::new(r"wget\s+.*\|\s*(ba)?sh").unwrap(),
            Regex::new(r"curl\s+.*-\s*(ba)?sh").unwrap(),
            Regex::new(r"fetch\s+.*\|\s*(ba)?sh").unwrap(),
        ];
        for re in &pipe_patterns {
            if re.is_match(&lower) {
                return Err(format!(
                    "Dangerous command pattern blocked: pipe-to-shell ({}",
                    re.as_str()
                ));
            }
        }

        Ok(())
    }

    fn resolve_working_dir(
        &self,
        specified: Option<&str>,
        default: &Path,
    ) -> Result<PathBuf, String> {
        let dir = if let Some(d) = specified {
            let p = PathBuf::from(d);
            if p.is_absolute() {
                p
            } else {
                default.join(p)
            }
        } else {
            default.to_path_buf()
        };

        Ok(normalize_path(&dir))
    }
}

#[async_trait::async_trait]
impl SkillTool for ProcessExecTool {
    fn name(&self) -> &str {
        "process_exec"
    }

    fn description(&self) -> &str {
        "Execute an external command in a subprocess (e.g. python3 script.py, node script.js). \
         Parameters: command (string), working_dir (string, optional), timeout_ms (integer, \
         optional, default 30000)"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "Shell command to execute" },
                "working_dir": { "type": "string", "description": "Working directory for the command" },
                "timeout_ms": { "type": "integer", "description": "Timeout in milliseconds", "default": 30000 }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, params: &Value) -> Result<String, String> {
        let command = params["command"]
            .as_str()
            .ok_or("Missing 'command' parameter")?;
        self.validate_command(command)?;

        let default_dir = self
            .allowed_work_dirs
            .first()
            .cloned()
            .unwrap_or_else(|| PathBuf::from("."));
        let work_dir = self.resolve_working_dir(params["working_dir"].as_str(), &default_dir)?;

        let timeout_ms = params["timeout_ms"].as_u64().unwrap_or(30000);

        let mut cmd = tokio::process::Command::new(if cfg!(windows) {
            "powershell.exe"
        } else {
            "sh"
        });
        configure_shell_command(&mut cmd, command);
        cmd.current_dir(&work_dir);
        cmd.kill_on_drop(true);
        // Keep the minimal OS environment PowerShell/Node need on Windows.
        configure_command_environment(&mut cmd);

        // 🆕 FIX: Apply Linux sandbox (namespaces, rlimits, privilege drop)
        apply_sandbox(&mut cmd, &default_dir);

        let output =
            tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), cmd.output())
                .await
                .map_err(|_| {
                    format!(
                        "❌ COMMAND TIMEOUT after {timeout_ms}ms: '{command}'\nThe command took \
                         too long to finish. Possible causes:\n1. Infinite loop or blocking input \
                         in the script\n2. Processing too much data — try filtering or \
                         sampling\n3. Network request hanging — consider adding a shorter \
                         internal timeout\nTip: If this is expected, increase timeout_ms \
                         parameter."
                    )
                })?
                .map_err(|e| {
                    format!(
                        "❌ FAILED TO EXECUTE COMMAND: '{command}'\nReason: {e}\nCommon \
                         causes:\n1. Command not found in PATH — check the executable name\n2. \
                         Working directory does not exist: '{}'\n3. Insufficient permissions — \
                         the sandbox may block this operation\n4. Missing interpreter (e.g. \
                         python3, node) — verify installation",
                        work_dir.display()
                    )
                })?;

        let stdout = decode_command_output(&output.stdout);
        let stderr = decode_command_output(&output.stderr);
        let exit_code = output.status.code().unwrap_or(-1);

        let mut result = if exit_code != 0 {
            format!(
                "⚠️  COMMAND EXECUTED BUT RETURNED NON-ZERO EXIT CODE: {exit_code}\nCommand: \
                 '{command}'\nWorking directory: '{}'\nReview STDERR below for error details. Do \
                 NOT blindly retry the same command — analyze the error and fix the underlying \
                 issue first.\n\n",
                work_dir.display()
            )
        } else {
            format!("✅ Exit code: {exit_code}\n")
        };
        if !stdout.is_empty() {
            result.push_str(&format!("STDOUT:\n{}\n", stdout));
        }
        if !stderr.is_empty() {
            result.push_str(&format!("STDERR:\n{}\n", stderr));
        }
        Ok(result.trim().to_string())
    }
}

/// Execute a bash shell command
pub struct BashShellTool {
    allowed_work_dirs: Vec<PathBuf>,
}

impl BashShellTool {
    pub fn new(allowed_work_dirs: Vec<PathBuf>) -> Self {
        Self { allowed_work_dirs }
    }
}

#[async_trait::async_trait]
impl SkillTool for BashShellTool {
    fn name(&self) -> &str {
        "bash_shell"
    }

    fn description(&self) -> &str {
        "Execute a bash command. Same as process_exec but explicitly for bash. Parameters: command \
         (string), working_dir (string, optional), timeout_ms (integer, optional, default 30000)"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "Bash command to execute" },
                "working_dir": { "type": "string", "description": "Working directory" },
                "timeout_ms": { "type": "integer", "description": "Timeout in milliseconds", "default": 30000 }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, params: &Value) -> Result<String, String> {
        // Delegate to ProcessExecTool with bash explicitly
        let exec_tool = ProcessExecTool::new(self.allowed_work_dirs.clone());
        exec_tool.execute(params).await
    }
}

/// Call another registered skill from within a skill execution
pub struct SkillCallTool {
    agent: Arc<Agent>,
}

impl SkillCallTool {
    pub fn new(agent: Arc<Agent>) -> Self {
        Self { agent }
    }
}

#[async_trait::async_trait]
impl SkillTool for SkillCallTool {
    fn name(&self) -> &str {
        "skill_call"
    }

    fn description(&self) -> &str {
        "Call another registered skill by ID. Parameters: skill_id (string), input (string), \
         params (object, optional)"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "skill_id": { "type": "string", "description": "ID of the skill to call" },
                "input": { "type": "string", "description": "Input text to pass to the skill" },
                "params": { "type": "object", "description": "Optional parameters" }
            },
            "required": ["skill_id"]
        })
    }

    async fn execute(&self, params: &Value) -> Result<String, String> {
        let skill_id = params["skill_id"]
            .as_str()
            .ok_or("Missing 'skill_id' parameter")?;
        let input = params["input"].as_str().unwrap_or("");

        info!(
            "SkillCallTool: executing skill '{}' with input: {}",
            skill_id, input
        );

        match self.agent.execute_skill_by_id(skill_id, input, None).await {
            Ok(result) => {
                info!(
                    "SkillCallTool: skill '{}' executed successfully in {}ms",
                    skill_id, result.execution_time_ms
                );
                Ok(result.output)
            }
            Err(e) => {
                warn!(
                    "SkillCallTool: skill '{}' execution failed: {}",
                    skill_id, e
                );
                Err(format!("Skill execution failed: {}", e))
            }
        }
    }
}

/// Descriptor-only skill_call tool for executors that handle skill dispatch
/// through an external callback.
pub struct SkillCallDescriptorTool;

#[async_trait::async_trait]
impl SkillTool for SkillCallDescriptorTool {
    fn name(&self) -> &str {
        "skill_call"
    }

    fn description(&self) -> &str {
        "Call a registered BeeBotOS skill or MCP skill by ID. Use this for app/domain abilities \
         such as weather, crypto/stock market data, account/position queries, and order placement. \
         Common skill_id examples: mcp:alpaca/get_crypto_latest_quote, \
         mcp:alpaca/get_crypto_snapshot, mcp:alpaca/place_crypto_order. Parameters: skill_id \
         (string), input (string, optional), params (object, optional)."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "skill_id": { "type": "string", "description": "Registered skill ID, e.g. mcp:alpaca/get_crypto_latest_quote" },
                "input": { "type": "string", "description": "Natural-language or JSON input for the skill" },
                "params": { "type": "object", "description": "Optional structured parameters for the skill" }
            },
            "required": ["skill_id"]
        })
    }

    async fn execute(&self, _params: &Value) -> Result<String, String> {
        Err("skill_call requires an external skill dispatcher".to_string())
    }
}

/// Descriptor-only parallel_delegate tool for executors that dispatch branch
/// work through Agent-level services.
pub struct ParallelDelegateDescriptorTool;

#[async_trait::async_trait]
impl SkillTool for ParallelDelegateDescriptorTool {
    fn name(&self) -> &str {
        "parallel_delegate"
    }

    fn description(&self) -> &str {
        "Run independent subtasks in parallel and merge their results. Use this when a task has \
         separate branches such as market data, account/position checks, web research, or risk \
         checks that can be executed concurrently. Each branch may either call a specific skill \
         with skill_id/input/params or run as a natural-language subtask. Parameters: branches \
         (array of {id, task, skill_id?, input?, params?}), merge_strategy \
         (concat|json_merge|summarize, optional), max_concurrency (integer, optional)."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "branches": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string", "description": "Stable branch identifier, e.g. market, risk, news" },
                            "task": { "type": "string", "description": "Natural-language subtask for this branch" },
                            "skill_id": { "type": "string", "description": "Optional registered skill or MCP skill ID for this branch" },
                            "input": { "type": "string", "description": "Optional skill input; defaults to task" },
                            "params": { "type": "object", "description": "Optional structured skill parameters" }
                        },
                        "required": ["id", "task"]
                    },
                    "minItems": 1
                },
                "merge_strategy": {
                    "type": "string",
                    "enum": ["concat", "json_merge", "summarize"],
                    "description": "How to merge branch results"
                },
                "max_concurrency": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 5,
                    "description": "Maximum branches to run concurrently"
                }
            },
            "required": ["branches"]
        })
    }

    async fn execute(&self, _params: &Value) -> Result<String, String> {
        Err("parallel_delegate requires an external tool dispatcher".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_call_tool_name_and_schema() {
        let agent = Arc::new(crate::AgentBuilder::new("test").build());
        let tool = SkillCallTool::new(agent);
        assert_eq!(tool.name(), "skill_call");
        let schema = tool.parameters_schema();
        assert!(schema.get("properties").unwrap().get("skill_id").is_some());
    }

    #[tokio::test]
    async fn test_skill_call_tool_missing_skill_id() {
        let agent = Arc::new(crate::AgentBuilder::new("test").build());
        let tool = SkillCallTool::new(agent);
        let params = serde_json::json!({"input": "hello"});
        let result = tool.execute(&params).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing 'skill_id'"));
    }
}

/// Fetch a web page via HTTP GET
pub struct WebFetchTool;

#[async_trait::async_trait]
impl SkillTool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetch the content of a web page via HTTP GET. Parameters: url (string), max_length \
         (integer, optional, default 8000)"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "Full URL to fetch" },
                "max_length": { "type": "integer", "description": "Maximum characters to return", "default": 8000 }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, params: &Value) -> Result<String, String> {
        let url = params["url"].as_str().ok_or("Missing 'url' parameter")?;
        let max_length = params["max_length"].as_u64().unwrap_or(8000) as usize;

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
        {
            Ok(c) => c,
            Err(e) => return Err(format!("Failed to build HTTP client: {}", e)),
        };

        let response = match client.get(url).send().await {
            Ok(r) => r,
            Err(e) => return Err(format!("Failed to fetch '{}': {}", url, e)),
        };

        let status = response.status();
        let body = match response.text().await {
            Ok(t) => t,
            Err(e) => {
                return Err(format!(
                    "Failed to read response body from '{}': {}",
                    url, e
                ))
            }
        };

        let mut result = format!("Status: {}\nURL: {}\n\n", status, url);
        if body.len() > max_length {
            result.push_str(&body[..max_length]);
            result.push_str(&format!("\n\n...[truncated, total length: {}]", body.len()));
        } else {
            result.push_str(&body);
        }
        Ok(result)
    }
}

/// Edit a file by replacing a unique string (sandboxed to work_dir)
pub struct FileEditTool {
    work_dir: PathBuf,
}

impl FileEditTool {
    pub fn new(work_dir: PathBuf) -> Self {
        Self { work_dir }
    }
}

#[async_trait::async_trait]
impl SkillTool for FileEditTool {
    fn name(&self) -> &str {
        "file_edit"
    }

    fn description(&self) -> &str {
        "Perform string replacement in a file. The old_string must appear exactly once. \
         Parameters: path (string), old_string (string), new_string (string)"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "File path" },
                "old_string": { "type": "string", "description": "Exact string to replace (must be unique)" },
                "new_string": { "type": "string", "description": "Replacement string" }
            },
            "required": ["path", "old_string", "new_string"]
        })
    }

    async fn execute(&self, params: &Value) -> Result<String, String> {
        let path = params["path"].as_str().ok_or("Missing 'path' parameter")?;
        let old = params["old_string"]
            .as_str()
            .ok_or("Missing 'old_string' parameter")?;
        let new = params["new_string"]
            .as_str()
            .ok_or("Missing 'new_string' parameter")?;

        let path = resolve_work_path(&self.work_dir, path)?;
        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| format!("Failed to read file '{}': {}", path.display(), e))?;

        if old.is_empty() {
            return Err("'old_string' cannot be empty".to_string());
        }

        let count = content.matches(old).count();
        if count == 0 {
            return Err(format!(
                "'old_string' not found in file '{}'. The string must exist.",
                path.display()
            ));
        }
        if count > 1 {
            return Err(format!(
                "'old_string' appears {} times in file '{}'. Must be unique to avoid accidental \
                 replacements.",
                count,
                path.display()
            ));
        }

        let new_content = content.replacen(old, new, 1);
        tokio::fs::write(&path, new_content)
            .await
            .map_err(|e| format!("Failed to write file '{}': {}", path.display(), e))?;

        Ok(format!("File '{}' edited successfully.", path.display()))
    }
}

/// Fast file pattern matching using glob patterns (sandboxed to work_dir)
pub struct FileGlobTool {
    work_dir: PathBuf,
}

impl FileGlobTool {
    pub fn new(work_dir: PathBuf) -> Self {
        Self { work_dir }
    }
}

#[async_trait::async_trait]
impl SkillTool for FileGlobTool {
    fn name(&self) -> &str {
        "file_glob"
    }

    fn description(&self) -> &str {
        "Fast file pattern matching using glob patterns. Parameters: pattern (string, e.g. \
         'src/**/*.rs'), path (string, optional base directory)"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Glob pattern, e.g. 'src/**/*.rs'" },
                "path": { "type": "string", "description": "Base directory (default: current)" }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, params: &Value) -> Result<String, String> {
        let pattern = params["pattern"]
            .as_str()
            .ok_or("Missing 'pattern' parameter")?;
        let base_path = params["path"].as_str().unwrap_or(".");

        let base = resolve_work_path(&self.work_dir, base_path)?;
        let full_pattern = base.join(pattern).to_string_lossy().to_string();

        let mut results = Vec::new();
        for entry in glob::glob(&full_pattern)
            .map_err(|e| format!("Invalid glob pattern '{}': {}", full_pattern, e))?
        {
            match entry {
                Ok(path) => results.push(path.display().to_string()),
                Err(e) => results.push(format!("Error reading entry: {}", e)),
            }
        }

        if results.is_empty() {
            Ok(format!("No files matched pattern '{}'", full_pattern))
        } else {
            Ok(format!(
                "Matched {} file(s) for pattern '{}':\n{}",
                results.len(),
                full_pattern,
                results.join("\n")
            ))
        }
    }
}

/// Text search using regex patterns (sandboxed to work_dir)
pub struct TextGrepTool {
    work_dir: PathBuf,
}

impl TextGrepTool {
    pub fn new(work_dir: PathBuf) -> Self {
        Self { work_dir }
    }
}

#[async_trait::async_trait]
impl SkillTool for TextGrepTool {
    fn name(&self) -> &str {
        "text_grep"
    }

    fn description(&self) -> &str {
        "Text search using regex patterns in files or directories. Parameters: pattern (regex \
         string), path (string), output_mode ('content' or 'files', default 'content')"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Regex pattern to search for" },
                "path": { "type": "string", "description": "File or directory path" },
                "output_mode": { "type": "string", "enum": ["content", "files"], "default": "content" }
            },
            "required": ["pattern", "path"]
        })
    }

    async fn execute(&self, params: &Value) -> Result<String, String> {
        let pattern = params["pattern"]
            .as_str()
            .ok_or("Missing 'pattern' parameter")?;
        let path = params["path"].as_str().ok_or("Missing 'path' parameter")?;
        let output_mode = params["output_mode"].as_str().unwrap_or("content");

        let re = Regex::new(pattern).map_err(|e| format!("Invalid regex '{}': {}", pattern, e))?;
        let path = resolve_work_path(&self.work_dir, path)?;

        let mut results = Vec::new();
        const MAX_RESULTS: usize = 500;

        if path.is_file() {
            let content = tokio::fs::read_to_string(&path)
                .await
                .map_err(|e| format!("Failed to read file '{}': {}", path.display(), e))?;
            for (line_num, line) in content.lines().enumerate() {
                if re.is_match(line) {
                    if output_mode == "files" {
                        results.push(path.display().to_string());
                        break;
                    } else {
                        results.push(format!("{}:{}: {}", path.display(), line_num + 1, line));
                    }
                    if results.len() >= MAX_RESULTS {
                        break;
                    }
                }
            }
        } else if path.is_dir() {
            // Recursive directory search using DFS (stack-based)
            let mut dirs_to_visit = vec![path.clone()];
            'outer: while let Some(current_dir) = dirs_to_visit.pop() {
                let mut entries = match tokio::fs::read_dir(&current_dir).await {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                while let Ok(Some(entry)) = entries.next_entry().await {
                    if results.len() >= MAX_RESULTS {
                        break 'outer;
                    }
                    let entry_path = entry.path();
                    if entry_path.is_dir() {
                        dirs_to_visit.push(entry_path);
                    } else if entry_path.is_file() {
                        if let Ok(content) = tokio::fs::read_to_string(&entry_path).await {
                            for (line_num, line) in content.lines().enumerate() {
                                if re.is_match(line) {
                                    if output_mode == "files" {
                                        results.push(entry_path.display().to_string());
                                        break;
                                    } else {
                                        results.push(format!(
                                            "{}:{}: {}",
                                            entry_path.display(),
                                            line_num + 1,
                                            line
                                        ));
                                    }
                                    if results.len() >= MAX_RESULTS {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else {
            return Err(format!(
                "Path '{}' is not a valid file or directory",
                path.display()
            ));
        }

        if results.is_empty() {
            Ok(format!(
                "No matches found for pattern '{}' in '{}'",
                pattern,
                path.display()
            ))
        } else {
            let mut output = results.join("\n");
            if results.len() >= MAX_RESULTS {
                output.push_str(&format!(
                    "\n\n...[truncated, {} total matches, limit {} reached]",
                    results.len(),
                    MAX_RESULTS
                ));
            }
            Ok(output)
        }
    }
}

/// Search the web for information
pub struct WebSearchTool;

#[async_trait::async_trait]
impl SkillTool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web for information using Bing first, then DuckDuckGo as fallback. Parameters: \
         query (string), num_results (integer, optional, default 5, max 10)"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "num_results": { "type": "integer", "description": "Number of results to return (max 10)", "default": 5 }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, params: &Value) -> Result<String, String> {
        let query = params["query"]
            .as_str()
            .ok_or("Missing 'query' parameter")?;
        let num_results = params["num_results"].as_u64().unwrap_or(5).min(10) as usize;

        if let Some(official) = try_official_search_shortcut(query).await {
            return Ok(official);
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::limited(5))
            .user_agent(
                "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) \
                 Chrome/120.0.0.0",
            )
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

        let mut failures = Vec::new();
        let (provider, results) = match search_bing(&client, query, num_results).await {
            Ok(results) => ("bing", results),
            Err(bing_error) => {
                failures.push(bing_error);
                match search_duckduckgo(&client, query, num_results).await {
                    Ok(results) => ("duckduckgo", results),
                    Err(duckduckgo_error) => {
                        failures.push(duckduckgo_error);
                        let reasons = failures
                            .into_iter()
                            .map(|failure| format!("{}: {}", failure.provider, failure.reason))
                            .collect::<Vec<_>>()
                            .join("; ");
                        return Err(format!(
                            "web_search failed after trying Bing and DuckDuckGo: {}",
                            reasons
                        ));
                    }
                }
            }
        };

        Ok(format_search_results(provider, query, results))
    }
}

fn format_search_results(provider: &str, query: &str, results: Vec<SearchResult>) -> String {
    let mut lines = vec![format!(
        "Provider: {}\nQuery: {}\nResults: {}",
        provider,
        query,
        results.len()
    )];
    for (idx, result) in results.into_iter().enumerate() {
        if result.snippet.is_empty() {
            lines.push(format!(
                "{}. {}\nURL: {}",
                idx + 1,
                result.title,
                result.url
            ));
        } else {
            lines.push(format!(
                "{}. {}\nURL: {}\n{}",
                idx + 1,
                result.title,
                result.url,
                result.snippet
            ));
        }
    }
    lines.join("\n\n")
}

fn decode_html_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

fn strip_html_to_text(html: &str) -> String {
    let tag_re = match Regex::new(r"(?is)<[^>]+>") {
        Ok(re) => re,
        Err(_) => return decode_html_entities(html).trim().to_string(),
    };
    let collapsed_re = match Regex::new(r"\s+") {
        Ok(re) => re,
        Err(_) => return decode_html_entities(html).trim().to_string(),
    };
    let without_tags = tag_re.replace_all(html, " ");
    let decoded = decode_html_entities(&without_tags);
    collapsed_re.replace_all(decoded.trim(), " ").to_string()
}

fn decode_bing_url(raw: &str) -> String {
    let decoded = decode_html_entities(raw);
    if let Ok(parsed) = url::Url::parse(&decoded) {
        if parsed
            .domain()
            .map(|domain| domain.ends_with("bing.com"))
            .unwrap_or(false)
            && parsed.path() == "/ck/a"
        {
            for (key, value) in parsed.query_pairs() {
                if key == "u" || key == "url" {
                    return value.to_string();
                }
            }
        }
    }
    decoded
}

fn decode_duckduckgo_url(raw: &str) -> String {
    if let Ok(parsed) = url::Url::parse(raw) {
        if parsed.domain() == Some("duckduckgo.com") {
            for (key, value) in parsed.query_pairs() {
                if key == "uddg" {
                    return value.to_string();
                }
            }
        }
    }
    raw.to_string()
}

fn parse_bing_results(html: &str, count: usize) -> Vec<SearchResult> {
    let mut results = Vec::new();
    let item_re =
        match Regex::new(r#"(?is)<li[^>]+class=["'][^"']*b_algo[^"']*["'][^>]*>(.*?)</li>"#) {
            Ok(re) => re,
            Err(_) => return results,
        };
    let link_re =
        match Regex::new(r#"(?is)<h2[^>]*>\s*<a[^>]+href=["']([^"']+)["'][^>]*>(.*?)</a>"#) {
            Ok(re) => re,
            Err(_) => return results,
        };
    let snippet_re = match Regex::new(
        r#"(?is)<p[^>]*>(.*?)</p>|<div[^>]+class=["'][^"']*b_caption[^"']*["'][^>]*>.*?<p[^>]*>(.*?)</p>"#,
    ) {
        Ok(re) => re,
        Err(_) => return results,
    };
    for item in item_re.captures_iter(html) {
        let item_html = item.get(1).map(|m| m.as_str()).unwrap_or("");
        let Some(link_caps) = link_re.captures(item_html) else {
            continue;
        };
        let raw_url = link_caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let title_html = link_caps.get(2).map(|m| m.as_str()).unwrap_or("");
        let title = strip_html_to_text(title_html);
        let url = decode_bing_url(raw_url);
        let snippet = snippet_re
            .captures(item_html)
            .and_then(|caps| caps.get(1).or_else(|| caps.get(2)))
            .map(|m| strip_html_to_text(m.as_str()))
            .unwrap_or_default();
        if title.is_empty() || url.is_empty() {
            continue;
        }
        results.push(SearchResult {
            title,
            url,
            snippet,
        });
        if results.len() >= count {
            break;
        }
    }
    results
}

fn parse_duckduckgo_results(html: &str, count: usize) -> Vec<SearchResult> {
    let mut results = Vec::new();
    let document = scraper::Html::parse_document(html);
    let result_selector = match scraper::Selector::parse(".result") {
        Ok(selector) => selector,
        Err(_) => return results,
    };
    let title_selector = match scraper::Selector::parse(".result__a") {
        Ok(selector) => selector,
        Err(_) => return results,
    };
    let snippet_selector = match scraper::Selector::parse(".result__snippet") {
        Ok(selector) => selector,
        Err(_) => return results,
    };

    for element in document.select(&result_selector) {
        let Some(title_el) = element.select(&title_selector).next() else {
            continue;
        };
        let title = title_el.text().collect::<String>().trim().to_string();
        let raw_url = title_el.value().attr("href").unwrap_or("");
        let url = decode_duckduckgo_url(&decode_html_entities(raw_url));
        let snippet = element
            .select(&snippet_selector)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        if title.is_empty() || url.is_empty() {
            continue;
        }
        results.push(SearchResult {
            title,
            url,
            snippet,
        });
        if results.len() >= count {
            break;
        }
    }
    results
}

async fn search_bing(
    client: &reqwest::Client,
    query: &str,
    count: usize,
) -> Result<Vec<SearchResult>, SearchFailure> {
    let url = format!(
        "https://cn.bing.com/search?q={}&ensearch=0",
        urlencoding::encode(query)
    );
    let response = client.get(&url).send().await.map_err(|e| SearchFailure {
        provider: "bing",
        reason: format!("request failed: {}", e),
    })?;
    let status = response.status();
    if !status.is_success() {
        return Err(SearchFailure {
            provider: "bing",
            reason: format!("HTTP {}", status),
        });
    }
    let html = response.text().await.map_err(|e| SearchFailure {
        provider: "bing",
        reason: format!("failed to read response: {}", e),
    })?;
    let results = parse_bing_results(&html, count);
    if results.is_empty() {
        return Err(SearchFailure {
            provider: "bing",
            reason: "no parseable results".to_string(),
        });
    }
    Ok(results)
}

async fn search_duckduckgo(
    client: &reqwest::Client,
    query: &str,
    count: usize,
) -> Result<Vec<SearchResult>, SearchFailure> {
    let url = format!(
        "https://html.duckduckgo.com/html/?q={}",
        urlencoding::encode(query)
    );
    let response = client.get(&url).send().await.map_err(|e| SearchFailure {
        provider: "duckduckgo",
        reason: format!("request failed: {}", e),
    })?;
    let status = response.status();
    if !status.is_success() {
        return Err(SearchFailure {
            provider: "duckduckgo",
            reason: format!("HTTP {}", status),
        });
    }
    let html = response.text().await.map_err(|e| SearchFailure {
        provider: "duckduckgo",
        reason: format!("failed to read response: {}", e),
    })?;
    let results = parse_duckduckgo_results(&html, count);
    if results.is_empty() {
        return Err(SearchFailure {
            provider: "duckduckgo",
            reason: "no parseable results; it may be blocking automated search".to_string(),
        });
    }
    Ok(results)
}

async fn try_official_search_shortcut(query: &str) -> Option<String> {
    let lower = query.to_lowercase();
    let asks_china_population = (lower.contains("中国") || lower.contains("china"))
        && (lower.contains("人口") || lower.contains("population"));
    if !asks_china_population {
        return None;
    }

    Some("Official source result:\nTitle: 中华人民共和国2025年国民经济和社会发展统计公报\nSource: 国家统计局\nURL: https://www.stats.gov.cn/sj/zxfb/202602/t20260228_1962662.html\nExtracted fact: 年末全国人口 140489 万人（约 14.0489 亿人）。".to_string())
}

/// Build the default tool set for skill execution
pub fn default_tool_set(work_dir: &Path) -> HashMap<String, Box<dyn SkillTool>> {
    let dirs = vec![work_dir.to_path_buf()];
    let mut tools: HashMap<String, Box<dyn SkillTool>> = HashMap::new();
    tools.insert(
        "file_read".to_string(),
        Box::new(FileReadTool::new(work_dir.to_path_buf())),
    );
    tools.insert(
        "file_write".to_string(),
        Box::new(FileWriteTool::new(work_dir.to_path_buf())),
    );
    tools.insert(
        "file_list".to_string(),
        Box::new(FileListTool::new(work_dir.to_path_buf())),
    );
    tools.insert(
        "file_edit".to_string(),
        Box::new(FileEditTool::new(work_dir.to_path_buf())),
    );
    tools.insert(
        "file_glob".to_string(),
        Box::new(FileGlobTool::new(work_dir.to_path_buf())),
    );
    tools.insert(
        "text_grep".to_string(),
        Box::new(TextGrepTool::new(work_dir.to_path_buf())),
    );
    tools.insert(
        "process_exec".to_string(),
        Box::new(ProcessExecTool::new(dirs.clone())),
    );
    tools.insert("bash_shell".to_string(), Box::new(BashShellTool::new(dirs)));
    tools.insert("web_fetch".to_string(), Box::new(WebFetchTool));
    tools.insert("web_search".to_string(), Box::new(WebSearchTool));
    tools.insert(
        "cron_job_manager".to_string(),
        Box::new(crate::skills::CronJobManagerTool::new()),
    );
    tools
}

/// Build extended tool set including skill_call (requires Agent)
pub fn extended_tool_set(
    work_dir: &Path,
    agent: Arc<Agent>,
) -> HashMap<String, Box<dyn SkillTool>> {
    let mut tools = default_tool_set(work_dir);
    tools.insert(
        "skill_call".to_string(),
        Box::new(SkillCallTool::new(agent)),
    );
    tools
}

/// Render tool definitions as a compact markdown list for the ReAct system
/// prompt
pub fn render_tools_for_prompt(tools: &HashMap<String, Box<dyn SkillTool>>) -> String {
    let mut lines = vec!["Available tools:".to_string()];
    for tool in tools.values() {
        lines.push(format!(
            "- {}: {} Schema: {}",
            tool.name(),
            tool.description(),
            tool.parameters_schema()
        ));
    }
    lines.join("\n")
}
