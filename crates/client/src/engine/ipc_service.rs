use anyhow::Result;
use hyper_util::rt::TokioIo;
use shared::proto::{
    azookey_service_client::AzookeyServiceClient, window_service_client::WindowServiceClient,
};
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::{net::windows::named_pipe::ClientOptions, time};
use tonic::transport::Endpoint;
use tower::service_fn;
use windows::Win32::Foundation::ERROR_PIPE_BUSY;

// connect to kkc server
#[derive(Debug, Clone)]
pub struct IPCService {
    // kkc server client
    azookey_client: AzookeyServiceClient<tonic::transport::channel::Channel>,
    // candidate window server client
    window_client: WindowServiceClient<tonic::transport::channel::Channel>,
    runtime: Arc<tokio::runtime::Runtime>,
    prediction: Arc<Mutex<PredictionState>>,
    context: Arc<Mutex<String>>,
    raw_input: Arc<Mutex<String>>,
}

static NEXT_REQUEST: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Default)]
struct PredictionState {
    request_id: String,
    reading: String,
    result: Option<std::result::Result<Candidates, String>>,
    task: Option<tokio::task::AbortHandle>,
}

fn request_id() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        NEXT_REQUEST.fetch_add(1, Ordering::Relaxed)
    )
}

impl From<shared::proto::ComposingText> for Candidates {
    fn from(text: shared::proto::ComposingText) -> Self {
        Self {
            texts: text.suggestions.iter().map(|s| s.text.clone()).collect(),
            sub_texts: text.suggestions.iter().map(|s| s.subtext.clone()).collect(),
            corresponding_count: text
                .suggestions
                .iter()
                .map(|s| s.corresponding_count)
                .collect(),
            is_prediction: text.suggestions.iter().map(|s| s.is_prediction).collect(),
            hiragana: text.hiragana,
            raw_input: text.raw_input,
        }
    }
}

fn window_candidates(
    candidates: &Candidates,
    request_id: String,
    activate: bool,
) -> shared::proto::SetCandidateRequest {
    shared::proto::SetCandidateRequest {
        candidates: candidates.texts.clone(),
        candidate_items: candidates
            .texts
            .iter()
            .enumerate()
            .map(|(index, text)| shared::proto::CandidateItem {
                text: text.clone(),
                subtext: if candidates.is_prediction.get(index) == Some(&true) {
                    "予測 · Tab".to_string()
                } else {
                    candidates.sub_texts.get(index).cloned().unwrap_or_default()
                },
            })
            .collect(),
        request_id,
        activate,
    }
}

#[derive(Debug, Clone, Default)]
pub struct Candidates {
    pub texts: Vec<String>,
    pub sub_texts: Vec<String>,
    pub hiragana: String,
    pub corresponding_count: Vec<i32>,
    pub is_prediction: Vec<bool>,
    pub raw_input: String,
}

impl Candidates {
    pub fn is_latin_input(&self) -> bool {
        self.raw_input.is_ascii()
            && self
                .raw_input
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_uppercase())
    }
}

impl IPCService {
    pub fn new() -> Result<Self> {
        let runtime = tokio::runtime::Runtime::new()?;

        let server_channel = runtime.block_on(
            Endpoint::try_from("http://[::]:50051")?.connect_with_connector(service_fn(
                |_| async {
                    let client = loop {
                        match ClientOptions::new().open(shared::pipe_path("azookey_server")) {
                            Ok(client) => break client,
                            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY.0 as i32) => (),
                            Err(e) => return Err(e),
                        }

                        time::sleep(Duration::from_millis(50)).await;
                    };

                    Ok::<_, std::io::Error>(TokioIo::new(client))
                },
            )),
        )?;

