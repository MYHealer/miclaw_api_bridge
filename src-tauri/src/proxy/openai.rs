use super::transport::{emit_log, forward, list_models, map_err, proxy_response};
use super::ProxyController;
use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use bytes::Bytes;
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

const RESPONSES_MODE_UNKNOWN: u8 = 0;
const RESPONSES_MODE_PASSTHROUGH: u8 = 1;
const RESPONSES_MODE_COMPAT: u8 = 2;

static RESPONSES_MODE: AtomicU8 = AtomicU8::new(RESPONSES_MODE_UNKNOWN);

pub async fn chat(State(ctrl): State<Arc<ProxyController>>, Json(body): Json<Value>) -> Response {
    forward(ctrl.clone(), crate::mimo::PATH_CHAT, body).await
}

pub async fn responses(
    State(ctrl): State<Arc<ProxyController>>,
    Json(body): Json<Value>,
) -> Response {
    responses_passthrough_or_compat(ctrl, body).await
}

pub async fn models(State(ctrl): State<Arc<ProxyController>>) -> Response {
    list_models(ctrl).await
}

async fn responses_passthrough_or_compat(ctrl: Arc<ProxyController>, body: Value) -> Response {
    match RESPONSES_MODE.load(Ordering::Relaxed) {
        RESPONSES_MODE_PASSTHROUGH => {
            return forward(ctrl, crate::mimo::PATH_RESPONSES, body).await;
        }
        RESPONSES_MODE_COMPAT => {
            return responses_compat(ctrl, body).await;
        }
        _ => {}
    }

    let started = std::time::Instant::now();
    emit_log(
        &ctrl,
        json!({
            "ts": chrono::Utc::now().timestamp_millis(),
            "kind": "request",
            "path": crate::mimo::PATH_RESPONSES,
            "model": body.get("model").and_then(|v| v.as_str()).unwrap_or(""),
            "stream": body.get("stream").and_then(|v| v.as_bool()).unwrap_or(false),
        }),
    );

    match ctrl
        .mimo
        .post_json(crate::mimo::PATH_RESPONSES, body.clone())
        .await
    {
        Ok(upstream) if upstream.status() == reqwest::StatusCode::NOT_FOUND => {
            let _ = upstream.bytes().await;
            RESPONSES_MODE.store(RESPONSES_MODE_COMPAT, Ordering::Relaxed);
            tracing::info!(
                target = "proxy",
                "mimo PC responses endpoint returned 404; using chat-completions compatibility mode"
            );
            emit_log(
                &ctrl,
                json!({
                    "ts": chrono::Utc::now().timestamp_millis(),
                    "kind": "response",
                    "path": crate::mimo::PATH_RESPONSES,
                    "status": 404,
                    "elapsed_ms": started.elapsed().as_millis() as u64,
                }),
            );
            responses_compat(ctrl, body).await
        }
        Ok(upstream) => {
            let status = upstream.status();
            if status.is_success() {
                RESPONSES_MODE.store(RESPONSES_MODE_PASSTHROUGH, Ordering::Relaxed);
            }
            emit_log(
                &ctrl,
                json!({
                    "ts": chrono::Utc::now().timestamp_millis(),
                    "kind": "response",
                    "path": crate::mimo::PATH_RESPONSES,
                    "status": status.as_u16(),
                    "elapsed_ms": started.elapsed().as_millis() as u64,
                }),
            );
            proxy_response(upstream).await
        }
        Err(e) => {
            emit_log(
                &ctrl,
                json!({
                    "ts": chrono::Utc::now().timestamp_millis(),
                    "kind": "error",
                    "path": crate::mimo::PATH_RESPONSES,
                    "message": e.to_string(),
                    "elapsed_ms": started.elapsed().as_millis() as u64,
                }),
            );
            map_err(e)
        }
    }
}

