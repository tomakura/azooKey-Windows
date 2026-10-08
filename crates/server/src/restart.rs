use std::{
    io,
    process::{Child, Command},
};

/// Retain a replacement only after it starts, so a failed launch remains retryable.
pub fn spawn_replacement(pending: &mut Option<Child>, mut command: Command) -> io::Result<u32> {
    if pending.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Engine restart is already in progress",
        ));
    }
    let child = command.spawn()?;
    let pid = child.id();
    *pending = Some(child);
    Ok(pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify a failed executable launch remains retryable and a successful launch cannot be duplicated.
    #[test]
    fn spawn_failure_can_be_retried_without_a_pending_restart() {
        let missing = std::env::temp_dir()
            .join(format!("azookey-missing-executable-{}", std::process::id()))
            .join("server.exe");
        let mut pending = None;
        assert!(spawn_replacement(&mut pending, Command::new(missing)).is_err());
        assert!(pending.is_none());

        // The test executable's help path exits without starting the engine or other tests.
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.arg("--help").stdout(std::process::Stdio::null());
        let pid = spawn_replacement(&mut pending, command).unwrap();
        assert_eq!(pending.as_ref().unwrap().id(), pid);
        assert_eq!(
            spawn_replacement(&mut pending, Command::new("unused"))
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(pending.as_ref().unwrap().id(), pid);
        assert!(pending.as_mut().unwrap().wait().unwrap().success());
    }
}
