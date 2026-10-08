mod engine;

use azookey_server::TonicNamedPipeServer;
use engine::Engine;
use shared::proto::azookey_service_server::{AzookeyService, AzookeyServiceServer};
use shared::proto::*;
use std::sync::Mutex;
use tonic::{transport::Server, Request, Response, Status};
use tonic_reflection::server::Builder as ReflectionBuilder;

static ENGINE: Mutex<Option<Engine>> = Mutex::new(None);
static READING: Mutex<()> = Mutex::new(());
static RESTART: tokio::sync::Notify = tokio::sync::Notify::const_new();

fn restart_command() -> Result<std::process::Command, Box<dyn std::error::Error>> {
    let exe = std::env::current_exe()?;
    let directory = exe.parent().ok_or("Server directory is missing")?;
    let backend = match shared::AppConfig::read().zenzai.backend.as_str() {
        "cpu" => "llama_cpu",
        "cuda" => "llama_cuda",
        "vulkan" => "llama_vulkan",
        _ => return Err("Unknown Zenzai backend".into()),
    };
    let backend = directory.join(backend);
    if !backend.join("llama.dll").is_file() {
        return Err(format!("Missing Zenzai backend: {}", backend.display()).into());
    }
    let mut paths = vec![backend];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let mut command = std::process::Command::new(&exe);
    command.env("PATH", std::env::join_paths(paths)?);
    Ok(command)
}

#[allow(
    clippy::result_large_err,
    reason = "Use tonic's RPC error type at the service boundary"
)]
fn with_engine<T>(action: impl FnOnce(&mut Engine) -> Result<T, String>) -> Result<T, Status> {
    let mut guard = ENGINE
        .lock()
        .map_err(|_| Status::internal("Engine mutex poisoned"))?;
    let engine = guard
        .as_mut()
        .ok_or_else(|| Status::unavailable("Engine not initialized"))?;
    action(engine).map_err(Status::internal)
}

#[allow(clippy::result_large_err)]
fn edit_reading(text: &str, operation: i32, count: i32) -> Result<ComposingText, Status> {
    let _guard = READING
        .lock()
        .map_err(|_| Status::internal("Reading mutex poisoned"))?;
    engine::edit_reading(text, operation, count).map_err(Status::internal)
}

#[derive(Debug, Default)]
pub struct MyAzookeyService;

#[tonic::async_trait]
impl AzookeyService for MyAzookeyService {
    async fn convert_text(
        &self,
        request: Request<ConvertTextRequest>,
    ) -> Result<Response<AppendTextResponse>, Status> {
        let request = request.into_inner();
        let composing_text = tokio::task::spawn_blocking(move || {
            with_engine(|engine| {
                engine.convert(
                    request.reading,
                    &request.raw_input,
                    &request.context,
                    request.prediction_only,
                )
            })
        })
        .await
        .map_err(|error| Status::internal(error.to_string()))??;
        Ok(Response::new(AppendTextResponse {
            composing_text: Some(composing_text),
        }))
    }

    async fn engine_status(
        &self,
        _: Request<EngineStatusRequest>,
    ) -> Result<Response<EngineStatusResponse>, Status> {
        Ok(Response::new(EngineStatusResponse {
            process_id: std::process::id(),
        }))
    }

    async fn restart_engine(
        &self,
        _: Request<RestartEngineRequest>,
    ) -> Result<Response<EngineStatusResponse>, Status> {
        restart_command().map_err(|error| Status::failed_precondition(error.to_string()))?;
        RESTART.notify_one();
        Ok(Response::new(EngineStatusResponse {
            process_id: std::process::id(),
        }))
    }

    async fn append_text(
        &self,
        request: Request<AppendTextRequest>,
    ) -> Result<Response<AppendTextResponse>, Status> {
        let request = request.into_inner();
        let composing_text = if request.preview_only {
            edit_reading(&request.text_to_append, 0, 0)?
        } else {
            with_engine(|engine| engine.append(&request.text_to_append))?
        };
        Ok(Response::new(AppendTextResponse {
            composing_text: Some(composing_text),
        }))
    }

