use crate::intelligence::gateway::{GatewayRequest, Message, OllamaGateway};

/// Routes a user prompt through the local Ollama Gateway using
/// the deepseek-coder model, returning the generated response.
pub async fn route_through_gateway(prompt: String) -> Result<String, String> {
    route_with_system(
        "You are an expert software engineer embedded in the NeuralForge IDE. Provide concise, accurate code solutions.",
        &prompt,
    )
    .await
}

/// Routes a prompt with a custom system message through the Ollama Gateway.
pub async fn route_with_system(system_prompt: &str, user_prompt: &str) -> Result<String, String> {
    let gateway = OllamaGateway::new()?;

    let request = GatewayRequest {
        model: "deepseek-coder:latest".to_string(),
        messages: vec![
            Message {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            Message {
                role: "user".to_string(),
                content: user_prompt.to_string(),
            },
        ],
        temperature: 0.3,
        stream: false,
    };

    gateway.execute_chat(request).await
}