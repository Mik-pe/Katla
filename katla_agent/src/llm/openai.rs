use std::future::Future;
use std::pin::Pin;

use futures::Stream;
use futures::StreamExt;

use super::{ChatMessage, ChatResponse, FinishReason, LlmError, LlmProvider, StreamChunk, ToolDefinition};

use std::fmt;

use async_openai::Client as OpenAIClient;
use async_openai::config::OpenAIConfig;
use async_openai::types::{
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
    ChatCompletionRequestSystemMessageContent, ChatCompletionRequestToolMessage,
    ChatCompletionRequestToolMessageContent, ChatCompletionRequestUserMessage,
    ChatCompletionRequestUserMessageContent, ChatCompletionTool, ChatCompletionToolType,
    FunctionObject,
};

use crate::config::LlmConfig;

pub struct OpenAiProvider {
    client: OpenAIClient<OpenAIConfig>,
    model: String,
}

impl fmt::Debug for OpenAiProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenAiProvider")
            .field("model", &self.model)
            .finish_non_exhaustive()
    }
}

impl OpenAiProvider {
    pub fn new(api_key: &str, model: &str) -> Self {
        let config = OpenAIConfig::new().with_api_key(api_key);
        let client = OpenAIClient::with_config(config);
        Self {
            client,
            model: model.to_string(),
        }
    }

    pub fn with_base_url(api_key: &str, model: &str, base_url: &str) -> Self {
        let config = OpenAIConfig::new()
            .with_api_key(api_key)
            .with_api_base(base_url);
        let client = OpenAIClient::with_config(config);
        Self {
            client,
            model: model.to_string(),
        }
    }

    pub fn from_config(config: &LlmConfig) -> Result<Self, LlmError> {
        let api_key = config.resolve_api_key();
        if api_key.is_empty() {
            return Err(LlmError::Config(
                "API key is empty. Set api_key in llm.toml or via $OPENAI_API_KEY env var."
                    .to_string(),
            ));
        }
        if config.model.is_empty() {
            return Err(LlmError::Config(
                "model is not set in LLM config".to_string(),
            ));
        }

        Ok(match config.effective_base_url() {
            Some(url) => Self::with_base_url(&api_key, &config.model, url),
            None => Self::new(&api_key, &config.model),
        })
    }

    pub fn from_env(model: &str) -> Result<Self, LlmError> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| LlmError::Config("OPENAI_API_KEY not set".to_string()))?;
        Ok(Self::new(&api_key, model))
    }
}

fn convert_message(msg: &ChatMessage) -> Result<ChatCompletionRequestMessage, LlmError> {
    match msg.role {
        super::MessageRole::System => Ok(ChatCompletionRequestMessage::System(
            ChatCompletionRequestSystemMessage {
                content: ChatCompletionRequestSystemMessageContent::Text(msg.content.clone()),
                ..Default::default()
            },
        )),
        super::MessageRole::User => Ok(ChatCompletionRequestMessage::User(
            ChatCompletionRequestUserMessage {
                content: ChatCompletionRequestUserMessageContent::Text(msg.content.clone()),
                ..Default::default()
            },
        )),
        super::MessageRole::Assistant => Ok(ChatCompletionRequestMessage::Assistant(
            async_openai::types::ChatCompletionRequestAssistantMessage {
                content: Some(
                    async_openai::types::ChatCompletionRequestAssistantMessageContent::Text(
                        msg.content.clone(),
                    ),
                ),
                ..Default::default()
            },
        )),
        super::MessageRole::Tool => Ok(ChatCompletionRequestMessage::Tool(
            ChatCompletionRequestToolMessage {
                content: ChatCompletionRequestToolMessageContent::Text(msg.content.clone()),
                ..Default::default()
            },
        )),
    }
}

fn convert_tool(tool: &ToolDefinition) -> ChatCompletionTool {
    ChatCompletionTool {
        r#type: ChatCompletionToolType::Function,
        function: FunctionObject {
            name: tool.name.clone(),
            description: Some(tool.description.clone()),
            parameters: Some(tool.parameters.clone()),
            strict: Some(false),
        },
    }
}