    async fn remove_text(
        &self,
        request: Request<RemoveTextRequest>,
    ) -> Result<Response<RemoveTextResponse>, Status> {
        let composing_text = if request.into_inner().preview_only {
            edit_reading("", 1, 0)?
        } else {
            with_engine(|engine| engine.remove())?
        };
        Ok(Response::new(RemoveTextResponse {
            composing_text: Some(composing_text),
        }))
    }

    async fn move_cursor(
        &self,
        request: Request<MoveCursorRequest>,
    ) -> Result<Response<MoveCursorResponse>, Status> {
        let offset = request.into_inner().offset;
        let composing_text = with_engine(|engine| engine.move_cursor(offset))?;
        Ok(Response::new(MoveCursorResponse {
            composing_text: Some(composing_text),
        }))
    }

    async fn clear_text(
        &self,
        request: Request<ClearTextRequest>,
    ) -> Result<Response<ClearTextResponse>, Status> {
        if request.into_inner().preview_only {
            edit_reading("", 3, 0)?;
        } else {
            with_engine(|engine| {
                engine.clear();
                Ok(())
            })?;
        }
        Ok(Response::new(ClearTextResponse {}))
    }

    async fn shrink_text(
        &self,
        request: Request<ShrinkTextRequest>,
    ) -> Result<Response<ShrinkTextResponse>, Status> {
        let request = request.into_inner();
        if request.offset < 0 {
            return Err(Status::invalid_argument("Shrink count cannot be negative"));
        }
        let composing_text = if request.preview_only {
            edit_reading("", 2, request.offset)?
        } else {
            with_engine(|engine| engine.shrink(request.offset))?
        };
        Ok(Response::new(ShrinkTextResponse {
            composing_text: Some(composing_text),
        }))
    }

    async fn set_context(
        &self,
        request: Request<SetContextRequest>,
    ) -> Result<Response<SetContextResponse>, Status> {
        let context = request.into_inner().context;
        with_engine(|engine| engine.set_context(&context))?;
        Ok(Response::new(SetContextResponse {}))
    }

    async fn update_config(
        &self,
        _: Request<UpdateConfigRequest>,
    ) -> Result<Response<UpdateConfigResponse>, Status> {
        let _reading = READING
            .lock()
            .map_err(|_| Status::internal("Reading mutex poisoned"))?;
        with_engine(|engine| engine.reload())?;
        Ok(Response::new(UpdateConfigResponse {}))
    }

    async fn commit_candidate(
        &self,
        request: Request<CommitCandidateRequest>,
    ) -> Result<Response<CommitCandidateResponse>, Status> {
        let request = request.into_inner();
        with_engine(|engine| engine.commit(&request.reading, &request.text))?;
        Ok(Response::new(CommitCandidateResponse {}))
    }

    async fn reset_learning(
        &self,
        _: Request<ResetLearningRequest>,
    ) -> Result<Response<ResetLearningResponse>, Status> {
        with_engine(|engine| {
            engine.reset_learning();
            Ok(())
        })?;
        Ok(Response::new(ResetLearningResponse {}))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("AzookeyServer started");
    let current_exe = std::env::current_exe()?;
    let parent_dir = current_exe
        .parent()
        .ok_or("Failed to resolve server directory")?;
    let engine = Engine::new(parent_dir).map_err(std::io::Error::other)?;
    *ENGINE.lock().map_err(|_| "Engine mutex poisoned")? = Some(engine);
    println!("AzookeyServer listening");
    Server::builder()
        .add_service(AzookeyServiceServer::new(MyAzookeyService))
        .add_service(
            ReflectionBuilder::configure()
                .register_encoded_file_descriptor_set(shared::proto::FILE_DESCRIPTOR_SET)
                .build_v1()?,
        )
        .serve_with_incoming_shutdown(
            TonicNamedPipeServer::new("azookey_server"),
            RESTART.notified(),
        )
        .await?;
    // All requests and named-pipe listeners are closed before the replacement starts.
    use std::os::windows::process::CommandExt;
    let child = restart_command()?.creation_flags(0x08000000).spawn()?;
    println!("AzookeyServer restarted: {}", child.id());
    Ok(())
}
