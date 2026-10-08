use anyhow::Result;
use hyper_util::rt::TokioIo;
use shared::proto::azookey_service_client::AzookeyServiceClient;
use std::{path::Path, sync::Arc, time::Duration};
use tokio::{net::windows::named_pipe::ClientOptions, time};
use tonic::transport::Endpoint;
use tower::service_fn;
use windows::Win32::Foundation::ERROR_PIPE_BUSY;

pub fn is_connection_error(error: &anyhow::Error) -> bool {
    error.downcast_ref::<tonic::transport::Error>().is_some()
        || error.downcast_ref::<tonic::Status>().is_some_and(|status| {
            matches!(
                status.code(),
                tonic::Code::Unavailable | tonic::Code::DeadlineExceeded
            )
        })
}

pub fn restart_engine(service: Option<IPCService>, directory: &Path) -> Result<IPCService> {
    let mut service = service.or_else(|| IPCService::new().ok());
    let previous = match service.as_mut().map(|service| service.process_id()) {
        Some(Ok(pid)) => Some(pid),
        Some(Err(error)) if !is_connection_error(&error) => return Err(error),
        _ => None,
    };
    let mut child = None;
    if previous.is_some() {
        if let Err(error) = service.as_mut().unwrap().request_restart() {
            if !is_connection_error(&error) {
                return Err(error);
            }
        }
    } else {
        use std::os::windows::process::CommandExt;
        child = Some(
            shared::server_process::server_command(
                directory,
                &shared::AppConfig::read().zenzai.backend,
            )?
            .creation_flags(0x08000000)
            .spawn()?,
        );
    }
    // Discard the old HTTP/2 channel; reconnect to the replacement explicitly.
    drop(service);
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let mut last_error = "新しい変換エンジンの応答を待っています".to_string();
    while std::time::Instant::now() < deadline {
        if let Some(child) = child.as_mut() {
            if let Some(status) = child.try_wait()? {
                anyhow::bail!("変換エンジンが起動直後に終了しました: {status}。バックエンドとドライバーを確認してください");
            }
        }
        match IPCService::new() {
            Ok(mut service) => match service.process_id() {
                Ok(pid) if Some(pid) != previous => return Ok(service),
                Ok(_) => {}
                Err(error) => last_error = error.to_string(),
            },
            Err(error) => last_error = error.to_string(),
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    anyhow::bail!("変換エンジンの再起動を確認できませんでした: {last_error}")
}

// connect to kkc server
#[derive(Debug, Clone)]
pub struct IPCService {
    // kkc server client
    azookey_client: AzookeyServiceClient<tonic::transport::channel::Channel>,
    runtime: Arc<tokio::runtime::Runtime>,
}

impl IPCService {
    pub fn new() -> Result<Self> {
        let runtime = tokio::runtime::Runtime::new()?;

        let server_channel = runtime.block_on(
            Endpoint::try_from("http://[::]:50051")?
                .connect_timeout(Duration::from_secs(2))
                .timeout(Duration::from_secs(5))
                .connect_with_connector(service_fn(|_| async {
                    let client = loop {
                        match ClientOptions::new().open(shared::pipe_path("azookey_server")) {
                            Ok(client) => break client,
                            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY.0 as i32) => (),
                            Err(e) => return Err(e),
                        }

                        time::sleep(Duration::from_millis(50)).await;
                    };

                    Ok::<_, std::io::Error>(TokioIo::new(client))
                })),
        )?;

        let azookey_client = AzookeyServiceClient::new(server_channel);

        Ok(Self {
            azookey_client,
            runtime: Arc::new(runtime),
        })
    }
}

// implement methods to interact with kkc server
impl IPCService {
    pub fn process_id(&mut self) -> anyhow::Result<u32> {
        let response = self.runtime.clone().block_on(
            self.azookey_client
                .engine_status(shared::proto::EngineStatusRequest {}),
        )?;
        Ok(response.into_inner().process_id)
    }

    pub fn request_restart(&mut self) -> anyhow::Result<u32> {
        let response = self.runtime.clone().block_on(
            self.azookey_client
                .restart_engine(shared::proto::RestartEngineRequest {}),
        )?;
        Ok(response.into_inner().process_id)
    }

    pub fn update_config(&mut self) -> anyhow::Result<()> {
        let request = tonic::Request::new(shared::proto::UpdateConfigRequest {});
        self.runtime
            .clone()
            .block_on(self.azookey_client.update_config(request))?;

        Ok(())
    }

    pub fn reset_learning(&mut self) -> anyhow::Result<()> {
        let request = tonic::Request::new(shared::proto::ResetLearningRequest {});
        self.runtime
            .clone()
            .block_on(self.azookey_client.reset_learning(request))?;

        Ok(())
    }
}
