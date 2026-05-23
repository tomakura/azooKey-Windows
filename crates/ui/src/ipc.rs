use shared::proto::{
    window_service_server::WindowService as WindowServiceProto, EmptyResponse, SetCandidateRequest,
    SetInputModeRequest, SetPositionRequest, SetSelectionRequest,
};
use tokio::sync::mpsc;
use tonic::{Request, Response, Status};

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
