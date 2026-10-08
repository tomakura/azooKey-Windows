use hyper_util::rt::TokioIo;
use shared::proto::{
    window_service_client::WindowServiceClient, CandidateItem, EmptyResponse, SetCandidateRequest,
    SetInputModeRequest, SetPositionRequest, SetSelectionRequest, WindowPosition,
};
use tokio::net::windows::named_pipe::ClientOptions;
use tonic::transport::Endpoint;
use tower::service_fn;

// Exercises the real candidate UI over its public protocol, without changing the active IME.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("AZOOKEY_INSTANCE")
        .unwrap_or_default()
        .is_empty()
    {
        return Err("Set AZOOKEY_INSTANCE to an isolated UI instance".into());
    }
    let channel = Endpoint::try_from("http://localhost")?
        .connect_with_connector(service_fn(|_| async {
            ClientOptions::new()
                .open(shared::pipe_path("azookey_ui"))
                .map(TokioIo::new)
        }))
        .await?;
    let mut client = WindowServiceClient::new(channel);
    client
        .set_candidate(SetCandidateRequest {
            candidates: vec![],
            candidate_items: vec![
                CandidateItem {
                    text: "検証候補".into(),
                    subtext: "ユーザー辞書".into(),
                },
                CandidateItem {
                    text: "漢字".into(),
                    subtext: String::new(),
                },
            ],
        })
        .await?;
    client
        .set_selection(SetSelectionRequest { index: 0 })
        .await?;
    client
        .set_input_mode(SetInputModeRequest { mode: "あ".into() })
        .await?;
    client
        .set_window_position(SetPositionRequest {
            position: Some(WindowPosition {
                top: 100,
                left: 100,
                bottom: 130,
                right: 120,
            }),
        })
        .await?;
    client.show_window(EmptyResponse {}).await?;
    println!("Real UI protocol: candidates, selection, input mode, position, show succeeded");
    Ok(())
}
