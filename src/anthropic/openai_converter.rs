//! OpenAI → Anthropic 格式转换器
//!
//! 将 OpenAI Chat Completion API 请求转换为 Anthropic Messages API 请求

use super::openai_types::{ChatCompletionRequest, ChatMessage, OpenAITool};
use super::types::{Message, MessagesRequest, Metadata, SystemMessage, Tool};
use std::collections::HashMap;

/// 将 OpenAI 请求转换为 Anthropic 请求
pub fn convert_openai_to_anthropic(req: &ChatCompletionRequest) -> MessagesRequest {
    let mut system_messages: Vec<SystemMessage> = Vec::new();
    let mut messages: Vec<Message> = Vec::new();

    for msg in &req.messages {
        match msg.role.as_str() {
            "system" => {
                // 系统消息
                if let Some(content) = extract_text_content(&msg.content) {
                    system_messages.push(SystemMessage { text: content });
                }
            }
            "user" => {
                // 用户消息
                let content = convert_user_content(&msg.content);
                messages.push(Message {
                    role: "user".to_string(),
                    content,
                });
            }
            "assistant" => {
                // 助手消息
                let content = convert_assistant_content(msg);
                messages.push(Message {
                    role: "assistant".to_string(),
                    content,
                });
            }
            "tool" => {
                // 工具结果消息 - 转换为 Anthropic 的 tool_result
                if let Some(tool_call_id) = &msg.tool_call_id {
                    let result_content = extract_text_content(&msg.content).unwrap_or_default();
                    let tool_result = serde_json::json!({
                        "type": "tool_result",
                        "tool_use_id": tool_call_id,
                        "content": result_content
                    });

                    // 检查最后一条消息是否是 user 消息
                    if let Some(last_msg) = messages.last_mut() {
                        if last_msg.role == "user" {
                            // 追加到现有的 user 消息
                            if let serde_json::Value::Array(ref mut arr) = last_msg.content {
                                arr.push(tool_result);
                            } else {
                                // 转换为数组
                                let old_content = last_msg.content.clone();
                                let mut new_arr = vec![serde_json::json!({
                                    "type": "text",
                                    "text": old_content.as_str().unwrap_or("")
                                })];
                                new_arr.push(tool_result);
                                last_msg.content = serde_json::Value::Array(new_arr);
                            }
                            continue;
                        }
                    }

                    // 创建新的 user 消息包含 tool_result
                    messages.push(Message {
                        role: "user".to_string(),
                        content: serde_json::Value::Array(vec![tool_result]),
                    });
                }
            }
            _ => {}
        }
    }

    // 转换工具定义
    let tools = convert_openai_tools(&req.tools);

    // 构建 metadata
    let metadata = req.user.as_ref().map(|user_id| Metadata {
        user_id: Some(user_id.clone()),
    });

    MessagesRequest {
        model: req.model.clone(),
        max_tokens: req.max_tokens.unwrap_or(4096),
        messages,
        stream: req.stream,
        system: if system_messages.is_empty() {
            None
        } else {
            Some(system_messages)
        },
        tools,
        tool_choice: req.tool_choice.clone(),
        thinking: None,
        metadata,
    }
}

/// 提取文本内容
fn extract_text_content(content: &Option<serde_json::Value>) -> Option<String> {
    match content {
        Some(serde_json::Value::String(s)) => Some(s.clone()),
        Some(serde_json::Value::Array(arr)) => {
            let mut text_parts = Vec::new();
            for item in arr {
                if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                    text_parts.push(text.to_string());
                }
            }
            if text_parts.is_empty() {
                None
            } else {
                Some(text_parts.join("\n"))
            }
        }
        _ => None,
    }
}