        let ui_channel = runtime.block_on(
            Endpoint::try_from("http://[::]:50052")?.connect_with_connector(service_fn(
                |_| async {
                    let client = loop {
                        match ClientOptions::new().open(shared::pipe_path("azookey_ui")) {
                            Ok(client) => break client,
                            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY.0 as i32) => (),
                            Err(e) => return Err(e),
                        }

                        time::sleep(Duration::from_millis(50)).await;
                    };

                    Ok::<_, std::io::Error>(TokioIo::new(client))
                },
            )),
        )?;

        let azookey_client = AzookeyServiceClient::new(server_channel);
        let window_client = WindowServiceClient::new(ui_channel);
        tracing::debug!("Connected to server: {:?}", azookey_client);

        Ok(Self {
            azookey_client,
            window_client,
            runtime: Arc::new(runtime),
            prediction: Arc::default(),
            context: Arc::default(),
            raw_input: Arc::default(),
        })
    }
}

// implement methods to interact with kkc server
impl IPCService {
    #[tracing::instrument]
    pub fn append_text(&mut self, text: String) -> anyhow::Result<Candidates> {
        let request = tonic::Request::new(shared::proto::AppendTextRequest {
            text_to_append: text,
            preview_only: true,
        });

        let response = self
            .runtime
            .clone()
            .block_on(self.azookey_client.append_text(request))?;
        let composing_text = response.into_inner().composing_text;

        let candidates = if let Some(composing_text) = composing_text {
            Candidates {
                texts: composing_text
                    .suggestions
                    .iter()
                    .map(|s| s.text.clone())
                    .collect(),
                sub_texts: composing_text
                    .suggestions
                    .iter()
                    .map(|s| s.subtext.clone())
                    .collect(),
                is_prediction: composing_text
                    .suggestions
                    .iter()
                    .map(|s| s.is_prediction)
                    .collect(),
                hiragana: composing_text.hiragana,
                raw_input: composing_text.raw_input,
                corresponding_count: composing_text
                    .suggestions
                    .iter()
                    .map(|s| s.corresponding_count)
                    .collect(),
            }
        } else {
            anyhow::bail!("composing_text is None");
        };

        self.live_candidates(candidates)
    }

    #[tracing::instrument]
    pub fn remove_text(&mut self) -> anyhow::Result<Candidates> {
        let request = tonic::Request::new(shared::proto::RemoveTextRequest { preview_only: true });
        let response = self
            .runtime
            .clone()
            .block_on(self.azookey_client.remove_text(request))?;
        let composing_text = response.into_inner().composing_text;

        let candidates = if let Some(composing_text) = composing_text {
            Candidates {
                texts: composing_text
                    .suggestions
                    .iter()
                    .map(|s| s.text.clone())
                    .collect(),
                sub_texts: composing_text
                    .suggestions
                    .iter()
                    .map(|s| s.subtext.clone())
                    .collect(),
                is_prediction: composing_text
                    .suggestions
                    .iter()
                    .map(|s| s.is_prediction)
                    .collect(),
                hiragana: composing_text.hiragana,
                raw_input: composing_text.raw_input,
                corresponding_count: composing_text
                    .suggestions
                    .iter()
                    .map(|s| s.corresponding_count)
                    .collect(),
            }
        } else {
            anyhow::bail!("composing_text is None");
        };

        self.live_candidates(candidates)
    }

    #[tracing::instrument]
    pub fn clear_text(&mut self) -> anyhow::Result<()> {
        self.set_candidates(&Candidates::default())?;
        let request = tonic::Request::new(shared::proto::ClearTextRequest { preview_only: true });
        let _response = self
            .runtime
            .clone()
            .block_on(self.azookey_client.clear_text(request))?;

        Ok(())
    }