async fn responses_compat(ctrl: Arc<ProxyController>, body: Value) -> Response {
    let stream = body
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let chat_body = responses_to_chat_body(&body, stream);
    let started = std::time::Instant::now();
    emit_log(
        &ctrl,
        json!({
            "ts": chrono::Utc::now().timestamp_millis(),
            "kind": "request",
            "path": "/v1/responses -> /v1/chat/completions",
            "model": body.get("model").and_then(|v| v.as_str()).unwrap_or(""),
            "stream": stream,
        }),
    );

    match ctrl.mimo.post_json(crate::mimo::PATH_CHAT, chat_body).await {
        Ok(upstream) if upstream.status().is_success() && stream => {
            emit_log(
                &ctrl,
                json!({
                    "ts": chrono::Utc::now().timestamp_millis(),
                    "kind": "response",
                    "path": "/v1/responses -> /v1/chat/completions",
                    "status": upstream.status().as_u16(),
                    "elapsed_ms": started.elapsed().as_millis() as u64,
                }),
            );
            responses_stream_from_chat(upstream, body).await
        }
        Ok(upstream) if upstream.status().is_success() => {
            let status = upstream.status();
            match upstream.json::<Value>().await {
                Ok(chat) => {
                    emit_log(
                        &ctrl,
                        json!({
                            "ts": chrono::Utc::now().timestamp_millis(),
                            "kind": "response",
                            "path": "/v1/responses -> /v1/chat/completions",
                            "status": status.as_u16(),
                            "elapsed_ms": started.elapsed().as_millis() as u64,
                        }),
                    );
                    Json(response_from_chat(&body, &chat)).into_response()
                }
                Err(e) => map_err(crate::error::BridgeError::from(e)),
            }
        }
        Ok(upstream) => proxy_response(upstream).await,
        Err(e) => {
            emit_log(
                &ctrl,
                json!({
                    "ts": chrono::Utc::now().timestamp_millis(),
                    "kind": "error",
                    "path": "/v1/responses -> /v1/chat/completions",
                    "message": e.to_string(),
                    "elapsed_ms": started.elapsed().as_millis() as u64,
                }),
            );
            map_err(e)
        }
    }
}

fn responses_to_chat_body(body: &Value, stream: bool) -> Value {
    let mut out = serde_json::Map::new();
    out.insert(
        "model".into(),
        body.get("model")
            .cloned()
            .unwrap_or_else(|| json!(crate::mimo::MODEL_DEFAULT)),
    );
    out.insert("stream".into(), Value::Bool(stream));

    if stream {
        out.insert(
            "stream_options".into(),
            json!({
                "include_usage": body
                    .pointer("/stream_options/include_usage")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true),
            }),
        );
    }

    copy_field(body, &mut out, "temperature", "temperature");
    copy_field(body, &mut out, "top_p", "top_p");
    copy_field(body, &mut out, "max_output_tokens", "max_tokens");
    copy_field(body, &mut out, "tool_choice", "tool_choice");
    copy_tools(body, &mut out);

    let mut messages = Vec::new();
    if let Some(instructions) = body.get("instructions") {
        if let Some(text) = text_from_value(instructions) {
            messages.push(json!({"role": "system", "content": text}));
        }
    }
    messages.extend(messages_from_input(body.get("input")));
    if messages.is_empty() {
        messages.push(json!({"role": "user", "content": ""}));
    }
    out.insert("messages".into(), Value::Array(messages));
    Value::Object(out)
}

fn copy_field(body: &Value, out: &mut serde_json::Map<String, Value>, from: &str, to: &str) {
    if let Some(value) = body.get(from) {
        out.insert(to.into(), value.clone());
    }
}

fn copy_tools(body: &Value, out: &mut serde_json::Map<String, Value>) {
    let Some(tools) = body.get("tools").and_then(|v| v.as_array()) else {
        return;
    };
    let converted: Vec<Value> = tools
        .iter()
        .filter_map(|tool| {
            if tool.get("type").and_then(|v| v.as_str()) == Some("function") {
                if tool.get("function").is_some() {
                    return Some(tool.clone());
                }
                return Some(json!({
                    "type": "function",
                    "function": {
                        "name": tool.get("name").cloned().unwrap_or_else(|| json!("function_tool")),
                        "description": tool.get("description").cloned().unwrap_or(Value::Null),
                        "parameters": tool.get("parameters").cloned().unwrap_or_else(|| json!({"type": "object"})),
                    }
                }));
            }
            if tool.get("type").and_then(|v| v.as_str()) == Some("custom") {
                return Some(json!({
                    "type": "function",
                    "function": {
                        "name": tool.get("name").cloned().unwrap_or_else(|| json!("custom_tool")),
                        "description": tool.get("description").cloned().unwrap_or(Value::Null),
                        "parameters": tool.get("parameters").cloned().unwrap_or_else(|| json!({"type": "object"})),
                    }
                }));
            }
            None
        })
        .collect();
    if !converted.is_empty() {
        out.insert("tools".into(), Value::Array(converted));
    }
}

