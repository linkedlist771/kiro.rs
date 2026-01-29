//! OpenAI Chat Completion API 处理器
//!
//! 提供与 OpenAI Chat Completion API 兼容的端点

use std::convert::Infallible;

use axum::{
    Json as JsonExtractor,
    body::Body,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Json, Response},
};
use bytes::Bytes;
use futures::{Stream, StreamExt, stream};
use serde_json::json;
use std::time::Duration;
use tokio::time::interval;
use uuid::Uuid;

use crate::kiro::model::events::Event;
use crate::kiro::model::requests::kiro::KiroRequest;
use crate::kiro::parser::decoder::EventStreamDecoder;
use crate::token;

use super::converter::{ConversionError, convert_request};
use super::middleware::AppState;
use super::openai_converter::convert_openai_to_anthropic;
use super::openai_types::{
    ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse, Choice, ChunkChoice,
    ChunkDelta, ChunkFunctionCall, ChunkToolCall, OpenAIModel, OpenAIModelsResponse,
    ResponseMessage, ToolCall, Usage,
};

/// Ping 事件间隔（25秒）
const PING_INTERVAL_SECS: u64 = 25;

/// 上下文窗口大小（200k tokens）
const CONTEXT_WINDOW_SIZE: i32 = 200_000;

/// GET /v1/models (OpenAI 格式)
pub async fn get_openai_models() -> impl IntoResponse {
    tracing::info!("Received GET /v1/models request (OpenAI format)");

    let models = vec![
        OpenAIModel {
            id: "gpt-4o".to_string(),
            object: "model".to_string(),
            created: 1727568000,
            owned_by: "openai".to_string(),
        },
        OpenAIModel {
            id: "gpt-4o-mini".to_string(),
            object: "model".to_string(),
            created: 1727568000,
            owned_by: "openai".to_string(),
        },
        OpenAIModel {
            id: "gpt-4-turbo".to_string(),
            object: "model".to_string(),
            created: 1727568000,
            owned_by: "openai".to_string(),
        },
        OpenAIModel {
            id: "claude-sonnet-4-5-20250929".to_string(),
            object: "model".to_string(),
            created: 1727568000,
            owned_by: "anthropic".to_string(),
        },
        OpenAIModel {
            id: "claude-opus-4-5-20251101".to_string(),
            object: "model".to_string(),
            created: 1730419200,
            owned_by: "anthropic".to_string(),
        },
        OpenAIModel {
            id: "claude-haiku-4-5-20251001".to_string(),
            object: "model".to_string(),
            created: 1727740800,
            owned_by: "anthropic".to_string(),
        },
    ];

    Json(OpenAIModelsResponse {
        object: "list".to_string(),
        data: models,
    })
}

/// POST /v1/chat/completions
pub async fn post_chat_completions(
    State(state): State<AppState>,
    JsonExtractor(payload): JsonExtractor<ChatCompletionRequest>,
) -> Response {
    tracing::info!(
        model = %payload.model,
        stream = %payload.stream,
        message_count = %payload.messages.len(),
        "Received POST /v1/chat/completions request"
    );

    // 检查 KiroProvider 是否可用
    let provider = match &state.kiro_provider {
        Some(p) => p.clone(),
        None => {
            tracing::error!("KiroProvider 未配置");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({
                    "error": {
                        "message": "Kiro API provider not configured",
                        "type": "service_unavailable",
                        "code": "service_unavailable"
                    }
                })),
            )
                .into_response();
        }
    };

    // 转换 OpenAI 请求为 Anthropic 请求
    let anthropic_request = convert_openai_to_anthropic(&payload);

    // 转换为 Kiro 请求
    let conversion_result = match convert_request(&anthropic_request) {
        Ok(result) => result,
        Err(e) => {
            let message = match &e {
                ConversionError::UnsupportedModel(model) => format!("Model not supported: {}", model),
                ConversionError::EmptyMessages => "Messages list is empty".to_string(),
            };
            tracing::warn!("请求转换失败: {}", e);
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": {
                        "message": message,
                        "type": "invalid_request_error",
                        "code": "invalid_request_error"
                    }
                })),
            )
                .into_response();
        }
    };

    // 构建 Kiro 请求
    let kiro_request = KiroRequest {
        conversation_state: conversion_result.conversation_state,
        profile_arn: state.profile_arn.clone(),
    };

    let request_body = match serde_json::to_string(&kiro_request) {
        Ok(body) => body,
        Err(e) => {
            tracing::error!("序列化请求失败: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": {
                        "message": format!("Failed to serialize request: {}", e),
                        "type": "internal_error",
                        "code": "internal_error"
                    }
                })),
            )
                .into_response();
        }
    };

    tracing::debug!("Kiro request body: {}", request_body);

    // 估算输入 tokens
    let input_tokens = token::count_all_tokens(
        anthropic_request.model.clone(),
        anthropic_request.system,
        anthropic_request.messages,
        anthropic_request.tools,
    ) as i32;

    if payload.stream {
        // 流式响应
        handle_openai_stream_request(provider, &request_body, &payload.model, input_tokens).await
    } else {
        // 非流式响应
        handle_openai_non_stream_request(provider, &request_body, &payload.model, input_tokens).await
    }
}

