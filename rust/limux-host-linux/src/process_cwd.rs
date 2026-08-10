use std::path::PathBuf;

pub fn surface_cwd(surface_id: &str) -> Option<String> {
    let expected_env = format!("LIMUX_SURFACE_ID={surface_id}");
    let shells = [
        "bash", "zsh", "fish", "nu", "elvish", "sh", "dash", "ksh", "tcsh", "csh",
    ];
    let mut best: Option<(usize, u32, PathBuf)> = None;

    for entry in std::fs::read_dir("/proc").ok()?.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        let process_dir = entry.path();
        let Ok(comm) = std::fs::read_to_string(process_dir.join("comm")) else {
            continue;
        };
        if !shells.contains(&comm.trim()) {
            continue;
        }
        let Ok(environ) = std::fs::read(process_dir.join("environ")) else {
            continue;
        };
        if !environ
            .split(|byte| *byte == 0)
            .any(|entry| entry == expected_env.as_bytes())
        {
            continue;
        }
        let Ok(cwd) = std::fs::read_link(process_dir.join("cwd")) else {
            continue;
        };

        let mut depth = 0;
        let mut ancestor = pid;
        while depth < 64 {
            let Ok(status) = std::fs::read_to_string(format!("/proc/{ancestor}/status")) else {
                break;
            };
            let Some(parent) = status
                .lines()
                .find_map(|line| line.strip_prefix("PPid:\t"))
                .and_then(|value| value.parse::<u32>().ok())
            else {
                break;
            };
            if parent == 0 || parent == ancestor {
                break;
            }
            ancestor = parent;
            depth += 1;
        }

        if best
            .as_ref()
            .is_none_or(|(best_depth, best_pid, _)| (depth, pid) > (*best_depth, *best_pid))
        {
            best = Some((depth, pid, cwd));
        }
    }

    best.and_then(|(_, _, cwd)| cwd.into_os_string().into_string().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::{Command, Stdio};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn resolves_surface_shell_working_directory() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let surface_id = format!("cwd-test-{}-{nonce}", std::process::id());
        let root = std::env::temp_dir().join(&surface_id);
        let initial_cwd = root.join("initial");
        let changed_cwd = root.join("changed");
        std::fs::create_dir_all(&initial_cwd).unwrap();
        std::fs::create_dir(&changed_cwd).unwrap();
        let mut shell = Command::new("/bin/sh")
            .current_dir(&initial_cwd)
            .env("LIMUX_SURFACE_ID", &surface_id)
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        let mut resolved = None;
        for _ in 0..50 {
            resolved = surface_cwd(&surface_id);
            if resolved.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(resolved.as_deref(), initial_cwd.to_str());

        writeln!(
            shell.stdin.as_mut().unwrap(),
            "cd {}",
            changed_cwd.display()
        )
        .unwrap();
        for _ in 0..50 {
            resolved = surface_cwd(&surface_id);
            if resolved.as_deref() == changed_cwd.to_str() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(resolved.as_deref(), changed_cwd.to_str());

        shell.kill().unwrap();
        shell.wait().unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}
