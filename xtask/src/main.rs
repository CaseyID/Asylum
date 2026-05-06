use std::env;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, SystemTime};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const DEFAULT_DEV_BIND: &str = "127.0.0.1:7788";
const DEV_HOME_DIR: &str = ".asylum-dev";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let repo = repo_root();
    env::set_current_dir(&repo)?;
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "dev".to_string());
    let passthrough: Vec<String> = args.collect();

    match command.as_str() {
        "dev" => run_dev_stack(&repo),
        "dev-daemon" => run_dev_daemon(&repo),
        "dev-cockpit" => run_dev_cockpit(&repo),
        "build-stack" => build_stack(&repo),
        "test-stack" => test_stack(&repo),
        "run-stack" => run_stack(&repo),
        "start-stack" => run_installed_asylum("start", &passthrough),
        "stop-stack" => run_installed_asylum("stop", &passthrough),
        "restart-stack" => run_installed_asylum("restart", &passthrough),
        "status-stack" => run_installed_asylum("status", &passthrough),
        "doctor-stack" => run_installed_asylum("doctor", &passthrough),
        "logs-stack" => run_installed_asylum("logs", &passthrough),
        "help" | "--help" | "-h" => {
            usage();
            Ok(())
        }
        other => {
            usage();
            Err(format!("unknown xtask command: {other}").into())
        }
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask should live directly under the workspace root")
        .to_path_buf()
}

fn usage() {
    println!(
        "\
Asylum Cargo dev workflows:
  dev          Run daemon + Cockpit hot-reload dev servers
  dev-daemon   Run daemon only, rebuilding/restarting on Rust changes
  dev-cockpit  Run Cockpit Vite dev server only
  build-stack  Build Cockpit assets and the Rust workspace
  test-stack   Run Rust and Cockpit tests
  run-stack    Build Cockpit assets, then run the daemon against them
  start-stack  Start the installed Asylum service using the installed asylum binary
  stop-stack   Stop the installed Asylum service using the installed asylum binary
  restart-stack Restart the installed Asylum service using the installed asylum binary
  status-stack Show installed Asylum status using the installed asylum binary
  doctor-stack Run installed Asylum doctor using the installed asylum binary
  logs-stack   Show installed Asylum logs using the installed asylum binary
  help         Show this help

Cargo aliases:
  cargo dev
  cargo dev-daemon
  cargo dev-cockpit
  cargo build-stack
  cargo test-stack
  cargo run-stack
  cargo start-stack
  cargo stop-stack
  cargo restart-stack
  cargo status-stack
  cargo doctor-stack
  cargo logs-stack"
    );
}

fn daemon_bind() -> String {
    env::var("ASYLUM_DEV_BIND").unwrap_or_else(|_| DEFAULT_DEV_BIND.to_string())
}

fn base_url(bind: &str) -> String {
    env::var("ASYLUM_BASE_URL").unwrap_or_else(|_| format!("http://{bind}"))
}

fn dev_home(repo: &Path) -> PathBuf {
    env::var_os("ASYLUM_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo.join(DEV_HOME_DIR))
}

fn env_value_or(default_var: &str, default: impl Into<OsString>) -> OsString {
    env::var_os(default_var).unwrap_or_else(|| default.into())
}

fn apply_dev_env(command: &mut Command, repo: &Path, bind: &str) {
    let home = dev_home(repo);
    command.env("ASYLUM_HOME", &home);
    command.env(
        "ASYLUM_CONFIG",
        env_value_or("ASYLUM_CONFIG", home.join("config.toml").into_os_string()),
    );
    command.env(
        "ASYLUM_DATABASE",
        env_value_or(
            "ASYLUM_DATABASE",
            home.join("asylum.sqlite3").into_os_string(),
        ),
    );
    command.env(
        "ASYLUM_SOCKET_PATH",
        env_value_or(
            "ASYLUM_SOCKET_PATH",
            home.join("run").join("asylum.sock").into_os_string(),
        ),
    );
    command.env("ASYLUM_BASE_URL", base_url(bind));
}

fn cockpit_port() -> String {
    env::var("ASYLUM_COCKPIT_DEV_PORT").unwrap_or_else(|_| "5173".to_string())
}

fn run_status(command: &mut Command) -> Result<()> {
    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("command exited with {status}: {command:?}").into())
    }
}

fn ensure_cockpit_deps(repo: &Path) -> Result<()> {
    if !repo.join("cockpit/node_modules").is_dir() {
        run_status(Command::new("npm").args(["--prefix", "cockpit", "ci"]))?;
    }
    Ok(())
}

fn run_daemon_once() -> Result<()> {
    let bind = daemon_bind();
    let repo = repo_root();
    let mut command = Command::new("cargo");
    command
        .args(["run", "-p", "asylum", "--", "daemon", "run", "--bind"])
        .arg(&bind);
    apply_dev_env(&mut command, &repo, &bind);
    run_status(&mut command)
}

fn run_built_daemon(repo: &Path) -> Result<Child> {
    let bind = daemon_bind();
    let mut command = Command::new(debug_asylum_binary(repo)?);
    command.args(["daemon", "run", "--bind"]).arg(&bind);
    apply_dev_env(&mut command, repo, &bind);
    Ok(command.spawn()?)
}