fn messages_from_input(input: Option<&Value>) -> Vec<Value> {
    match input {
        Some(Value::String(s)) => vec![json!({"role": "user", "content": s})],
        Some(Value::Array(items)) => items.iter().filter_map(message_from_input_item).collect(),
        Some(other) => text_from_value(other)
            .map(|text| vec![json!({"role": "user", "content": text})])
            .unwrap_or_default(),
        None => Vec::new(),
    }
}

fn message_from_input_item(item: &Value) -> Option<Value> {
    if let Some(text) = item.as_str() {
        return Some(json!({"role": "user", "content": text}));
    }

    let obj = item.as_object()?;
    let typ = obj.get("type").and_then(|v| v.as_str());
    if typ == Some("input_text") {
        return obj
            .get("text")
            .and_then(|v| v.as_str())
            .map(|text| json!({"role": "user", "content": text}));
    }

    let role = obj
        .get("role")
        .and_then(|v| v.as_str())
        .map(chat_role)
        .unwrap_or("user");
    let content = obj.get("content").unwrap_or(item);
    Some(json!({
        "role": role,
        "content": chat_content_from_responses_content(content, role == "user"),
    }))
}

fn chat_role(role: &str) -> &'static str {
    match role {
        "assistant" => "assistant",
        "system" | "developer" => "system",
        "tool" => "tool",
        _ => "user",
    }
}

fn chat_content_from_responses_content(content: &Value, allow_parts: bool) -> Value {
    match content {
        Value::String(s) => Value::String(s.clone()),
        Value::Array(blocks) if allow_parts => {
            let parts: Vec<Value> = blocks
                .iter()
                .filter_map(|block| {
                    let typ = block.get("type").and_then(|v| v.as_str())?;
                    match typ {
                        "input_text" | "output_text" => block
                            .get("text")
                            .and_then(|v| v.as_str())
                            .map(|text| json!({"type": "text", "text": text})),
                        "input_image" => {
                            let url = block
                                .get("image_url")
                                .or_else(|| block.get("file_data"))
                                .and_then(|v| v.as_str())?;
                            Some(json!({"type": "image_url", "image_url": {"url": url}}))
                        }
                        _ => None,
                    }
                })
                .collect();
            if parts.is_empty() {
                Value::String(text_from_blocks(blocks))
            } else {
                Value::Array(parts)
            }
        }
        Value::Array(blocks) => Value::String(text_from_blocks(blocks)),
        other => text_from_value(other)
            .map(Value::String)
            .unwrap_or_else(|| Value::String(String::new())),
    }
}

fn text_from_blocks(blocks: &[Value]) -> String {
    blocks
        .iter()
        .filter_map(text_from_value)
        .collect::<Vec<_>>()
        .join("\n")
}

fn text_from_value(value: &Value) -> Option<String> {
    if let Some(s) = value.as_str() {
        return Some(s.to_string());
    }
    if let Some(text) = value.get("text").and_then(|v| v.as_str()) {
        return Some(text.to_string());
    }
    if let Some(text) = value.get("content").and_then(|v| v.as_str()) {
        return Some(text.to_string());
    }
    None
}

