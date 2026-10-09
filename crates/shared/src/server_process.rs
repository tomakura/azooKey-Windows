use std::{path::Path, process::Command};

/// Keep DLL selection and the working directory identical for startup and recovery.
pub fn server_command(directory: &Path, backend: &str) -> std::io::Result<Command> {
    let backend_directory = match backend {
        "cpu" => "llama_cpu",
        "cuda" => "llama_cuda",
        "vulkan" => "llama_vulkan",
        _ => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Unknown Zenzai backend: {backend}"),
            ));
        }
    };
    let exe = directory.join("azookey-server.exe");
    let backend_path = directory.join(backend_directory);
    for file in [&exe, &backend_path.join("llama.dll")] {
        if !file.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Required engine file is missing: {}", file.display()),
            ));
        }
    }
    let mut paths = vec![backend_path];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let mut command = Command::new(exe);
    command.current_dir(directory).env(
        "PATH",
        std::env::join_paths(paths)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidInput, error))?,
    );
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify backend validation and child-only DLL paths without mutating the parent environment.
    #[test]
    fn recovery_selects_backend_without_changing_parent_environment() {
        let directory = std::env::temp_dir().join(format!("azookey-spawn-{}", std::process::id()));
        std::fs::create_dir_all(directory.join("llama_vulkan")).unwrap();
        std::fs::write(directory.join("azookey-server.exe"), []).unwrap();
        std::fs::write(directory.join("llama_vulkan/llama.dll"), []).unwrap();
        let previous_path = std::env::var_os("PATH");
        let command = server_command(&directory, "vulkan").unwrap();
        assert_eq!(command.get_current_dir(), Some(directory.as_path()));
        let path = command
            .get_envs()
            .find(|(key, _)| *key == "PATH")
            .unwrap()
            .1
            .unwrap();
        assert_eq!(
            std::env::split_paths(path).next().unwrap(),
            directory.join("llama_vulkan")
        );
        assert_eq!(std::env::var_os("PATH"), previous_path);
        assert!(server_command(&directory, "cuda").is_err());
        assert!(server_command(&directory, "unknown").is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