fn run_dev_daemon(repo: &Path) -> Result<()> {
    if env::var("ASYLUM_DEV_DAEMON_WATCH").as_deref() == Ok("0") {
        return run_daemon_once();
    }

    let interval = env::var("ASYLUM_DEV_WATCH_INTERVAL_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(1);
    let mut last_seen = latest_rust_source_mtime(repo)?;
    let mut daemon = start_daemon(repo)?;

    loop {
        if let Some(status) = daemon.try_wait()? {
            return if status.success() {
                Ok(())
            } else {
                Err(format!("daemon exited with {status}").into())
            };
        }

        thread::sleep(Duration::from_secs(interval));
        let current = latest_rust_source_mtime(repo)?;
        if current > last_seen {
            last_seen = current;
            eprintln!("Rust source changed; rebuilding daemon...");
            stop_child(&mut daemon)?;
            daemon = start_daemon(repo)?;
        }
    }
}

fn start_daemon(repo: &Path) -> Result<Child> {
    run_status(Command::new("cargo").args(["build", "-p", "asylum"]))?;
    run_built_daemon(repo)
}

fn debug_asylum_binary(repo: &Path) -> Result<PathBuf> {
    let mut path = cargo_target_dir(repo)?;
    path.push("debug");
    path.push(format!("asylum{}", env::consts::EXE_SUFFIX));
    Ok(path)
}

fn cargo_target_dir(repo: &Path) -> Result<PathBuf> {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(repo)
        .output()?;
    if !output.status.success() {
        return Err(format!("cargo metadata exited with {}", output.status).into());
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let target = metadata
        .get("target_directory")
        .and_then(|value| value.as_str())
        .ok_or("cargo metadata did not include target_directory")?;
    Ok(PathBuf::from(target))
}

fn stop_child(child: &mut Child) -> Result<()> {
    if child.try_wait()?.is_none() {
        let _ = child.kill();
    }
    let _ = child.wait();
    Ok(())
}

fn latest_rust_source_mtime(repo: &Path) -> io::Result<SystemTime> {
    let mut latest =
        file_mtime(&repo.join("Cargo.toml"))?.max(file_mtime(&repo.join("Cargo.lock"))?);
    latest = latest.max(latest_in_dir(&repo.join("crates"))?);
    Ok(latest)
}

fn latest_in_dir(dir: &Path) -> io::Result<SystemTime> {
    let mut latest = SystemTime::UNIX_EPOCH;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            latest = latest.max(latest_in_dir(&path)?);
        } else if is_rust_watched_file(&path) {
            latest = latest.max(file_mtime(&path)?);
        }
    }
    Ok(latest)
}

fn is_rust_watched_file(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "rs")
        || path.file_name().is_some_and(|name| name == "Cargo.toml")
}

fn file_mtime(path: &Path) -> io::Result<SystemTime> {
    Ok(fs::metadata(path)?.modified()?)
}

fn run_dev_cockpit(repo: &Path) -> Result<()> {
    ensure_cockpit_deps(repo)?;
    let bind = daemon_bind();
    run_status(
        Command::new("npm")
            .args([
                "--prefix",
                "cockpit",
                "run",
                "dev",
                "--",
                "--host",
                "127.0.0.1",
                "--port",
            ])
            .arg(cockpit_port())
            .args(["--strictPort"])
            .env("ASYLUM_BASE_URL", base_url(&bind)),
    )
}

fn run_dev_stack(repo: &Path) -> Result<()> {
    let mut daemon = spawn_xtask(repo, "dev-daemon")?;
    let mut cockpit = spawn_xtask(repo, "dev-cockpit")?;

    loop {
        if let Some(status) = daemon.try_wait()? {
            stop_child(&mut cockpit)?;
            return if status.success() {
                Ok(())
            } else {
                Err(format!("dev-daemon exited with {status}").into())
            };
        }
        if let Some(status) = cockpit.try_wait()? {
            stop_child(&mut daemon)?;
            return if status.success() {
                Ok(())
            } else {
                Err(format!("dev-cockpit exited with {status}").into())
            };
        }
        thread::sleep(Duration::from_millis(200));
    }
}

fn spawn_xtask(repo: &Path, command: &str) -> Result<Child> {
    Ok(Command::new(env::current_exe()?)
        .arg(command)
        .current_dir(repo)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?)
}

fn build_stack(repo: &Path) -> Result<()> {
    ensure_cockpit_deps(repo)?;
    run_status(Command::new("npm").args(["--prefix", "cockpit", "run", "build"]))?;
    run_status(Command::new("cargo").args(["build", "--workspace"]))
}

fn test_stack(repo: &Path) -> Result<()> {
    run_status(Command::new("cargo").args(["test", "--workspace"]))?;
    ensure_cockpit_deps(repo)?;
    run_status(Command::new("npm").args(["--prefix", "cockpit", "run", "test"]))
}

fn run_stack(repo: &Path) -> Result<()> {
    ensure_cockpit_deps(repo)?;
    run_status(Command::new("npm").args(["--prefix", "cockpit", "run", "build"]))?;
    run_daemon_once()
}

fn run_installed_asylum(command_name: &str, passthrough: &[String]) -> Result<()> {
    let mut command = Command::new("asylum");
    let args = if passthrough.first().is_some_and(|arg| arg == "--") {
        &passthrough[1..]
    } else {
        passthrough
    };
    command.arg(command_name).args(args);
    run_status(&mut command)
}
