//! Optional Linux worker affinity and game-boundary shared-CPU admission.
use std::{
    fs::{File, OpenOptions},
    os::fd::AsRawFd,
    path::PathBuf,
};
pub struct ComputeGate {
    dir: PathBuf,
    cpus: Vec<usize>,
}
impl ComputeGate {
    pub fn from_env(jobs: usize) -> Result<Option<Self>, String> {
        let Some(dir) = std::env::var_os("TAIKYOKU_COMPUTE_DIR") else {
            return Ok(None);
        };
        let cpus: Vec<usize> = std::env::var("TAIKYOKU_COMPUTE_CPUS")
            .map_err(|e| e.to_string())?
            .split(',')
            .map(|s| s.parse().map_err(|_| "invalid CPU".to_string()))
            .collect::<Result<_, _>>()?;
        if cpus.len() != jobs || jobs != 4 || cpus.iter().any(|c| *c >= libc::CPU_SETSIZE as usize)
        {
            return Err("adaptive compute requires four valid CPUs and --jobs 4".into());
        }
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        Ok(Some(Self {
            dir: dir.into(),
            cpus,
        }))
    }
    pub fn pin(&self, worker: usize) -> Result<(), String> {
        #[cfg(target_os = "linux")]
        unsafe {
            let mut set: libc::cpu_set_t = std::mem::zeroed();
            libc::CPU_ZERO(&mut set);
            libc::CPU_SET(self.cpus[worker], &mut set);
            if libc::sched_setaffinity(0, std::mem::size_of_val(&set), &set) != 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
        }
        #[cfg(not(target_os = "linux"))]
        return Err("adaptive compute requires Linux".into());
        Ok(())
    }
    /// None means the fourth worker must wait; dropping File releases its lock.
    pub fn claim(&self) -> Result<Option<File>, String> {
        if self.dir.join("analysis.request").exists() {
            return Ok(None);
        }
        let f = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.dir.join("shared.lock"))
            .map_err(|e| e.to_string())?;
        if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            let e = std::io::Error::last_os_error();
            if e.kind() == std::io::ErrorKind::WouldBlock {
                return Ok(None);
            }
            return Err(e.to_string());
        }
        if self.dir.join("analysis.request").exists() {
            return Ok(None);
        }
        Ok(Some(f))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn request_drains_existing_lease_and_blocks_new_games() {
        let dir = std::env::temp_dir().join(format!("compute-gate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let gate = ComputeGate {
            dir: dir.clone(),
            cpus: vec![0, 1, 2, 3],
        };
        let game = gate.claim().unwrap().unwrap();
        std::fs::write(dir.join("analysis.request"), "").unwrap();
        assert!(gate.claim().unwrap().is_none());
        let analysis = OpenOptions::new()
            .read(true)
            .write(true)
            .open(dir.join("shared.lock"))
            .unwrap();
        assert_ne!(
            unsafe { libc::flock(analysis.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
            0
        );
        drop(game);
        assert_eq!(
            unsafe { libc::flock(analysis.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
            0
        );
        std::fs::remove_file(dir.join("analysis.request")).unwrap();
        assert!(gate.claim().unwrap().is_none());
        drop(analysis);
        assert!(gate.claim().unwrap().is_some());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
