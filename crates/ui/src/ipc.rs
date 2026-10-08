use shared::proto::{
    window_service_server::WindowService as WindowServiceProto, EmptyResponse, SetCandidateRequest,
    SetInputModeRequest, SetPositionRequest, SetSelectionRequest,
};
use tokio::sync::mpsc;
use tonic::{Request, Response, Status};

pub async fn update_server_config() -> anyhow::Result<()> {
    let channel = tonic::transport::Endpoint::try_from("http://localhost")?
        .connect_with_connector(tower::service_fn(|_| async {
            tokio::net::windows::named_pipe::ClientOptions::new()
                .open(shared::pipe_path("azookey_server"))
                .map(hyper_util::rt::TokioIo::new)
        }))
        .await?;
    shared::proto::azookey_service_client::AzookeyServiceClient::new(channel)
        .update_config(shared::proto::UpdateConfigRequest {})
        .await?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct WindowController {
    sender: mpsc::Sender<WindowAction>,
}

impl WindowController {
    pub fn new(sender: mpsc::Sender<WindowAction>) -> Self {
        Self { sender }
    }

    async fn send(&self, action: WindowAction) -> Result<(), Status> {
        self.sender
            .send(action)
            .await
            .map_err(|_| Status::unavailable("window event loop is closed"))
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CandidateView {
    pub text: String,
    pub subtext: String,
}

// ウィンドウ操作コマンド
#[derive(Debug, serde::Serialize)]
pub enum WindowAction {
    Show,
    Hide,
    SetPosition {
        top: i32,
        left: i32,
        bottom: i32,
        right: i32,
    },
    SetSelection {
        index: i32,
    },
    SetCandidate {
        candidates: Vec<CandidateView>,
    },
    SetInputMode(String),
}

#[derive(Debug)]
pub struct WindowService {
    pub controller: WindowController,
    pub candidate_request: tokio::sync::Mutex<String>,
}

#[tonic::async_trait]
impl WindowServiceProto for WindowService {
    async fn show_window(
        &self,
        _request: Request<EmptyResponse>,
    ) -> Result<Response<EmptyResponse>, Status> {
        self.controller.send(WindowAction::Show).await?;
        Ok(Response::new(EmptyResponse {}))
    }

    async fn hide_window(
        &self,
        _request: Request<EmptyResponse>,
    ) -> Result<Response<EmptyResponse>, Status> {
        self.controller.send(WindowAction::Hide).await?;
        Ok(Response::new(EmptyResponse {}))
    }
    async fn set_window_position(
        &self,
        request: Request<SetPositionRequest>,
    ) -> Result<Response<EmptyResponse>, Status> {
        let position = request
            .into_inner()
            .position
            .ok_or_else(|| Status::invalid_argument("position is required"))?;
        let top = position.top;
        let left = position.left;
        let bottom = position.bottom;
        let right = position.right;
        self.controller
            .send(WindowAction::SetPosition {
                top,
                left,
                bottom,
                right,
            })
            .await?;

        Ok(Response::new(EmptyResponse {}))
    }

    async fn set_candidate(
        &self,
        request: Request<SetCandidateRequest>,
    ) -> Result<Response<EmptyResponse>, Status> {
        let request = request.into_inner();
        // Serialize activation and delivery so an older prediction cannot overwrite new input.
        let mut active = self.candidate_request.lock().await;
        if request.activate {
            *active = request.request_id.clone();
        } else if *active != request.request_id {
            return Ok(Response::new(EmptyResponse {}));
        }
        let candidates = if request.candidate_items.is_empty() {
            request
                .candidates
                .into_iter()
                .map(|text| CandidateView {
                    text,
                    subtext: String::new(),
                })
                .collect()
        } else {
            request
                .candidate_items
                .into_iter()
                .map(|candidate| CandidateView {
                    text: candidate.text,
                    subtext: candidate.subtext,
                })
                .collect()
        };

        self.controller
            .send(WindowAction::SetCandidate { candidates })
            .await?;

        Ok(Response::new(EmptyResponse {}))
    }

    async fn set_selection(
        &self,
        request: Request<SetSelectionRequest>,
    ) -> Result<Response<EmptyResponse>, Status> {
        let index = request.into_inner().index;
        self.controller
            .send(WindowAction::SetSelection { index })
            .await?;

        Ok(Response::new(EmptyResponse {}))
    }

    async fn set_input_mode(
        &self,
        request: Request<SetInputModeRequest>,
    ) -> Result<Response<EmptyResponse>, Status> {
        let mode = request.into_inner().mode;
        self.controller
            .send(WindowAction::SetInputMode(mode))
            .await?;

        Ok(Response::new(EmptyResponse {}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stale_predictions_cannot_replace_new_input_or_conversion() {
        let (sender, mut receiver) = mpsc::channel(8);
        let service = WindowService {
            controller: WindowController::new(sender),
            candidate_request: tokio::sync::Mutex::default(),
        };
        for (id, activate, text) in [
            ("old", true, ""),
            ("new", true, ""),
            ("old", false, "古い予測"),
            ("new", false, "新しい予測"),
            ("space", true, "通常変換"),
            ("new", false, "遅れた予測"),
        ] {
            service
                .set_candidate(Request::new(SetCandidateRequest {
                    candidates: vec![text.into()],
                    candidate_items: vec![],
                    request_id: id.into(),
                    activate,
                }))
                .await
                .unwrap();
        }
        let mut displayed = Vec::new();
        while let Ok(WindowAction::SetCandidate { candidates }) = receiver.try_recv() {
            displayed.push(candidates[0].text.clone());
        }
        assert_eq!(displayed, ["", "", "新しい予測", "通常変換"]);
    }
}
