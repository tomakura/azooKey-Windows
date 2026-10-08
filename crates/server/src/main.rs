mod engine;
mod restart;

use azookey_server::TonicNamedPipeServer;
use engine::Engine;
use shared::proto::azookey_service_server::{AzookeyService, AzookeyServiceServer};
use shared::proto::*;
use std::sync::Mutex;
use tonic::{transport::Server, Request, Response, Status};
use tonic_reflection::server::Builder as ReflectionBuilder;

static ENGINE: Mutex<Option<Engine>> = Mutex::new(None);
static READING: Mutex<()> = Mutex::new(());
static RESTART_CHILD: Mutex<Option<std::process::Child>> = Mutex::new(None);
static RESTART: tokio::sync::Notify = tokio::sync::Notify::const_new();
static SHUTDOWN: tokio::sync::Notify = tokio::sync::Notify::const_new();

fn restart_command() -> Result<std::process::Command, Box<dyn std::error::Error>> {
    let exe = std::env::current_exe()?;
    let directory = exe.parent().ok_or("Server directory is missing")?;
    Ok(shared::server_process::server_command(
        directory,
        &shared::AppConfig::read().zenzai.backend,
    )?)
}

fn wait_for_previous_process() -> Result<(), Box<dyn std::error::Error>> {
    use windows::Win32::{
        Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, WAIT_OBJECT_0},
        System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
    };
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("--wait-for-process") {
        return Ok(());
    }
    let pid = args
        .next()
        .ok_or("Previous process ID is missing")?
        .parse()?;
    let handle = match unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) } {
        Ok(handle) => handle,
        Err(error) if error.code() == ERROR_INVALID_PARAMETER.to_hresult() => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let result = unsafe { WaitForSingleObject(handle, 10_000) };
    unsafe { CloseHandle(handle)? };
    if result != WAIT_OBJECT_0 {
        return Err("Previous engine did not exit within 10 seconds".into());
    }
    Ok(())
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
        use std::os::windows::process::CommandExt;
        let mut pending = RESTART_CHILD
            .lock()
            .map_err(|_| Status::internal("Restart mutex poisoned"))?;
        let mut command =
            restart_command().map_err(|error| Status::failed_precondition(error.to_string()))?;
        command
            .args(["--wait-for-process", &std::process::id().to_string()])
            .creation_flags(0x08000000);
        let pid = restart::spawn_replacement(&mut pending, command).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                Status::failed_precondition(error.to_string())
            } else {
                Status::internal(format!("Failed to start replacement engine: {error}"))
            }
        })?;
        println!("AzookeyServer replacement started: {pid}");
        // Only a successful spawn may shut down the old server. The child waits for our exit.
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
    wait_for_previous_process()?;
    println!("AzookeyServer started");
    let current_exe = std::env::current_exe()?;
    let parent_dir = current_exe
        .parent()
        .ok_or("Failed to resolve server directory")?;
    let engine = Engine::new(parent_dir).map_err(std::io::Error::other)?;
    *ENGINE.lock().map_err(|_| "Engine mutex poisoned")? = Some(engine);
    println!("AzookeyServer listening");
    let mut server = tokio::spawn(
        Server::builder()
            .add_service(AzookeyServiceServer::new(MyAzookeyService))
            .add_service(
                ReflectionBuilder::configure()
                    .register_encoded_file_descriptor_set(shared::proto::FILE_DESCRIPTOR_SET)
                    .build_v1()?,
            )
            .serve_with_incoming_shutdown(
                TonicNamedPipeServer::new("azookey_server"),
                SHUTDOWN.notified(),
            ),
    );
    tokio::select! {
        result = &mut server => {
            result??;
            return Ok(());
        }
        _ = RESTART.notified() => {}
    }
    // Let the restart RPC finish, but do not let persistent IPC clients block shutdown.
    SHUTDOWN.notify_one();
    if let Ok(result) = tokio::time::timeout(std::time::Duration::from_secs(3), &mut server).await {
        result??;
    }
    // Close every old named-pipe handle before the child initializes its listener.
    std::process::exit(0)
}