/// 转换用户消息内容
fn convert_user_content(content: &Option<serde_json::Value>) -> serde_json::Value {
    match content {
        Some(serde_json::Value::String(s)) => serde_json::Value::String(s.clone()),
        Some(serde_json::Value::Array(arr)) => {
            // 转换 OpenAI 格式的内容块到 Anthropic 格式
            let mut anthropic_blocks = Vec::new();
            for item in arr {
                if let Some(content_type) = item.get("type").and_then(|v| v.as_str()) {
                    match content_type {
                        "text" => {
                            if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                                anthropic_blocks.push(serde_json::json!({
                                    "type": "text",
                                    "text": text
                                }));
                            }
                        }
                        "image_url" => {
                            // 转换图片 URL 到 Anthropic 格式
                            if let Some(image_url) = item.get("image_url") {
                                if let Some(url) = image_url.get("url").and_then(|v| v.as_str()) {
                                    // 检查是否是 base64 数据 URL
                                    if url.starts_with("data:image/") {
                                        if let Some((media_type, data)) = parse_data_url(url) {
                                            anthropic_blocks.push(serde_json::json!({
                                                "type": "image",
                                                "source": {
                                                    "type": "base64",
                                                    "media_type": media_type,
                                                    "data": data
                                                }
                                            }));
                                        }
                                    }
                                    // 注意：Anthropic 不支持直接的图片 URL，需要先下载
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            if anthropic_blocks.is_empty() {
                serde_json::Value::String(String::new())
            } else {
                serde_json::Value::Array(anthropic_blocks)
            }
        }
        _ => serde_json::Value::String(String::new()),
    }
}

/// 解析 data URL
fn parse_data_url(url: &str) -> Option<(String, String)> {
    // 格式: data:image/png;base64,xxxxx
    if !url.starts_with("data:") {
        return None;
    }

    let rest = &url[5..];
    let parts: Vec<&str> = rest.splitn(2, ',').collect();
    if parts.len() != 2 {
        return None;
    }

    let meta = parts[0];
    let data = parts[1];

    // 解析 media type
    let media_type = if meta.contains(';') {
        meta.split(';').next().unwrap_or("image/png")
    } else {
        meta
    };

    Some((media_type.to_string(), data.to_string()))
}

/// 转换助手消息内容
fn convert_assistant_content(msg: &ChatMessage) -> serde_json::Value {
    let mut blocks = Vec::new();

    // 添加文本内容
    if let Some(content) = extract_text_content(&msg.content) {
        if !content.is_empty() {
            blocks.push(serde_json::json!({
                "type": "text",
                "text": content
            }));
        }
    }

    // 添加工具调用
    if let Some(tool_calls) = &msg.tool_calls {
        for tool_call in tool_calls {
            // 解析参数 JSON
            let input: serde_json::Value =
                serde_json::from_str(&tool_call.function.arguments).unwrap_or(serde_json::json!({}));

            blocks.push(serde_json::json!({
                "type": "tool_use",
                "id": tool_call.id,
                "name": tool_call.function.name,
                "input": input
            }));
        }
    }

    if blocks.is_empty() {
        serde_json::Value::String(String::new())
    } else if blocks.len() == 1 && blocks[0].get("type").and_then(|v| v.as_str()) == Some("text") {
        // 如果只有一个文本块，直接返回字符串
        blocks[0]
            .get("text")
            .cloned()
            .unwrap_or(serde_json::Value::String(String::new()))
    } else {
        serde_json::Value::Array(blocks)
    }
}

/// 转换 OpenAI 工具定义到 Anthropic 格式
fn convert_openai_tools(tools: &Option<Vec<OpenAITool>>) -> Option<Vec<Tool>> {
    let tools = tools.as_ref()?;

    let anthropic_tools: Vec<Tool> = tools
        .iter()
        .filter(|t| t.tool_type == "function")
        .map(|t| {
            let input_schema: HashMap<String, serde_json::Value> = t
                .function
                .parameters
                .as_ref()
                .and_then(|p| serde_json::from_value(p.clone()).ok())
                .unwrap_or_else(|| {
                    let mut map = HashMap::new();
                    map.insert("type".to_string(), serde_json::json!("object"));
                    map.insert("properties".to_string(), serde_json::json!({}));
                    map
                });

            Tool {
                tool_type: None,
                name: t.function.name.clone(),
                description: t.function.description.clone().unwrap_or_default(),
                input_schema,
                max_uses: None,
            }
        })
        .collect();

    if anthropic_tools.is_empty() {
        None
    } else {
        Some(anthropic_tools)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_simple_request() {
        let req = ChatCompletionRequest {
            model: "gpt-4".to_string(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: Some(serde_json::json!("You are a helpful assistant.")),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: Some(serde_json::json!("Hello!")),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                },
            ],
            max_tokens: Some(1024),
            stream: false,
            temperature: None,
            top_p: None,
            tools: None,
            tool_choice: None,
            user: None,
        };

        let anthropic_req = convert_openai_to_anthropic(&req);

        assert_eq!(anthropic_req.model, "gpt-4");
        assert_eq!(anthropic_req.max_tokens, 1024);
        assert!(anthropic_req.system.is_some());
        assert_eq!(anthropic_req.messages.len(), 1);
        assert_eq!(anthropic_req.messages[0].role, "user");
    }

    #[test]
    fn test_convert_tool_calls() {
        let req = ChatCompletionRequest {
            model: "gpt-4".to_string(),
            messages: vec![
                ChatMessage {
                    role: "user".to_string(),
                    content: Some(serde_json::json!("What's the weather?")),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                },
                ChatMessage {
                    role: "assistant".to_string(),
                    content: None,
                    tool_calls: Some(vec![super::super::openai_types::ToolCall {
                        id: "call_123".to_string(),
                        call_type: "function".to_string(),
                        function: super::super::openai_types::FunctionCall {
                            name: "get_weather".to_string(),
                            arguments: r#"{"location": "Tokyo"}"#.to_string(),
                        },
                    }]),
                    tool_call_id: None,
                    name: None,
                },
                ChatMessage {
                    role: "tool".to_string(),
                    content: Some(serde_json::json!("Sunny, 25°C")),
                    tool_calls: None,
                    tool_call_id: Some("call_123".to_string()),
                    name: Some("get_weather".to_string()),
                },
            ],
            max_tokens: None,
            stream: false,
            temperature: None,
            top_p: None,
            tools: None,
            tool_choice: None,
            user: None,
        };

        let anthropic_req = convert_openai_to_anthropic(&req);

        // 应该有 3 条消息：user, assistant (with tool_use), user (with tool_result)
        assert_eq!(anthropic_req.messages.len(), 3);
        assert_eq!(anthropic_req.messages[0].role, "user");
        assert_eq!(anthropic_req.messages[1].role, "assistant");
        assert_eq!(anthropic_req.messages[2].role, "user");
    }

    #[test]
    fn test_parse_data_url() {
        let url = "data:image/png;base64,iVBORw0KGgo=";
        let result = parse_data_url(url);
        assert!(result.is_some());
        let (media_type, data) = result.unwrap();
        assert_eq!(media_type, "image/png");
        assert_eq!(data, "iVBORw0KGgo=");
    }
}
