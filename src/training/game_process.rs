//! One immutable engine selection per game. Scheduling remains in the coordinator.
use super::record::GameRecordV2;
use super::worker::{play_one_game, WorkerConfig};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
    sync::atomic::Ordering,
    time::Duration,
};

pub const PROTOCOL: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EngineBundle {
    pub protocol: u32,
    pub revision: String,
    pub engine_bin: String,
    pub engine_sha256: String,
    pub analyzer_bin: String,
    pub analyzer_sha256: String,
}

pub fn file_hash(path: &Path) -> Result<String, String> {
    use std::io::Read;
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

impl EngineBundle {
    pub fn load(path: &Path) -> Result<Self, String> {
        let bundle: Self = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        if bundle.protocol != PROTOCOL {
            return Err("unsupported game worker protocol".into());
        }
        for (path, expected) in [
            (&bundle.engine_bin, &bundle.engine_sha256),
            (&bundle.analyzer_bin, &bundle.analyzer_sha256),
        ] {
            if !Path::new(path).is_absolute() || file_hash(Path::new(path))? != *expected {
                return Err(format!("engine bundle missing or changed: {path}"));
            }
        }
        Ok(bundle)
    }
}

#[derive(Serialize, Deserialize)]
struct Request {
    protocol: u32,
    bundle: EngineBundle,
    config: WorkerConfig,
}

fn bind_agents(config: &mut WorkerConfig, bundle: &EngineBundle) -> Result<(), String> {
    for agent in [&mut config.black, &mut config.white] {
        if let Some(engine) = &agent.engine {
            agent.engine_sha256 = Some(file_hash(Path::new(engine))?);
            agent.engine_build = None;
        } else {
            agent.engine_sha256 = Some(bundle.engine_sha256.clone());
            agent.engine_build = Some(bundle.clone());
        }
    }
    Ok(())
}

/// Internal CLI entrypoint: writes a complete game only after success.
pub fn child_main(request: &Path, output: &Path) -> Result<(), String> {
    let req: Request = serde_json::from_slice(&fs::read(request).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if req.protocol != PROTOCOL || req.bundle.protocol != PROTOCOL {
        return Err("unsupported game worker protocol".into());
    }
    if file_hash(&std::env::current_exe().map_err(|e| e.to_string())?)? != req.bundle.engine_sha256
    {
        return Err("game worker executable does not match request".into());
    }
    let mut expected = req.config.clone();
    bind_agents(&mut expected, &req.bundle)?;
    if expected.black != req.config.black || expected.white != req.config.white {
        return Err("agent executable changed after admission".into());
    }
    let record = play_one_game(&req.config).map_err(|e| e.message)?;
    let temporary = output.with_extension("tmp");
    record.save_path(&temporary)?;
    fs::rename(temporary, output).map_err(|e| e.to_string())
}

#[cfg(unix)]
struct GameChild(std::process::Child);
#[cfg(unix)]
impl Drop for GameChild {
    fn drop(&mut self) {
        // The process group also contains historical think-loop children.
        unsafe {
            libc::kill(-(self.0.id() as i32), libc::SIGKILL);
        }
        let _ = self.0.wait();
    }
}

/// Older binaries ignore unknown JSON fields; never let them silently ignore clocks.
pub fn require_clock_support(binary: &str) -> Result<(), String> {
    let mut child = Command::new(binary).arg("tournament-game-clock-protocol")
        .stdout(Stdio::piped()).stderr(Stdio::null()).spawn()
        .map_err(|e| format!("clock capability probe failed: {e}"))?;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while child.try_wait().map_err(|e| e.to_string())?.is_none() {
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("clock capability probe timed out".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if !output.status.success() || output.stdout != b"1\n" {
        return Err(
            "game engine does not support Fischer clocks; publish a clock-capable bundle first"
                .into(),
        );
    }
    Ok(())
}

#[cfg(unix)]
pub fn play(
    config: &WorkerConfig,
    pointer: &Path,
    directory: &Path,
    slot: u64,
) -> Result<GameRecordV2, String> {
    use std::os::unix::process::CommandExt;
    let bundle = EngineBundle::load(pointer)?; // Exactly once per game, never per move.
    if config.time_control.is_some() {
        require_clock_support(&bundle.engine_bin)?;
    }
    fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let request = directory.join(format!("slot-{slot}.request.json"));
    let output = directory.join(format!("slot-{slot}.result.json"));
    // An interrupted attempt must never be mistaken for a successful retry.
    if output.exists() {
        fs::remove_file(&output).map_err(|e| e.to_string())?;
    }
    let mut game_config = config.clone();
    bind_agents(&mut game_config, &bundle)?;
    fs::write(
        &request,
        serde_json::to_vec(&Request {
            protocol: PROTOCOL,
            bundle: bundle.clone(),
            config: game_config.clone(),
        })
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let parent = unsafe { libc::getpid() };
    let mut command = Command::new(&bundle.engine_bin);
    command
        .args(["tournament-game"])
        .arg(&request)
        .arg(&output)
        .stdin(Stdio::null())
        .process_group(0);
    unsafe {
        command.pre_exec(move || {
            #[cfg(target_os = "linux")]
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::getppid() != parent {
                return Err(std::io::Error::other("coordinator exited"));
            }
            Ok(())
        });
    }
    let mut child = GameChild(
        command
            .spawn()
            .map_err(|e| format!("game spawn failed: {e}"))?,
    );
    loop {
        if config
            .stop
            .as_ref()
            .is_some_and(|s| s.load(Ordering::Relaxed))
        {
            return Err("stopped".into());
        }
        if let Some(status) = child.0.try_wait().map_err(|e| e.to_string())? {
            if !status.success() {
                return Err(format!(
                    "game worker exited {status}; request: {}",
                    request.display()
                ));
            }
            let record = GameRecordV2::load_path(&output)?;
            if record.stats.clock.as_ref().map(|c| c.control) != config.time_control
                || record
                    .stats
                    .clock
                    .as_ref()
                    .is_some_and(|c| c.moves.len() != record.moves.len())
                || record.result.is_none()
                || record.abort_reason.is_some()
                || record.seed != config.seed
                || record.black != game_config.black
                || record.white != game_config.white
                || serde_json::to_value(&record.start).unwrap()
                    != serde_json::to_value(&config.start).unwrap()
            {
                return Err("incomplete or mismatched game worker result".into());
            }
            fs::remove_file(&request).map_err(|e| e.to_string())?;
            fs::remove_file(&output).map_err(|e| e.to_string())?;
            return Ok(record);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(not(unix))]
pub fn play(_: &WorkerConfig, _: &Path, _: &Path, _: u64) -> Result<GameRecordV2, String> {
    Err("rolling game processes require Unix".into())
}
