use anyhow::Result;
use hyper_util::rt::TokioIo;
use shared::proto::azookey_service_client::AzookeyServiceClient;
use std::{path::Path, sync::Arc, time::Duration};
use tokio::{net::windows::named_pipe::ClientOptions, time};
use tonic::transport::Endpoint;
use tower::service_fn;
use windows::Win32::Foundation::ERROR_PIPE_BUSY;

/// Identify transport failures that permit offline settings and a fresh engine connection.
pub fn is_connection_error(error: &anyhow::Error) -> bool {
    error.downcast_ref::<tonic::transport::Error>().is_some()
        // Tonic 0.12 reports its local Endpoint timeout as Cancelled with this source.
        || error.chain().any(|cause| cause.is::<tonic::TimeoutExpired>())
        || error.downcast_ref::<tonic::Status>().is_some_and(|status| {
            matches!(
                status.code(),
                tonic::Code::Unavailable | tonic::Code::DeadlineExceeded
            )
        })
}

/// Report whether a connection failure proves that the named pipe is absent.
fn is_engine_absent(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause.downcast_ref::<std::io::Error>().is_some_and(|error| {
            error.raw_os_error() == Some(windows::Win32::Foundation::ERROR_FILE_NOT_FOUND.0 as i32)
        })
    })
}

/// Check the cached engine PID without relying on an RPC response; ambiguous failures are errors.
fn engine_process_is_alive(pid: u32) -> Result<bool> {
    use windows::Win32::{
        Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, WAIT_OBJECT_0, WAIT_TIMEOUT},
        System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
    };
    let handle = match unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) } {
        Ok(handle) => handle,
        Err(error) if error.code() == ERROR_INVALID_PARAMETER.to_hresult() => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let state = unsafe { WaitForSingleObject(handle, 0) };
    unsafe { CloseHandle(handle)? };
    match state {
        WAIT_TIMEOUT => Ok(true),
        WAIT_OBJECT_0 => Ok(false),
        _ => Err(windows::core::Error::from_win32().into()),
    }
}

/// Interpret the server's explicit duplicate-restart response as an already accepted launch.
fn is_restart_in_progress(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<tonic::Status>()
        .is_some_and(|status| status.code() == tonic::Code::AlreadyExists)
}