/// 处理 OpenAI 格式的非流式请求
async fn handle_openai_non_stream_request(
    provider: std::sync::Arc<crate::kiro::provider::KiroProvider>,
    request_body: &str,
    model: &str,
    input_tokens: i32,
) -> Response {
    // 调用 Kiro API
    let response = match provider.call_api(request_body).await {
        Ok(resp) => resp,
        Err(e) => {
            tracing::error!("Kiro API 调用失败: {}", e);
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({
                    "error": {
                        "message": format!("Upstream API call failed: {}", e),
                        "type": "api_error",
                        "code": "api_error"
                    }
                })),
            )
                .into_response();
        }
    };

    // 读取响应体
    let body_bytes = match response.bytes().await {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::error!("读取响应体失败: {}", e);
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({
                    "error": {
                        "message": format!("Failed to read response: {}", e),
                        "type": "api_error",
                        "code": "api_error"
                    }
                })),
            )
                .into_response();
        }
    };

    // 解析事件流
    let mut decoder = EventStreamDecoder::new();
    if let Err(e) = decoder.feed(&body_bytes) {
        tracing::warn!("缓冲区溢出: {}", e);
    }

    let mut text_content = String::new();
    let mut tool_calls: Vec<ToolCall> = Vec::new();
    let mut has_tool_use = false;
    let mut finish_reason = "stop".to_string();
    let mut context_input_tokens: Option<i32> = None;

    // 收集工具调用的增量 JSON
    let mut tool_json_buffers: std::collections::HashMap<String, (String, String)> =
        std::collections::HashMap::new();

    for result in decoder.decode_iter() {
        match result {
            Ok(frame) => {
                if let Ok(event) = Event::from_frame(frame) {
                    match event {
                        Event::AssistantResponse(resp) => {
                            text_content.push_str(&resp.content);
                        }
                        Event::ToolUse(tool_use) => {
                            has_tool_use = true;

                            // 累积工具的 JSON 输入
                            let entry = tool_json_buffers
                                .entry(tool_use.tool_use_id.clone())
                                .or_insert_with(|| (tool_use.name.clone(), String::new()));
                            entry.1.push_str(&tool_use.input);

                            // 如果是完整的工具调用，添加到列表
                            if tool_use.stop {
                                let (name, args) = tool_json_buffers
                                    .remove(&tool_use.tool_use_id)
                                    .unwrap_or((tool_use.name.clone(), String::new()));

                                tool_calls.push(ToolCall {
                                    id: tool_use.tool_use_id,
                                    call_type: "function".to_string(),
                                    function: super::openai_types::FunctionCall {
                                        name,
                                        arguments: args,
                                    },
                                });
                            }
                        }
                        Event::ContextUsage(context_usage) => {
                            let actual_input_tokens = (context_usage.context_usage_percentage
                                * (CONTEXT_WINDOW_SIZE as f64)
                                / 100.0) as i32;
                            context_input_tokens = Some(actual_input_tokens);
                        }
                        Event::Exception { exception_type, .. } => {
                            if exception_type == "ContentLengthExceededException" {
                                finish_reason = "length".to_string();
                            }
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                tracing::warn!("解码事件失败: {}", e);
            }
        }
    }

    // 确定 finish_reason
    if has_tool_use && finish_reason == "stop" {
        finish_reason = "tool_calls".to_string();
    }

    // 估算输出 tokens
    let output_tokens = estimate_output_tokens(&text_content, &tool_calls);
    let final_input_tokens = context_input_tokens.unwrap_or(input_tokens);

    // 构建响应
    let response_body = ChatCompletionResponse {
        id: format!("chatcmpl-{}", Uuid::new_v4().to_string().replace('-', "")),
        object: "chat.completion".to_string(),
        created: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64,
        model: model.to_string(),
        choices: vec![Choice {
            index: 0,
            message: ResponseMessage {
                role: "assistant".to_string(),
                content: if text_content.is_empty() {
                    None
                } else {
                    Some(text_content)
                },
                tool_calls: if tool_calls.is_empty() {
                    None
                } else {
                    Some(tool_calls)
                },
            },
            finish_reason: Some(finish_reason),
            logprobs: None,
        }],
        usage: Usage {
            prompt_tokens: final_input_tokens,
            completion_tokens: output_tokens,
            total_tokens: final_input_tokens + output_tokens,
        },
        system_fingerprint: None,
    };

    (StatusCode::OK, Json(response_body)).into_response()
}

/// 处理 OpenAI 格式的流式请求
async fn handle_openai_stream_request(
    provider: std::sync::Arc<crate::kiro::provider::KiroProvider>,
    request_body: &str,
    model: &str,
    input_tokens: i32,
) -> Response {
    // 调用 Kiro API
    let response = match provider.call_api_stream(request_body).await {
        Ok(resp) => resp,
        Err(e) => {
            tracing::error!("Kiro API 调用失败: {}", e);
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({
                    "error": {
                        "message": format!("Upstream API call failed: {}", e),
                        "type": "api_error",
                        "code": "api_error"
                    }
                })),
            )
                .into_response();
        }
    };

    // 创建流处理上下文
    let ctx = OpenAIStreamContext::new(model, input_tokens);

    // 创建 SSE 流
    let stream = create_openai_sse_stream(response, ctx);

    // 返回 SSE 响应
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header(header::CONNECTION, "keep-alive")
        .body(Body::from_stream(stream))
        .unwrap()
}