impl LlmProvider for OpenAiProvider {
    fn chat_completion(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> Pin<Box<dyn Future<Output = Result<ChatResponse, LlmError>> + Send + '_>> {
        let openai_messages: Vec<ChatCompletionRequestMessage> = match messages
            .iter()
            .map(convert_message)
            .collect::<Result<_, _>>()
        {
            Ok(msgs) => msgs,
            Err(e) => return Box::pin(async move { Err(e) }),
        };

        let mut request = async_openai::types::CreateChatCompletionRequest {
            model: self.model.clone(),
            messages: openai_messages,
            ..Default::default()
        };

        if !tools.is_empty() {
            request.tools = Some(tools.iter().map(convert_tool).collect());
        }

        let client = self.client.clone();
        Box::pin(async move {
            let response = client
                .chat()
                .create(request)
                .await
                .map_err(|e| LlmError::Api(e.to_string()))?;

            let choice = response
                .choices
                .into_iter()
                .next()
                .ok_or_else(|| LlmError::Api("No response choices returned".to_string()))?;

            let finish_reason = match choice.finish_reason {
                Some(async_openai::types::FinishReason::Stop) => FinishReason::Stop,
                Some(async_openai::types::FinishReason::ToolCalls) => FinishReason::ToolCall,
                Some(async_openai::types::FinishReason::Length) => FinishReason::Length,
                _ => FinishReason::Stop,
            };

            let tool_calls = choice.message.tool_calls.map(|calls| {
                calls
                    .into_iter()
                    .map(|tc| super::ToolCall {
                        id: tc.id,
                        name: tc.function.name,
                        arguments: serde_json::from_str(&tc.function.arguments).unwrap_or_default(),
                    })
                    .collect()
            });

            let content = choice.message.content.unwrap_or_default();

            Ok(ChatResponse {
                message: ChatMessage {
                    role: super::MessageRole::Assistant,
                    content,
                    tool_calls,
                },
                finish_reason,
            })
        })
    }

    fn chat_completion_stream(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send + '_>> {
        let openai_messages: Vec<ChatCompletionRequestMessage> = match messages
            .iter()
            .map(convert_message)
            .collect::<Result<_, _>>()
        {
            Ok(msgs) => msgs,
            Err(e) => {
                return Box::pin(futures::stream::once(async move { Err(e) }));
            }
        };

        let mut request = async_openai::types::CreateChatCompletionRequest {
            model: self.model.clone(),
            messages: openai_messages,
            ..Default::default()
        };

        if !tools.is_empty() {
            request.tools = Some(tools.iter().map(convert_tool).collect());
        }

        let client = self.client.clone();

        // State: either we haven't initiated yet, or we're reading from the openai stream.
        type OpenAiStream = async_openai::types::ChatCompletionResponseStream;

        let state: Option<OpenAiStream> = None;

        Box::pin(futures::stream::unfold(
            (Some(client), Some(request), state),
            |(client_opt, request_opt, stream_opt)| async move {
                if let Some(mut stream) = stream_opt {
                    match stream.next().await {
                        Some(Ok(chunk)) => Some((
                            convert_stream_chunk(chunk),
                            (None, None, Some(stream)),
                        )),
                        Some(Err(e)) => {
                            Some((Err(LlmError::Api(e.to_string())), (None, None, None)))
                        }
                        None => None,
                    }
                } else if let (Some(client), Some(request)) = (client_opt, request_opt) {
                    match client.chat().create_stream(request).await {
                        Ok(mut stream) => {
                            match stream.next().await {
                                Some(Ok(chunk)) => Some((
                                    convert_stream_chunk(chunk),
                                    (None, None, Some(stream)),
                                )),
                                Some(Err(e)) => Some((
                                    Err(LlmError::Api(e.to_string())),
                                    (None, None, None),
                                )),
                                None => None,
                            }
                        }
                        Err(e) => Some((Err(LlmError::Api(e.to_string())), (None, None, None))),
                    }
                } else {
                    None
                }
            },
        ))
    }
}

fn convert_stream_chunk(
    chunk: async_openai::types::CreateChatCompletionStreamResponse,
) -> Result<StreamChunk, LlmError> {
    if let Some(choice) = chunk.choices.first() {
        let content_delta = choice.delta.content.clone().unwrap_or_default();
        let finish_reason = choice.finish_reason.map(|fr| match fr {
            async_openai::types::FinishReason::Stop => FinishReason::Stop,
            async_openai::types::FinishReason::ToolCalls => FinishReason::ToolCall,
            async_openai::types::FinishReason::Length => FinishReason::Length,
            _ => FinishReason::Stop,
        });
        Ok(StreamChunk {
            content_delta,
            finish_reason,
        })
    } else {
        Ok(StreamChunk {
            content_delta: String::new(),
            finish_reason: None,
        })
    }
}