/// Restart or recover an engine while retaining its known PID across transient connection errors.
pub fn restart_engine(service: Option<IPCService>, directory: &Path) -> Result<IPCService> {
    let mut service = match service {
        Some(service) => Some(service),
        None => match IPCService::new() {
            Ok(service) => Some(service),
            Err(error) if is_engine_absent(&error) => None,
            Err(error) => return Err(error),
        },
    };
    let mut previous = service.as_ref().map(|service| service.last_process_id);
    let mut accepted = false;
    if let Some(service) = service.as_mut() {
        match service.request_restart() {
            Ok(pid) => {
                previous = Some(pid);
                accepted = true;
            }
            Err(error) if is_restart_in_progress(&error) => accepted = true,
            Err(error) if is_connection_error(&error) => {}
            Err(error) => return Err(error),
        }
    }
    drop(service);
    let started = std::time::Instant::now();
    let deadline = started + Duration::from_secs(20);
    let mut child: Option<std::process::Child> = None;
    // Keep all errors inside this closure so a child started here is reaped on failure.
    let result = (|| -> Result<IPCService> {
        let mut last_error = "新しい変換エンジンの応答を待っています".to_string();
        while std::time::Instant::now() < deadline {
            if let Some(child) = child.as_mut() {
                if let Some(status) = child.try_wait()? {
                    anyhow::bail!("変換エンジンが起動直後に終了しました: {status}。バックエンドとドライバーを確認してください");
                }
            }
            match IPCService::new() {
                Ok(mut service) => {
                    let pid = service.last_process_id;
                    if previous.is_none() && child.is_none() {
                        // An engine appeared before we launched anything; restart it rather than
                        // treating its first successful connection as a completed restart.
                        previous = Some(pid);
                    }
                    if Some(pid) != previous {
                        // Offline recovery must connect to the exact process we started.
                        if child.as_ref().is_none_or(|child| child.id() == pid) {
                            return Ok(service);
                        }
                        last_error = "起動した変換エンジンと接続先が一致しません".into();
                    } else if !accepted {
                        match service.request_restart() {
                            Ok(pid) => {
                                previous = Some(pid);
                                accepted = true;
                            }
                            Err(error) if is_restart_in_progress(&error) => accepted = true,
                            Err(error) if is_connection_error(&error) => {
                                last_error = error.to_string()
                            }
                            Err(error) => return Err(error),
                        }
                    }
                }
                Err(error) => {
                    let absent = is_engine_absent(&error);
                    last_error = error.to_string();
                    // A lost reply may still have launched a successor. Allow it time to appear.
                    let can_recover =
                        previous.is_none() || started.elapsed() >= Duration::from_secs(3);
                    if absent && !accepted && child.is_none() && can_recover {
                        let alive = previous
                            .map(engine_process_is_alive)
                            .transpose()?
                            .unwrap_or(false);
                        if !alive {
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
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        anyhow::bail!("変換エンジンの再起動を確認できませんでした: {last_error}")
    })();
    if result.is_err() {
        if let Some(mut child) = child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    result
}

// connect to kkc server
#[derive(Debug, Clone)]
pub struct IPCService {
    // kkc server client
    azookey_client: AzookeyServiceClient<tonic::transport::channel::Channel>,
    runtime: Arc<tokio::runtime::Runtime>,
    last_process_id: u32,
}

impl IPCService {
    /// Connect to the engine pipe with a two-second connection limit and five-second RPC limit.
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

        let mut service = Self {
            azookey_client,
            runtime: Arc::new(runtime),
            last_process_id: 0,
        };
        service.process_id()?;
        Ok(service)
    }
}

// implement methods to interact with kkc server
impl IPCService {
    /// Query the connected engine's PID to detect a completed restart.
    pub fn process_id(&mut self) -> anyhow::Result<u32> {
        let response = self.runtime.clone().block_on(
            self.azookey_client
                .engine_status(shared::proto::EngineStatusRequest {}),
        )?;
        let pid = response.into_inner().process_id;
        self.last_process_id = pid;
        Ok(pid)
    }

    /// Request replacement and return the old PID once the server has accepted the launch.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A local RPC timeout is retryable, while an explicit server cancellation is preserved.
    #[test]
    fn local_rpc_timeout_is_a_connection_error() {
        let timeout = tonic::Status::from_error(Box::new(tonic::TimeoutExpired(())));
        assert_eq!(timeout.code(), tonic::Code::Cancelled);
        assert!(is_connection_error(&timeout.into()));
        assert!(!is_connection_error(
            &tonic::Status::cancelled("Request cancelled by the server").into()
        ));
    }

    /// Distinguish a live process from an exited process even while its handle is retained.
    #[test]
    fn process_probe_distinguishes_live_and_exited_processes() {
        assert!(engine_process_is_alive(std::process::id()).unwrap());
        let mut child = std::process::Command::new("cmd")
            .args(["/C", "exit", "0"])
            .spawn()
            .unwrap();
        assert!(child.wait().unwrap().success());
        assert!(!engine_process_is_alive(child.id()).unwrap());
    }

    struct TestEngine(u32);

    impl Drop for TestEngine {
        /// Stop only the engine obtained from this test's isolated pipe.
        fn drop(&mut self) {
            let _ = std::process::Command::new("taskkill")
                .args(["/F", "/PID", &self.0.to_string()])
                .status();
        }
    }

    /// An expired RPC on a live engine must neither report the old PID as a restart nor orphan a child.
    #[test]
    #[ignore = "requires the staged Windows engine and CPU model"]
    fn restart_recovers_from_a_transient_rpc_failure() {
        let directory =
            std::path::PathBuf::from(std::env::var_os("AZOOKEY_TEST_RESOURCES").unwrap());
        let appdata =
            std::env::temp_dir().join(format!("azookey-rpc-timeout-{}", std::process::id()));
        std::env::set_var("APPDATA", &appdata);
        std::env::set_var(
            "AZOOKEY_INSTANCE",
            format!("_rpc_timeout_{}", std::process::id()),
        );
        let mut config = shared::AppConfig::new();
        config.zenzai.backend = "cpu".into();
        config.try_write().unwrap();
        let mut service = restart_engine(None, &directory).unwrap();
        let previous = service.process_id().unwrap();
        let mut engine = TestEngine(previous);
        let channel = service
            .runtime
            .block_on(
                Endpoint::from_static("http://localhost")
                    .connect_timeout(Duration::from_secs(2))
                    .timeout(Duration::ZERO)
                    .connect_with_connector(service_fn(|_| async {
                        let client = loop {
                            match ClientOptions::new().open(shared::pipe_path("azookey_server")) {
                                Ok(client) => break client,
                                Err(error)
                                    if error.raw_os_error() == Some(ERROR_PIPE_BUSY.0 as i32) => {}
                                Err(error) => return Err(error),
                            }
                            time::sleep(Duration::from_millis(50)).await;
                        };
                        Ok::<_, std::io::Error>(TokioIo::new(client))
                    })),
            )
            .unwrap();
        service.azookey_client = AzookeyServiceClient::new(channel);
        assert!(service.process_id().is_err());
        assert_eq!(service.last_process_id, previous);
        let mut service = restart_engine(Some(service), &directory).unwrap();
        engine.0 = service.process_id().unwrap();
        assert_ne!(engine.0, previous);
        assert!(!engine_process_is_alive(previous).unwrap());
        drop(service);
        drop(engine);
        std::fs::remove_dir_all(appdata).unwrap();
    }
}
