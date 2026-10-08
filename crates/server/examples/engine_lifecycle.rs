// Exercise the same recovery path used by the settings app, against the staged server.
#[path = "../../../frontend/src-tauri/src/ipc.rs"]
mod ipc;

use shared::{proto::AppendTextRequest, AppConfig};
use std::path::PathBuf;

struct TestEngine(u32);

impl Drop for TestEngine {
    fn drop(&mut self) {
        // Only the PID obtained from this test's isolated pipe is terminated.
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/PID", &self.0.to_string()])
            .status();
    }
}

fn main() -> anyhow::Result<()> {
    let directory = PathBuf::from(std::env::var_os("AZOOKEY_TEST_RESOURCES").unwrap());
    let appdata = std::env::temp_dir().join(format!("azookey-lifecycle-{}", std::process::id()));
    std::env::set_var("APPDATA", &appdata);
    std::env::set_var(
        "AZOOKEY_INSTANCE",
        format!("_lifecycle_{}", std::process::id()),
    );
    let mut config = AppConfig::new();
    config.zenzai.enable = true;
    config.zenzai.personalization = true;
    config.zenzai.personalization_path.clear();
    config.zenzai.model_path = directory.join("zenz.gguf").to_string_lossy().into_owned();
    config.try_write()?;
    let mut service = ipc::restart_engine(None, &directory)?;
    let mut engine = TestEngine(service.process_id()?);
    service.update_config()?;
    service.reset_learning()?;
    for _ in 0..3 {
        let previous = engine.0;
        service = ipc::restart_engine(Some(service), &directory)?;
        engine.0 = service.process_id()?;
        assert_ne!(engine.0, previous);
        assert!(!service.test_conversion()?.is_empty());
    }
    // A rejected backend must leave the current engine running.
    config.zenzai.backend = "missing".into();
    config.try_write()?;
    assert!(service.request_restart().is_err());
    assert_eq!(service.process_id()?, engine.0);
    config.zenzai.backend = "cpu".into();
    config.try_write()?;
    // Recover with the stale connection that an already-open settings window retains.
    drop(engine);
    assert!(service.process_id().is_err());
    service = ipc::restart_engine(Some(service), &directory)?;
    let engine = TestEngine(service.process_id()?);
    assert!(!service.test_conversion()?.is_empty());
    println!("Engine lifecycle: blank personalization, 3 restarts, rejected backend, offline recovery passed");
    drop(service);
    drop(engine);
    std::fs::remove_dir_all(appdata)?;
    Ok(())
}

impl ipc::IPCService {
    fn test_conversion(&mut self) -> anyhow::Result<String> {
        // The transport stays private; query via a separate real named-pipe client.
        let runtime = tokio::runtime::Runtime::new()?;
        runtime.block_on(async {
            use hyper_util::rt::TokioIo;
            use shared::proto::azookey_service_client::AzookeyServiceClient;
            use tokio::net::windows::named_pipe::ClientOptions;
            use tonic::transport::Endpoint;
            use tower::service_fn;
            let channel = Endpoint::from_static("http://localhost")
                .connect_timeout(std::time::Duration::from_secs(2))
                .timeout(std::time::Duration::from_secs(10))
                .connect_with_connector(service_fn(|_| async {
                    ClientOptions::new()
                        .open(shared::pipe_path("azookey_server"))
                        .map(TokioIo::new)
                }))
                .await?;
            let response = AzookeyServiceClient::new(channel)
                .append_text(AppendTextRequest {
                    text_to_append: "kanji".into(),
                    preview_only: false,
                })
                .await?;
            let composed = response.into_inner().composing_text.unwrap();
            assert!(composed
                .suggestions
                .iter()
                .any(|candidate| candidate.text == "漢字"));
            Ok(composed.hiragana)
        })
    }
}
