use hyper_util::rt::TokioIo;
use shared::proto::{
    azookey_service_client::AzookeyServiceClient, AppendTextRequest, ClearTextRequest,
};
use tokio::net::windows::named_pipe::ClientOptions;
use tonic::transport::Endpoint;
use tower::service_fn;

// Verification client deliberately requires an isolated instance.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("AZOOKEY_INSTANCE")
        .unwrap_or_default()
        .is_empty()
    {
        return Err("Set AZOOKEY_INSTANCE to a test instance before querying".into());
    }
    let text = std::env::args().nth(1).ok_or("Provide input text")?;
    let channel = Endpoint::try_from("http://localhost")?
        .connect_with_connector(service_fn(|_| async {
            ClientOptions::new()
                .open(shared::pipe_path("azookey_server"))
                .map(TokioIo::new)
        }))
        .await?;
    let mut client = AzookeyServiceClient::new(channel);
    client.clear_text(ClearTextRequest {}).await?;
    let response = client
        .append_text(AppendTextRequest {
            text_to_append: text,
        })
        .await?
        .into_inner();
    let composed = response.composing_text.ok_or("Missing composing text")?;
    println!(
        "{}",
        serde_json::json!({
            "process_id": client.engine_status(shared::proto::EngineStatusRequest {}).await?.into_inner().process_id,
            "reading": composed.hiragana,
            "candidates": composed.suggestions.into_iter().map(|item| item.text).collect::<Vec<_>>()
        })
    );
    client.clear_text(ClearTextRequest {}).await?;
    Ok(())
}
