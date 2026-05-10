use std::env;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
    let command = args.next().unwrap_or_else(|| "help".to_string());
    let passthrough: Vec<String> = args.collect();

    match command.as_str() {
        "run-asylum-dev" => run_asylum_dev(&repo),
        "run-daemon-dev" => run_daemon_dev(&repo),
        "run-cockpit-dev" => run_cockpit_dev(&repo),
        "run-asylum" => run_asylum(&repo),
        "run-daemon" => run_daemon_once(&repo),
        "build-asylum" => build_asylum(&repo),
        "build-rust" => build_rust(),
        "build-cockpit" => build_cockpit(&repo),
        "build-asylum-release" => {
            run_repo_script(&repo, "build-release-artifacts.sh", &passthrough)
        }
        "test-asylum" => test_asylum(&repo),
        "test-rust" => test_rust(),
        "test-cockpit" => test_cockpit(&repo),
        "test-asylum-release" => test_asylum_release(&repo, &passthrough),
        "check-asylum" => check_asylum(&repo),
        "status-asylum-dev" => status_asylum_dev(&repo),
        "stop-asylum-dev" => stop_asylum_dev(&repo),
        "reset-asylum-dev" => reset_asylum_dev(&repo),
        "publish-asylum-release" => run_repo_script(&repo, "publish-release.sh", &passthrough),
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
Asylum source workflows:
  run-asylum-dev        Run daemon + Cockpit with source hot reload/watch
  run-daemon-dev        Run source daemon only, watched/restarted on Rust changes
  run-cockpit-dev       Run Cockpit/Vite only with hot reload
  run-asylum            Build Cockpit once, then run the source daemon
  run-daemon            Run the source daemon once with no watch
  build-asylum          Build Cockpit assets and the Rust workspace
  build-rust            Build the Rust workspace only
  build-cockpit         Build Cockpit production assets only
  build-asylum-release  Build release artifacts via scripts/build-release-artifacts.sh
  test-asylum           Run Rust and Cockpit tests
  test-rust             Run Rust workspace tests only
  test-cockpit          Run Cockpit tests only
  test-asylum-release   Run release-install smoke via scripts/test-release-install.sh
  check-asylum          Run fast source preflight checks
  status-asylum-dev     Show source-dev ports, processes, and .asylum-dev state
  stop-asylum-dev       Stop source-dev daemon/Vite processes
  reset-asylum-dev      Stop source-dev processes and remove .asylum-dev
  publish-asylum-release Publish release artifacts via scripts/publish-release.sh
  help                  Show this help

Cargo aliases:
  cargo run-asylum-dev
  cargo run-daemon-dev
  cargo run-cockpit-dev
  cargo run-asylum
  cargo run-daemon
  cargo build-asylum
  cargo build-rust
  cargo build-cockpit
  cargo build-asylum-release
  cargo test-asylum
  cargo test-rust
  cargo test-cockpit
  cargo test-asylum-release
  cargo check-asylum
  cargo status-asylum-dev
  cargo stop-asylum-dev
  cargo reset-asylum-dev
  cargo publish-asylum-release"
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

fn run_daemon_once(repo: &Path) -> Result<()> {
    let bind = daemon_bind();
    let mut command = Command::new("cargo");
    command
        .args(["run", "-p", "asylum", "--", "daemon", "run", "--bind"])
        .arg(&bind);
    apply_dev_env(&mut command, repo, &bind);
    run_status(&mut command)
}

fn run_built_daemon(repo: &Path) -> Result<Child> {
    let bind = daemon_bind();
    let mut command = Command::new(debug_asylum_binary(repo)?);
    command.args(["daemon", "run", "--bind"]).arg(&bind);
    apply_dev_env(&mut command, repo, &bind);
    Ok(command.spawn()?)
}

fn run_daemon_dev(repo: &Path) -> Result<()> {
    if env::var("ASYLUM_DEV_DAEMON_WATCH").as_deref() == Ok("0") {
        return run_daemon_once(repo);
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

fn run_cockpit_dev(repo: &Path) -> Result<()> {
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

fn run_asylum_dev(repo: &Path) -> Result<()> {
    let mut daemon = spawn_xtask(repo, "run-daemon-dev")?;
    let mut cockpit = spawn_xtask(repo, "run-cockpit-dev")?;

    loop {
        if let Some(status) = daemon.try_wait()? {
            stop_child(&mut cockpit)?;
            return if status.success() {
                Ok(())
            } else {
                Err(format!("run-daemon-dev exited with {status}").into())
            };
        }
        if let Some(status) = cockpit.try_wait()? {
            stop_child(&mut daemon)?;
            return if status.success() {
                Ok(())
            } else {
                Err(format!("run-cockpit-dev exited with {status}").into())
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

fn build_asylum(repo: &Path) -> Result<()> {
    build_cockpit(repo)?;
    build_rust()
}

fn build_rust() -> Result<()> {
    run_status(Command::new("cargo").args(["build", "--workspace"]))
}

fn build_cockpit(repo: &Path) -> Result<()> {
    ensure_cockpit_deps(repo)?;
    run_status(Command::new("npm").args(["--prefix", "cockpit", "run", "build"]))
}

fn test_asylum(repo: &Path) -> Result<()> {
    test_rust()?;
    test_cockpit(repo)
}

fn test_rust() -> Result<()> {
    run_status(Command::new("cargo").args(["test", "--workspace"]))?;
    Ok(())
}

fn test_cockpit(repo: &Path) -> Result<()> {
    ensure_cockpit_deps(repo)?;
    run_status(Command::new("npm").args(["--prefix", "cockpit", "run", "test"]))
}

fn run_asylum(repo: &Path) -> Result<()> {
    build_cockpit(repo)?;
    run_daemon_once(repo)
}

fn check_asylum(repo: &Path) -> Result<()> {
    run_status(Command::new("cargo").args(["fmt", "--all", "--check"]))?;
    run_status(Command::new("cargo").args(["check", "--workspace"]))?;
    build_cockpit(repo)
}

fn run_repo_script(repo: &Path, script_name: &str, passthrough: &[String]) -> Result<()> {
    run_repo_script_args(repo, script_name, normalized_passthrough(passthrough))
}

fn run_repo_script_args(repo: &Path, script_name: &str, args: &[String]) -> Result<()> {
    let mut command = Command::new(repo.join("scripts").join(script_name));
    command.args(args);
    run_status(&mut command)
}

fn normalized_passthrough(passthrough: &[String]) -> &[String] {
    if passthrough.first().is_some_and(|arg| arg == "--") {
        &passthrough[1..]
    } else {
        passthrough
    }
}

fn test_asylum_release(repo: &Path, passthrough: &[String]) -> Result<()> {
    let args = ReleaseSmokeArgs::parse(normalized_passthrough(passthrough))?;
    if args.help {
        release_smoke_usage();
        return Ok(());
    }

    let version = if let Some(version) = args.version {
        version
    } else {
        format!("v{}", workspace_version(repo)?)
    };
    let semver = version.trim_start_matches('v').to_string();
    let artifact_dir = args
        .artifact_dir
        .unwrap_or_else(|| repo.join("dist").join("release").join(&version));
    let archive = artifact_dir.join(host_release_archive_name()?);

    if !archive.is_file() {
        return Err(format!(
            "release archive is missing: {}; run `cargo build-asylum-release` first",
            archive.display()
        )
        .into());
    }

    let scratch = ScratchDir::new("asylum-release-smoke", args.keep)?;
    let extract_dir = scratch.path.join("extract");
    let home_dir = scratch.path.join("home");
    fs::create_dir_all(&extract_dir)?;
    fs::create_dir_all(&home_dir)?;

    run_status(
        Command::new("tar")
            .args(["-xzf"])
            .arg(&archive)
            .args(["-C"])
            .arg(&extract_dir),
    )?;

    let binary = extract_dir.join(format!("asylum{}", env::consts::EXE_SUFFIX));
    if !binary.is_file() {
        return Err(format!("release archive did not contain {}", binary.display()).into());
    }

    let output = Command::new(&binary).arg("--version").output()?;
    if !output.status.success() {
        return Err(format!(
            "{} --version exited with {}",
            binary.display(),
            output.status
        )
        .into());
    }
    let actual_version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let expected_version = format!("asylum {semver}");
    if actual_version != expected_version {
        return Err(format!(
            "unexpected release binary version: expected `{expected_version}`, got `{actual_version}`"
        )
        .into());
    }

    run_status(
        Command::new(&binary)
            .arg("setup")
            .env("ASYLUM_HOME", &home_dir),
    )?;
    run_status(
        Command::new(&binary)
            .args(["doctor", "--verbose"])
            .env("ASYLUM_HOME", &home_dir),
    )?;

    println!(
        "Release archive smoke passed for {version}: {}",
        archive.display()
    );
    Ok(())
}

#[derive(Debug, Default)]
struct ReleaseSmokeArgs {
    version: Option<String>,
    artifact_dir: Option<PathBuf>,
    keep: bool,
    help: bool,
}

impl ReleaseSmokeArgs {
    fn parse(args: &[String]) -> Result<Self> {
        let mut parsed = ReleaseSmokeArgs::default();
        let mut iter = args.iter();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--version" => {
                    let value = iter.next().ok_or("missing value for --version")?;
                    parsed.version = Some(normalize_release_version(value));
                }
                "--artifact-dir" => {
                    let value = iter.next().ok_or("missing value for --artifact-dir")?;
                    parsed.artifact_dir = Some(PathBuf::from(value));
                }
                "--keep" => parsed.keep = true,
                "--help" | "-h" => parsed.help = true,
                other => return Err(format!("unknown test-asylum-release option: {other}").into()),
            }
        }
        Ok(parsed)
    }
}

fn release_smoke_usage() {
    println!(
        "\
Usage: cargo test-asylum-release [-- --version vX.Y.Z] [--artifact-dir <path>] [--keep]

Smoke-test the local release archive for this host platform. By default it
expects artifacts under dist/release/v<workspace-version>/ and uses isolated
temporary ASYLUM_HOME state."
    );
}

fn normalize_release_version(value: &str) -> String {
    if value.starts_with('v') {
        value.to_string()
    } else {
        format!("v{value}")
    }
}

fn workspace_version(repo: &Path) -> Result<String> {
    let manifest = fs::read_to_string(repo.join("Cargo.toml"))?;
    for line in manifest.lines() {
        let line = line.trim();
        if let Some(version) = line
            .strip_prefix("version = \"")
            .and_then(|value| value.strip_suffix('"'))
        {
            return Ok(version.to_string());
        }
    }
    Err("workspace Cargo.toml does not define version".into())
}

fn host_release_archive_name() -> Result<&'static str> {
    match (env::consts::OS, env::consts::ARCH) {
        ("linux", "x86_64") => Ok("asylum-linux-x86_64.tar.gz"),
        ("linux", "aarch64") => Ok("asylum-linux-arm64.tar.gz"),
        ("macos", "aarch64") => Ok("asylum-darwin-arm64.tar.gz"),
        ("macos", "x86_64") => Ok("asylum-darwin-x86_64.tar.gz"),
        (os, arch) => Err(format!("unsupported release-smoke host platform: {os}/{arch}").into()),
    }
}

struct ScratchDir {
    path: PathBuf,
    keep: bool,
}

impl ScratchDir {
    fn new(prefix: &str, keep: bool) -> Result<Self> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_nanos()
            .to_string();
        let path = env::temp_dir().join(format!("{prefix}-{}-{unique}", std::process::id()));
        fs::create_dir_all(&path)?;
        Ok(Self { path, keep })
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        if self.keep {
            println!("Kept scratch directory: {}", self.path.display());
            return;
        }
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn status_asylum_dev(repo: &Path) -> Result<()> {
    let bind = daemon_bind();
    let cockpit_addr = format!("127.0.0.1:{}", cockpit_port());
    let home = dev_home(repo);

    println!("Asylum source-dev status");
    println!("  repo: {}", repo.display());
    println!("  runtime: {}", home.display());
    println!(
        "  daemon: http://{} ({})",
        bind,
        if tcp_listening(&bind) {
            "listening"
        } else {
            "not listening"
        }
    );
    println!(
        "  cockpit: http://{} ({})",
        cockpit_addr,
        if tcp_listening(&cockpit_addr) {
            "listening"
        } else {
            "not listening"
        }
    );
    println!(
        "  runtime state: {}",
        if home.exists() { "present" } else { "absent" }
    );

    let processes = matching_source_dev_processes(repo)?;
    if processes.is_empty() {
        println!("  processes: none detected");
    } else {
        println!("  processes:");
        for process in processes {
            println!("    {} {}", process.pid, process.command);
        }
    }

    Ok(())
}

fn stop_asylum_dev(repo: &Path) -> Result<()> {
    let processes = matching_source_dev_processes(repo)?;
    if processes.is_empty() {
        println!("No Asylum source-dev processes detected.");
        return Ok(());
    }

    for process in &processes {
        println!("Stopping {} {}", process.pid, process.command);
        let _ = Command::new("kill")
            .args(["-TERM", &process.pid.to_string()])
            .status();
    }

    thread::sleep(Duration::from_millis(800));

    for process in processes {
        if pid_alive(process.pid) {
            eprintln!(
                "Process {} did not stop after TERM; sending KILL",
                process.pid
            );
            let _ = Command::new("kill")
                .args(["-KILL", &process.pid.to_string()])
                .status();
        }
    }

    Ok(())
}

fn reset_asylum_dev(repo: &Path) -> Result<()> {
    stop_asylum_dev(repo)?;
    let home = dev_home(repo);
    if home.exists() {
        fs::remove_dir_all(&home)?;
        println!("Removed {}", home.display());
    } else {
        println!("Runtime state already absent: {}", home.display());
    }
    Ok(())
}

fn tcp_listening(address: &str) -> bool {
    let Ok(address) = address.parse::<SocketAddr>() else {
        return false;
    };
    TcpStream::connect_timeout(&address, Duration::from_millis(150)).is_ok()
}

#[derive(Debug)]
struct ProcessInfo {
    pid: u32,
    command: String,
}

fn matching_source_dev_processes(repo: &Path) -> Result<Vec<ProcessInfo>> {
    let bind = daemon_bind();
    let cockpit_port = cockpit_port();
    let output = Command::new("ps")
        .args(["-axo", "pid=,command="])
        .output()?;
    if !output.status.success() {
        return Err(format!("ps exited with {}", output.status).into());
    }

    let current_pid = std::process::id();
    let mut matches = Vec::new();

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Some((pid, command)) = parse_ps_line(line) else {
            continue;
        };
        if pid == current_pid {
            continue;
        }

        let in_repo = process_cwd_matches_repo(pid, repo);
        let is_source_daemon = command.contains("target/debug/asylum daemon run")
            && command.contains("--bind")
            && command.contains(&bind)
            && in_repo;
        let is_cargo_source_daemon = command.contains("cargo run -p asylum -- daemon run")
            && command.contains("--bind")
            && command.contains(&bind)
            && in_repo;
        let is_xtask_dev = command.contains("target/debug/xtask")
            && (command.contains("run-asylum-dev")
                || command.contains("run-daemon-dev")
                || command.contains("run-cockpit-dev"))
            && in_repo;
        let is_cockpit_npm = command.contains("npm --prefix cockpit run dev") && in_repo;
        let is_cockpit_vite = command.contains("vite")
            && command.contains("--host 127.0.0.1")
            && command.contains(&format!("--port {cockpit_port}"))
            && in_repo;

        if is_source_daemon
            || is_cargo_source_daemon
            || is_xtask_dev
            || is_cockpit_npm
            || is_cockpit_vite
        {
            matches.push(ProcessInfo { pid, command });
        }
    }

    Ok(matches)
}

fn process_cwd_matches_repo(pid: u32, repo: &Path) -> bool {
    fs::read_link(format!("/proc/{pid}/cwd"))
        .map(|cwd| cwd.starts_with(repo))
        .unwrap_or(true)
}

fn parse_ps_line(line: &str) -> Option<(u32, String)> {
    let trimmed = line.trim_start();
    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let pid = parts.next()?;
    let command = parts.next()?;
    Some((pid.parse().ok()?, command.trim_start().to_string()))
}

fn pid_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .is_ok_and(|status| status.success())
}
