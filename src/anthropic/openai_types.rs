//! OpenAI Chat Completion API 类型定义
//!
//! 用于兼容 OpenAI Chat Completion API 格式

use serde::{Deserialize, Serialize};

// === 请求类型 ===

/// OpenAI Chat Completion 请求
#[derive(Debug, Deserialize)]
pub struct ChatCompletionRequest {
    /// 模型名称
    pub model: String,
    /// 消息列表
    pub messages: Vec<ChatMessage>,
    /// 最大生成 token 数
    #[serde(default)]
    pub max_tokens: Option<i32>,
    /// 是否流式响应
    #[serde(default)]
    pub stream: bool,
    /// 温度参数（0-2）
    #[serde(default)]
    pub temperature: Option<f32>,
    /// Top-p 采样
    #[serde(default)]
    pub top_p: Option<f32>,
    /// 工具列表
    #[serde(default)]
    pub tools: Option<Vec<OpenAITool>>,
    /// 工具选择
    #[serde(default)]
    pub tool_choice: Option<serde_json::Value>,
    /// 用户标识
    #[serde(default)]
    pub user: Option<String>,
}

/// OpenAI 消息
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatMessage {
    /// 角色: system, user, assistant, tool
    pub role: String,
    /// 消息内容（可以是字符串或内容块数组）
    #[serde(default)]
    pub content: Option<serde_json::Value>,
    /// 工具调用（assistant 消息）
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// 工具调用 ID（tool 消息）
    #[serde(default)]
    pub tool_call_id: Option<String>,
    /// 函数名称（tool 消息，可选）
    #[serde(default)]
    pub name: Option<String>,
}

/// OpenAI 工具定义
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenAITool {
    /// 工具类型，通常是 "function"
    #[serde(rename = "type")]
    pub tool_type: String,
    /// 函数定义
    pub function: FunctionDefinition,
}

/// 函数定义
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FunctionDefinition {
    /// 函数名称
    pub name: String,
    /// 函数描述
    #[serde(default)]
    pub description: Option<String>,
    /// 参数 schema
    #[serde(default)]
    pub parameters: Option<serde_json::Value>,
}

/// 工具调用
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolCall {
    /// 工具调用 ID
    pub id: String,
    /// 工具类型
    #[serde(rename = "type")]
    pub call_type: String,
    /// 函数调用
    pub function: FunctionCall,
}

/// 函数调用
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FunctionCall {
    /// 函数名称
    pub name: String,
    /// 函数参数（JSON 字符串）
    pub arguments: String,
}

// === 响应类型 ===

/// OpenAI Chat Completion 响应
#[derive(Debug, Serialize)]
pub struct ChatCompletionResponse {
    /// 响应 ID
    pub id: String,
    /// 对象类型
    pub object: String,
    /// 创建时间戳
    pub created: i64,
    /// 模型名称
    pub model: String,
    /// 选择列表
    pub choices: Vec<Choice>,
    /// 使用统计
    pub usage: Usage,
    /// 系统指纹
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_fingerprint: Option<String>,
}

/// 选择
#[derive(Debug, Serialize)]
pub struct Choice {
    /// 索引
    pub index: i32,
    /// 消息
    pub message: ResponseMessage,
    /// 结束原因
    pub finish_reason: Option<String>,
    /// 日志概率
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<serde_json::Value>,
}

/// 响应消息
#[derive(Debug, Serialize)]
pub struct ResponseMessage {
    /// 角色
    pub role: String,
    /// 内容
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// 工具调用
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

/// 使用统计
#[derive(Debug, Serialize)]
pub struct Usage {
    /// 提示 token 数
    pub prompt_tokens: i32,
    /// 完成 token 数
    pub completion_tokens: i32,
    /// 总 token 数
    pub total_tokens: i32,
}

// === 流式响应类型 ===

/// OpenAI Chat Completion 流式响应块
#[derive(Debug, Serialize)]
pub struct ChatCompletionChunk {
    /// 响应 ID
    pub id: String,
    /// 对象类型
    pub object: String,
    /// 创建时间戳
    pub created: i64,
    /// 模型名称
    pub model: String,
    /// 选择列表
    pub choices: Vec<ChunkChoice>,
    /// 系统指纹
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_fingerprint: Option<String>,
    /// 使用统计（仅在最后一个块中）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

/// 流式选择
#[derive(Debug, Serialize)]
pub struct ChunkChoice {
    /// 索引
    pub index: i32,
    /// 增量
    pub delta: ChunkDelta,
    /// 结束原因
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    /// 日志概率
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<serde_json::Value>,
}

/// 流式增量
#[derive(Debug, Default, Serialize)]
pub struct ChunkDelta {
    /// 角色（仅在第一个块中）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// 内容增量
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// 工具调用增量
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ChunkToolCall>>,
}

/// 流式工具调用
#[derive(Debug, Serialize)]
pub struct ChunkToolCall {
    /// 索引
    pub index: i32,
    /// 工具调用 ID（仅在第一个块中）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// 工具类型（仅在第一个块中）
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub call_type: Option<String>,
    /// 函数调用
    pub function: ChunkFunctionCall,
}

/// 流式函数调用
#[derive(Debug, Serialize)]
pub struct ChunkFunctionCall {
    /// 函数名称（仅在第一个块中）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 参数增量
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}

// === OpenAI 模型列表响应 ===

/// OpenAI 模型列表响应
#[derive(Debug, Serialize)]
pub struct OpenAIModelsResponse {
    pub object: String,
    pub data: Vec<OpenAIModel>,
}

/// OpenAI 模型信息
#[derive(Debug, Serialize)]
pub struct OpenAIModel {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub owned_by: String,
}