fn response_from_chat(request: &Value, chat: &Value) -> Value {
    let id = new_response_id("resp");
    let msg_id = new_response_id("msg");
    let created_at = chrono::Utc::now().timestamp();
    let model = chat
        .get("model")
        .or_else(|| request.get("model"))
        .cloned()
        .unwrap_or_else(|| json!(crate::mimo::MODEL_DEFAULT));
    let message = chat.pointer("/choices/0/message").unwrap_or(&Value::Null);
    let text = message
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let reasoning = message
        .get("reasoning_content")
        .or_else(|| message.get("reasoning"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let usage = usage_from_chat(chat.get("usage"), &reasoning);
    let output = response_output(&msg_id, &text, &reasoning);

    json!({
        "id": id,
        "object": "response",
        "created_at": created_at,
        "status": "completed",
        "error": null,
        "incomplete_details": null,
        "instructions": request.get("instructions").cloned().unwrap_or(Value::Null),
        "max_output_tokens": request.get("max_output_tokens").cloned().unwrap_or(Value::Null),
        "model": model,
        "output": output,
        "output_text": text,
        "parallel_tool_calls": request.get("parallel_tool_calls").cloned().unwrap_or(Value::Bool(true)),
        "previous_response_id": request.get("previous_response_id").cloned().unwrap_or(Value::Null),
        "reasoning": request.get("reasoning").cloned().unwrap_or_else(|| json!({"effort": null, "summary": null})),
        "store": request.get("store").cloned().unwrap_or(Value::Bool(true)),
        "temperature": request.get("temperature").cloned().unwrap_or_else(|| json!(1)),
        "text": request.get("text").cloned().unwrap_or_else(|| json!({"format": {"type": "text"}})),
        "tool_choice": request.get("tool_choice").cloned().unwrap_or_else(|| json!("auto")),
        "tools": request.get("tools").cloned().unwrap_or_else(|| json!([])),
        "top_p": request.get("top_p").cloned().unwrap_or_else(|| json!(1)),
        "truncation": request.get("truncation").cloned().unwrap_or_else(|| json!("disabled")),
        "usage": usage,
        "user": request.get("user").cloned().unwrap_or(Value::Null),
        "metadata": request.get("metadata").cloned().unwrap_or_else(|| json!({})),
    })
}

fn response_output(msg_id: &str, text: &str, reasoning: &str) -> Vec<Value> {
    let mut output = Vec::new();
    if !reasoning.is_empty() {
        output.push(json!({
            "id": new_response_id("rs"),
            "type": "reasoning",
            "summary": [{"type": "summary_text", "text": reasoning}],
        }));
    }
    output.push(json!({
        "id": msg_id,
        "type": "message",
        "status": "completed",
        "role": "assistant",
        "content": [{
            "type": "output_text",
            "text": text,
            "annotations": [],
        }],
    }));
    output
}

fn usage_from_chat(usage: Option<&Value>, reasoning: &str) -> Value {
    let prompt = usage
        .and_then(|u| u.get("prompt_tokens"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let completion = usage
        .and_then(|u| u.get("completion_tokens"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let total = usage
        .and_then(|u| u.get("total_tokens"))
        .and_then(|v| v.as_i64())
        .unwrap_or(prompt + completion);
    json!({
        "input_tokens": prompt,
        "input_tokens_details": {"cached_tokens": 0},
        "output_tokens": completion,
        "output_tokens_details": {
            "reasoning_tokens": if reasoning.is_empty() { 0 } else { completion },
        },
        "total_tokens": total,
    })
}

async fn responses_stream_from_chat(upstream: reqwest::Response, request: Value) -> Response {
    let response_id = new_response_id("resp");
    let msg_id = new_response_id("msg");
    let created_at = chrono::Utc::now().timestamp();
    let model = request
        .get("model")
        .cloned()
        .unwrap_or_else(|| json!(crate::mimo::MODEL_DEFAULT));

    let (tx, rx) = mpsc::channel::<Result<Bytes, std::io::Error>>(32);
    tokio::spawn(async move {
        let mut seq = 1_i64;
        let mut text = String::new();
        let mut reasoning = String::new();
        let mut usage = Value::Null;
        let mut buffer = String::new();
        let mut stream = upstream.bytes_stream();

        send_event(
            &tx,
            response_event(
                "response.created",
                seq,
                "in_progress",
                &response_id,
                created_at,
                model.clone(),
                Vec::new(),
                "",
                Value::Null,
                &request,
            ),
        )
        .await;
        seq += 1;
        send_event(
            &tx,
            response_event(
                "response.in_progress",
                seq,
                "in_progress",
                &response_id,
                created_at,
                model.clone(),
                Vec::new(),
                "",
                Value::Null,
                &request,
            ),
        )
        .await;
        seq += 1;
        send_event(
            &tx,
            json!({
                "type": "response.output_item.added",
                "output_index": 0,
                "item": {
                    "id": msg_id,
                    "type": "message",
                    "status": "in_progress",
                    "role": "assistant",
                    "content": [],
                },
                "sequence_number": seq,
            }),
        )
        .await;
        seq += 1;
        send_event(
            &tx,
            json!({
                "type": "response.content_part.added",
                "item_id": msg_id,
                "output_index": 0,
                "content_index": 0,
                "part": {"type": "output_text", "text": "", "annotations": []},
                "sequence_number": seq,
            }),
        )
        .await;
        seq += 1;

        while let Some(chunk) = stream.next().await {
            let Ok(chunk) = chunk else {
                send_event(
                    &tx,
                    json!({
                        "type": "error",
                        "code": "upstream_stream_error",
                        "message": "upstream stream ended with an error",
                        "sequence_number": seq,
                    }),
                )
                .await;
                return;
            };
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(packet) = take_sse_packet(&mut buffer) {
                let payload = sse_payload(&packet);
                if payload.is_empty() {
                    continue;
                }
                if payload == "[DONE]" {
                    break;
                }
                let Ok(value) = serde_json::from_str::<Value>(&payload) else {
                    continue;
                };
                if let Some(u) = value.get("usage") {
                    usage = usage_from_chat(Some(u), &reasoning);
                }
                let choice = value.pointer("/choices/0").unwrap_or(&Value::Null);
                let delta = choice.get("delta").unwrap_or(&Value::Null);
                if let Some(piece) = delta
                    .get("reasoning_content")
                    .or_else(|| delta.get("reasoning"))
                    .and_then(|v| v.as_str())
                {
                    reasoning.push_str(piece);
                    send_event(
                        &tx,
                        json!({
                            "type": "response.reasoning_text.delta",
                            "item_id": msg_id,
                            "output_index": 0,
                            "content_index": 0,
                            "delta": piece,
                            "sequence_number": seq,
                        }),
                    )
                    .await;
                    seq += 1;
                }
                if let Some(piece) = delta.get("content").and_then(|v| v.as_str()) {
                    text.push_str(piece);
                    send_event(
                        &tx,
                        json!({
                            "type": "response.output_text.delta",
                            "item_id": msg_id,
                            "output_index": 0,
                            "content_index": 0,
                            "delta": piece,
                            "sequence_number": seq,
                        }),
                    )
                    .await;
                    seq += 1;
                }
            }
        }

        if usage.is_null() {
            usage = usage_from_chat(None, &reasoning);
        }
        send_event(
            &tx,
            json!({
                "type": "response.output_text.done",
                "item_id": msg_id,
                "output_index": 0,
                "content_index": 0,
                "text": text,
                "sequence_number": seq,
            }),
        )
        .await;
        seq += 1;
        send_event(
            &tx,
            json!({
                "type": "response.content_part.done",
                "item_id": msg_id,
                "output_index": 0,
                "content_index": 0,
                "part": {"type": "output_text", "text": text, "annotations": []},
                "sequence_number": seq,
            }),
        )
        .await;
        seq += 1;
        let output = response_output(&msg_id, &text, &reasoning);
        send_event(
            &tx,
            json!({
                "type": "response.output_item.done",
                "output_index": 0,
                "item": output.last().cloned().unwrap_or(Value::Null),
                "sequence_number": seq,
            }),
        )
        .await;
        seq += 1;
        send_event(
            &tx,
            response_event(
                "response.completed",
                seq,
                "completed",
                &response_id,
                created_at,
                model,
                output,
                &text,
                usage,
                &request,
            ),
        )
        .await;
    });

    let body_stream = futures_util::stream::unfold(rx, |mut rx| async {
        rx.recv().await.map(|item| (item, rx))
    });
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("text/event-stream"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-cache"),
    );
    let mut resp = Response::new(Body::from_stream(body_stream));
    *resp.status_mut() = StatusCode::OK;
    *resp.headers_mut() = headers;
    resp
}

#[allow(clippy::too_many_arguments)]
fn response_event(
    event_type: &str,
    sequence_number: i64,
    status: &str,
    id: &str,
    created_at: i64,
    model: Value,
    output: Vec<Value>,
    output_text: &str,
    usage: Value,
    request: &Value,
) -> Value {
    let mut response = response_from_chat(
        request,
        &json!({
            "model": model,
            "usage": usage,
            "choices": [{
                "message": {
                    "content": output_text,
                }
            }]
        }),
    );
    response["id"] = json!(id);
    response["created_at"] = json!(created_at);
    response["status"] = json!(status);
    response["output"] = Value::Array(output);
    response["usage"] = usage;
    json!({
        "type": event_type,
        "response": response,
        "sequence_number": sequence_number,
    })
}

async fn send_event(tx: &mpsc::Sender<Result<Bytes, std::io::Error>>, value: Value) {
    let event_type = value
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("message");
    let Ok(data) = serde_json::to_string(&value) else {
        return;
    };
    let frame = format!("event: {event_type}\ndata: {data}\n\n");
    let _ = tx.send(Ok(Bytes::from(frame))).await;
}

fn take_sse_packet(buffer: &mut String) -> Option<String> {
    let lf = buffer.find("\n\n").map(|idx| (idx, 2));
    let crlf = buffer.find("\r\n\r\n").map(|idx| (idx, 4));
    let (idx, sep_len) = match (lf, crlf) {
        (Some(a), Some(b)) => {
            if a.0 < b.0 {
                a
            } else {
                b
            }
        }
        (Some(a), None) => a,
        (None, Some(b)) => b,
        (None, None) => return None,
    };
    let packet = buffer[..idx].to_string();
    buffer.drain(..idx + sep_len);
    Some(packet)
}

fn sse_payload(packet: &str) -> String {
    packet
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim_start)
        .collect::<Vec<_>>()
        .join("\n")
}

fn new_response_id(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::new_v4().simple())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responses_string_input_maps_to_chat_messages() {
        let body = json!({
            "model": "mimo-pro",
            "input": "hi",
            "max_output_tokens": 32,
        });
        let chat = responses_to_chat_body(&body, false);
        assert_eq!(chat["model"], "mimo-pro");
        assert_eq!(chat["max_tokens"], 32);
        assert_eq!(chat["messages"][0]["role"], "user");
        assert_eq!(chat["messages"][0]["content"], "hi");
    }

    #[test]
    fn responses_message_input_preserves_system_and_images() {
        let body = json!({
            "instructions": "be terse",
            "input": [{
                "role": "user",
                "content": [
                    {"type": "input_text", "text": "describe"},
                    {"type": "input_image", "image_url": "data:image/png;base64,abc"}
                ]
            }]
        });
        let chat = responses_to_chat_body(&body, true);
        assert_eq!(chat["stream"], true);
        assert_eq!(chat["messages"][0]["role"], "system");
        assert_eq!(chat["messages"][1]["content"][0]["type"], "text");
        assert_eq!(chat["messages"][1]["content"][1]["type"], "image_url");
    }

    #[test]
    fn chat_response_maps_to_responses_output_text() {
        let req = json!({"model": "mimo-pro", "input": "hi"});
        let chat = json!({
            "model": "mimo-pro",
            "choices": [{"message": {"content": "hello", "reasoning_content": "thinking"}}],
            "usage": {"prompt_tokens": 3, "completion_tokens": 4, "total_tokens": 7}
        });
        let response = response_from_chat(&req, &chat);
        assert_eq!(response["object"], "response");
        assert_eq!(response["status"], "completed");
        assert_eq!(response["output_text"], "hello");
        assert_eq!(response["usage"]["total_tokens"], 7);
        assert_eq!(response["output"][0]["type"], "reasoning");
    }
}
