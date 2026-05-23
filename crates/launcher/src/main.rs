use shared::AppConfig;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::{env, thread};

fn main() -> anyhow::Result<()> {
    let config = AppConfig::new();

    let exe_path = env::current_exe()?
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Failed to resolve launcher directory"))?
        .to_path_buf();
    let backend_dir = match config.zenzai.backend.as_str() {
        "cpu" => "llama_cpu",
        "cuda" => "llama_cuda",
        "vulkan" => "llama_vulkan",
        _ => "llama_cpu",
    };

    let backend_path = exe_path.join(backend_dir);
    let backend_path_str = backend_path.to_string_lossy();

    let mut new_path = env::var("PATH").unwrap_or_else(|_| String::new());
    new_path = format!("{};{}", backend_path_str, new_path);
    env::set_var("PATH", &new_path);

    let mut server_process = start_process(&exe_path, "azookey-server.exe", "[server]")?;
    let ui_process = match start_process(&exe_path, "ui.exe", "[ui]") {
        Ok(process) => process,
        Err(error) => {
            let _ = server_process.kill();
            let _ = server_process.wait();
            return Err(error);
        }
    };

    let mut server = server_process;
    let mut ui = ui_process;
    let server_handle = thread::spawn(move || server.wait());
    let ui_handle = thread::spawn(move || ui.wait());

    let _ = server_handle.join();
    let _ = ui_handle.join();

    Ok(())
}

fn start_process(exe_dir: &Path, exe: &str, prefix: &str) -> anyhow::Result<Child> {
    let exe_path = exe_dir.join(exe);
    let mut child = Command::new(&exe_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| anyhow::anyhow!("Failed to start {}: {}", exe_path.display(), error))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("Failed to capture stdout for {}", exe))?;
    let stdout_reader = BufReader::new(stdout);
    let prefix_stdout = prefix.to_string();
    thread::spawn(move || {
        for line in stdout_reader.lines().map_while(Result::ok) {
            println!("{}: {}", prefix_stdout, line);
        }
    });

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("Failed to capture stderr for {}", exe))?;
    let stderr_reader = BufReader::new(stderr);
    let prefix_stderr = prefix.to_string();
    thread::spawn(move || {
        for line in stderr_reader.lines().map_while(Result::ok) {
            eprintln!("{}: {}", prefix_stderr, line);
        }
    });

    Ok(child)
}