/// OpenAI 流处理上下文
struct OpenAIStreamContext {
    /// 响应 ID
    id: String,
    /// 模型名称
    model: String,
    /// 输入 tokens
    input_tokens: i32,
    /// 输出 tokens
    output_tokens: i32,
    /// 是否已发送初始角色
    sent_initial_role: bool,
    /// 当前工具索引
    current_tool_index: i32,
    /// 工具 ID 到索引的映射
    tool_indices: std::collections::HashMap<String, i32>,
    /// 是否有工具调用
    has_tool_use: bool,
    /// 停止原因
    stop_reason: Option<String>,
    /// 从 contextUsageEvent 计算的实际输入 tokens
    context_input_tokens: Option<i32>,
}

impl OpenAIStreamContext {
    fn new(model: impl Into<String>, input_tokens: i32) -> Self {
        Self {
            id: format!("chatcmpl-{}", Uuid::new_v4().to_string().replace('-', "")),
            model: model.into(),
            input_tokens,
            output_tokens: 0,
            sent_initial_role: false,
            current_tool_index: -1,
            tool_indices: std::collections::HashMap::new(),
            has_tool_use: false,
            stop_reason: None,
            context_input_tokens: None,
        }
    }

    /// 生成初始块（包含角色）
    fn generate_initial_chunk(&mut self) -> Option<ChatCompletionChunk> {
        if self.sent_initial_role {
            return None;
        }
        self.sent_initial_role = true;

        Some(ChatCompletionChunk {
            id: self.id.clone(),
            object: "chat.completion.chunk".to_string(),
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            model: self.model.clone(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: ChunkDelta {
                    role: Some("assistant".to_string()),
                    content: Some(String::new()),
                    tool_calls: None,
                },
                finish_reason: None,
                logprobs: None,
            }],
            system_fingerprint: None,
            usage: None,
        })
    }

    /// 处理 Kiro 事件
    fn process_event(&mut self, event: &Event) -> Vec<ChatCompletionChunk> {
        let mut chunks = Vec::new();

        // 确保发送初始角色
        if !self.sent_initial_role {
            if let Some(chunk) = self.generate_initial_chunk() {
                chunks.push(chunk);
            }
        }

        match event {
            Event::AssistantResponse(resp) => {
                if !resp.content.is_empty() {
                    self.output_tokens += estimate_tokens(&resp.content);
                    chunks.push(self.create_content_chunk(&resp.content));
                }
            }
            Event::ToolUse(tool_use) => {
                self.has_tool_use = true;

                // 获取或分配工具索引
                let tool_index = if let Some(&idx) = self.tool_indices.get(&tool_use.tool_use_id) {
                    idx
                } else {
                    self.current_tool_index += 1;
                    let idx = self.current_tool_index;
                    self.tool_indices.insert(tool_use.tool_use_id.clone(), idx);

                    // 发送工具调用开始块
                    chunks.push(self.create_tool_start_chunk(
                        idx,
                        &tool_use.tool_use_id,
                        &tool_use.name,
                    ));
                    idx
                };

                // 发送参数增量
                if !tool_use.input.is_empty() {
                    self.output_tokens += (tool_use.input.len() as i32 + 3) / 4;
                    chunks.push(self.create_tool_delta_chunk(tool_index, &tool_use.input));
                }
            }
            Event::ContextUsage(context_usage) => {
                let actual_input_tokens = (context_usage.context_usage_percentage
                    * (CONTEXT_WINDOW_SIZE as f64)
                    / 100.0) as i32;
                self.context_input_tokens = Some(actual_input_tokens);
            }
            Event::Exception { exception_type, .. } => {
                if exception_type == "ContentLengthExceededException" {
                    self.stop_reason = Some("length".to_string());
                }
            }
            _ => {}
        }

        chunks
    }

    /// 创建内容块
    fn create_content_chunk(&self, content: &str) -> ChatCompletionChunk {
        ChatCompletionChunk {
            id: self.id.clone(),
            object: "chat.completion.chunk".to_string(),
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            model: self.model.clone(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: ChunkDelta {
                    role: None,
                    content: Some(content.to_string()),
                    tool_calls: None,
                },
                finish_reason: None,
                logprobs: None,
            }],
            system_fingerprint: None,
            usage: None,
        }
    }

    /// 创建工具调用开始块
    fn create_tool_start_chunk(&self, index: i32, id: &str, name: &str) -> ChatCompletionChunk {
        ChatCompletionChunk {
            id: self.id.clone(),
            object: "chat.completion.chunk".to_string(),
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            model: self.model.clone(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: ChunkDelta {
                    role: None,
                    content: None,
                    tool_calls: Some(vec![ChunkToolCall {
                        index,
                        id: Some(id.to_string()),
                        call_type: Some("function".to_string()),
                        function: ChunkFunctionCall {
                            name: Some(name.to_string()),
                            arguments: Some(String::new()),
                        },
                    }]),
                },
                finish_reason: None,
                logprobs: None,
            }],
            system_fingerprint: None,
            usage: None,
        }
    }

    /// 创建工具调用增量块
    fn create_tool_delta_chunk(&self, index: i32, arguments: &str) -> ChatCompletionChunk {
        ChatCompletionChunk {
            id: self.id.clone(),
            object: "chat.completion.chunk".to_string(),
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            model: self.model.clone(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: ChunkDelta {
                    role: None,
                    content: None,
                    tool_calls: Some(vec![ChunkToolCall {
                        index,
                        id: None,
                        call_type: None,
                        function: ChunkFunctionCall {
                            name: None,
                            arguments: Some(arguments.to_string()),
                        },
                    }]),
                },
                finish_reason: None,
                logprobs: None,
            }],
            system_fingerprint: None,
            usage: None,
        }
    }

    /// 生成最终块
    fn generate_final_chunk(&self) -> ChatCompletionChunk {
        let finish_reason = if let Some(ref reason) = self.stop_reason {
            reason.clone()
        } else if self.has_tool_use {
            "tool_calls".to_string()
        } else {
            "stop".to_string()
        };

        let final_input_tokens = self.context_input_tokens.unwrap_or(self.input_tokens);

        ChatCompletionChunk {
            id: self.id.clone(),
            object: "chat.completion.chunk".to_string(),
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            model: self.model.clone(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: ChunkDelta::default(),
                finish_reason: Some(finish_reason),
                logprobs: None,
            }],
            system_fingerprint: None,
            usage: Some(Usage {
                prompt_tokens: final_input_tokens,
                completion_tokens: self.output_tokens,
                total_tokens: final_input_tokens + self.output_tokens,
            }),
        }
    }
}