    #[tracing::instrument]
    pub fn shrink_text(&mut self, offset: i32) -> anyhow::Result<Candidates> {
        let request = tonic::Request::new(shared::proto::ShrinkTextRequest {
            offset,
            preview_only: true,
        });
        let response = self
            .runtime
            .clone()
            .block_on(self.azookey_client.shrink_text(request))?;
        let composing_text = response.into_inner().composing_text;

        let candidates = if let Some(composing_text) = composing_text {
            Candidates {
                texts: composing_text
                    .suggestions
                    .iter()
                    .map(|s| s.text.clone())
                    .collect(),
                sub_texts: composing_text
                    .suggestions
                    .iter()
                    .map(|s| s.subtext.clone())
                    .collect(),
                is_prediction: composing_text
                    .suggestions
                    .iter()
                    .map(|s| s.is_prediction)
                    .collect(),
                hiragana: composing_text.hiragana,
                raw_input: composing_text.raw_input,
                corresponding_count: composing_text
                    .suggestions
                    .iter()
                    .map(|s| s.corresponding_count)
                    .collect(),
            }
        } else {
            anyhow::bail!("composing_text is None");
        };

        Ok(candidates)
    }

    pub fn set_context(&mut self, context: String) -> anyhow::Result<()> {
        *self
            .context
            .lock()
            .map_err(|_| anyhow::anyhow!("Context mutex poisoned"))? = context;
        Ok(())
    }

    fn live_candidates(&mut self, candidates: Candidates) -> Result<Candidates> {
        *self
            .raw_input
            .lock()
            .map_err(|_| anyhow::anyhow!("Input mutex poisoned"))? = candidates.raw_input.clone();
        if shared::AppConfig::read().conversion.live_conversion && !candidates.is_latin_input() {
            self.convert_text(candidates.hiragana, false)
        } else {
            Ok(candidates)
        }
    }

    pub fn cancel_prediction(&self) -> Result<()> {
        let mut state = self
            .prediction
            .lock()
            .map_err(|_| anyhow::anyhow!("Prediction mutex poisoned"))?;
        if let Some(task) = state.task.take() {
            task.abort();
        }
        state.request_id.clear();
        state.result = None;
        Ok(())
    }

    pub fn convert_text(&mut self, reading: String, prediction: bool) -> Result<Candidates> {
        let cached = {
            let state = self
                .prediction
                .lock()
                .map_err(|_| anyhow::anyhow!("Prediction mutex poisoned"))?;
            if prediction && state.reading == reading {
                state.result.clone()
            } else {
                None
            }
        };
        self.set_candidates(&Candidates::default())?;
        if let Some(result) = cached {
            return result.map_err(anyhow::Error::msg);
        }
        let context = self
            .context
            .lock()
            .map_err(|_| anyhow::anyhow!("Context mutex poisoned"))?
            .clone();
        let raw_input = self
            .raw_input
            .lock()
            .map_err(|_| anyhow::anyhow!("Input mutex poisoned"))?
            .clone();
        let request = shared::proto::ConvertTextRequest {
            reading,
            raw_input,
            context,
            prediction_only: prediction,
        };
        let response = self
            .runtime
            .clone()
            .block_on(self.azookey_client.convert_text(request))?;
        Ok(response
            .into_inner()
            .composing_text
            .ok_or_else(|| anyhow::anyhow!("composing_text is None"))?
            .into())
    }

