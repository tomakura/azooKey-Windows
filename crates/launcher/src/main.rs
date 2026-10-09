use shared::AppConfig;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::{env, thread};

/// Launch the engine and candidate UI from the installation directory and supervise them.
fn main() -> anyhow::Result<()> {
    let config = AppConfig::new();

    let exe_path = env::current_exe()?
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Failed to resolve launcher directory"))?
        .to_path_buf();
    let mut server_command =
        shared::server_process::server_command(&exe_path, &config.zenzai.backend)?;
    let mut server_process = start_command(&mut server_command, "[server]")?;
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

/// Start an installed executable with the installation directory as its working directory.
fn start_process(exe_dir: &Path, exe: &str, prefix: &str) -> anyhow::Result<Child> {
    let mut command = Command::new(exe_dir.join(exe));
    command.current_dir(exe_dir);
    start_command(&mut command, prefix)
}

/// Spawn a prepared command and forward its stdout and stderr with a component prefix.
fn start_command(command: &mut Command, prefix: &str) -> anyhow::Result<Child> {
    let exe = command.get_program().to_string_lossy().into_owned();
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| anyhow::anyhow!("Failed to start {}: {}", exe, error))?;

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