/// 创建 OpenAI SSE 流
fn create_openai_sse_stream(
    response: reqwest::Response,
    ctx: OpenAIStreamContext,
) -> impl Stream<Item = Result<Bytes, Infallible>> {
    let body_stream = response.bytes_stream();

    stream::unfold(
        (
            body_stream,
            ctx,
            EventStreamDecoder::new(),
            false,
            interval(Duration::from_secs(PING_INTERVAL_SECS)),
        ),
        |(mut body_stream, mut ctx, mut decoder, finished, mut ping_interval)| async move {
            if finished {
                return None;
            }

            tokio::select! {
                chunk_result = body_stream.next() => {
                    match chunk_result {
                        Some(Ok(chunk)) => {
                            if let Err(e) = decoder.feed(&chunk) {
                                tracing::warn!("缓冲区溢出: {}", e);
                            }

                            let mut chunks = Vec::new();
                            for result in decoder.decode_iter() {
                                match result {
                                    Ok(frame) => {
                                        if let Ok(event) = Event::from_frame(frame) {
                                            chunks.extend(ctx.process_event(&event));
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!("解码事件失败: {}", e);
                                    }
                                }
                            }

                            let bytes: Vec<Result<Bytes, Infallible>> = chunks
                                .into_iter()
                                .map(|c| Ok(chunk_to_sse_bytes(&c)))
                                .collect();

                            Some((stream::iter(bytes), (body_stream, ctx, decoder, false, ping_interval)))
                        }
                        Some(Err(e)) => {
                            tracing::error!("读取响应流失败: {}", e);
                            let final_chunk = ctx.generate_final_chunk();
                            let bytes: Vec<Result<Bytes, Infallible>> = vec![
                                Ok(chunk_to_sse_bytes(&final_chunk)),
                                Ok(Bytes::from("data: [DONE]\n\n")),
                            ];
                            Some((stream::iter(bytes), (body_stream, ctx, decoder, true, ping_interval)))
                        }
                        None => {
                            let final_chunk = ctx.generate_final_chunk();
                            let bytes: Vec<Result<Bytes, Infallible>> = vec![
                                Ok(chunk_to_sse_bytes(&final_chunk)),
                                Ok(Bytes::from("data: [DONE]\n\n")),
                            ];
                            Some((stream::iter(bytes), (body_stream, ctx, decoder, true, ping_interval)))
                        }
                    }
                }
                _ = ping_interval.tick() => {
                    // OpenAI 格式不需要 ping，但保持连接活跃
                    let bytes: Vec<Result<Bytes, Infallible>> = vec![];
                    Some((stream::iter(bytes), (body_stream, ctx, decoder, false, ping_interval)))
                }
            }
        },
    )
    .flatten()
}

/// 将 ChatCompletionChunk 转换为 SSE 字节
fn chunk_to_sse_bytes(chunk: &ChatCompletionChunk) -> Bytes {
    let json = serde_json::to_string(chunk).unwrap_or_default();
    Bytes::from(format!("data: {}\n\n", json))
}

/// 估算输出 tokens
fn estimate_output_tokens(text: &str, tool_calls: &[ToolCall]) -> i32 {
    let text_tokens = estimate_tokens(text);
    let tool_tokens: i32 = tool_calls
        .iter()
        .map(|tc| (tc.function.arguments.len() as i32 + 3) / 4 + 10)
        .sum();
    text_tokens + tool_tokens
}

/// 简单的 token 估算
fn estimate_tokens(text: &str) -> i32 {
    let chars: Vec<char> = text.chars().collect();
    let mut chinese_count = 0;
    let mut other_count = 0;

    for c in &chars {
        if *c >= '\u{4E00}' && *c <= '\u{9FFF}' {
            chinese_count += 1;
        } else {
            other_count += 1;
        }
    }

    let chinese_tokens = (chinese_count * 2 + 2) / 3;
    let other_tokens = (other_count + 3) / 4;

    (chinese_tokens + other_tokens).max(1)
}