    pub fn schedule_prediction(&mut self, reading: String) -> Result<()> {
        self.cancel_prediction()?;
        let id = request_id();
        self.runtime
            .clone()
            .block_on(self.window_client.set_candidate(window_candidates(
                &Candidates::default(),
                id.clone(),
                true,
            )))?;
        if reading.is_empty() || !shared::AppConfig::read().conversion.prediction {
            return Ok(());
        }
        let context = self
            .context
            .lock()
            .map_err(|_| anyhow::anyhow!("Context mutex poisoned"))?
            .clone();
        {
            let mut state = self
                .prediction
                .lock()
                .map_err(|_| anyhow::anyhow!("Prediction mutex poisoned"))?;
            state.request_id = id.clone();
            state.reading = reading.clone();
        }
        let state = self.prediction.clone();
        let mut converter = self.azookey_client.clone();
        let mut window = self.window_client.clone();
        let raw_input = self
            .raw_input
            .lock()
            .map_err(|_| anyhow::anyhow!("Input mutex poisoned"))?
            .clone();
        let task = self.runtime.spawn(async move {
            time::sleep(Duration::from_millis(120)).await;
            let result = async {
                let response = converter
                    .convert_text(shared::proto::ConvertTextRequest {
                        reading,
                        raw_input,
                        context,
                        prediction_only: true,
                    })
                    .await?;
                Ok::<Candidates, anyhow::Error>(
                    response
                        .into_inner()
                        .composing_text
                        .ok_or_else(|| anyhow::anyhow!("composing_text is None"))?
                        .into(),
                )
            }
            .await
            .map_err(|error| error.to_string());
            {
                let Ok(mut state) = state.lock() else {
                    tracing::error!("Prediction mutex poisoned");
                    return;
                };
                if state.request_id != id {
                    return;
                }
                state.result = Some(result.clone());
            }
            let candidates = match result {
                Ok(candidates) => candidates,
                Err(error) => Candidates {
                    texts: vec!["予測を取得できませんでした".into()],
                    sub_texts: vec![error],
                    ..Candidates::default()
                },
            };
            if let Err(error) = window
                .set_candidate(window_candidates(&candidates, id, false))
                .await
            {
                tracing::error!("Failed to display prediction: {error}");
            }
        });
        self.prediction
            .lock()
            .map_err(|_| anyhow::anyhow!("Prediction mutex poisoned"))?
            .task = Some(task.abort_handle());
        Ok(())
    }

    pub fn commit_candidate(&mut self, reading: String, text: String) -> anyhow::Result<()> {
        let request = tonic::Request::new(shared::proto::CommitCandidateRequest { reading, text });
        let _response = self
            .runtime
            .clone()
            .block_on(self.azookey_client.commit_candidate(request))?;

        Ok(())
    }
}

// implement methods to interact with candidate window server
impl IPCService {
    #[tracing::instrument]
    pub fn show_window(&mut self) -> anyhow::Result<()> {
        let request = tonic::Request::new(shared::proto::EmptyResponse {});
        self.runtime
            .clone()
            .block_on(self.window_client.show_window(request))?;

        Ok(())
    }

    #[tracing::instrument]
    pub fn hide_window(&mut self) -> anyhow::Result<()> {
        self.set_candidates(&Candidates::default())?;
        let request = tonic::Request::new(shared::proto::EmptyResponse {});
        self.runtime
            .clone()
            .block_on(self.window_client.hide_window(request))?;

        Ok(())
    }

    #[tracing::instrument]
    pub fn set_window_position(
        &mut self,
        top: i32,
        left: i32,
        bottom: i32,
        right: i32,
    ) -> anyhow::Result<()> {
        let request = tonic::Request::new(shared::proto::SetPositionRequest {
            position: Some(shared::proto::WindowPosition {
                top,
                left,
                bottom,
                right,
            }),
        });
        self.runtime
            .clone()
            .block_on(self.window_client.set_window_position(request))?;

        Ok(())
    }

    #[tracing::instrument]
    pub fn set_candidates(&mut self, candidates: &Candidates) -> anyhow::Result<()> {
        self.cancel_prediction()?;
        let request = tonic::Request::new(window_candidates(candidates, request_id(), true));
        self.runtime
            .clone()
            .block_on(self.window_client.set_candidate(request))?;

        Ok(())
    }

    #[tracing::instrument]
    pub fn set_selection(&mut self, index: i32) -> anyhow::Result<()> {
        let request = tonic::Request::new(shared::proto::SetSelectionRequest { index });
        self.runtime
            .clone()
            .block_on(self.window_client.set_selection(request))?;

        Ok(())
    }

    #[tracing::instrument]
    pub fn set_input_mode(&mut self, mode: &str) -> anyhow::Result<()> {
        let request = tonic::Request::new(shared::proto::SetInputModeRequest {
            mode: mode.to_string(),
        });
        self.runtime
            .clone()
            .block_on(self.window_client.set_input_mode(request))?;

        Ok(())
    }
}
