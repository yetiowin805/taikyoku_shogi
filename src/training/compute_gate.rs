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
        if cpus.len() != jobs
            || ![4, 8].contains(&jobs)
            || cpus.iter().collect::<std::collections::BTreeSet<_>>().len() != jobs
            || cpus.iter().any(|c| *c >= libc::CPU_SETSIZE as usize)
        {
            return Err(
                "adaptive compute requires four or eight distinct valid CPUs matching --jobs"
                    .into(),
            );
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
    /// Eight-CPU admission: worker 0 remains a game player, worker 2 top-two.
    /// Demand and reservations share one cross-process lock with the analyzer.
    pub fn claim_worker(&self, worker: usize) -> Result<Option<File>, String> {
        if self.cpus.len() == 4 {
            return self.claim();
        }
        let control = lock_file(self.dir.join("allocation.lock"), false)?.unwrap();
        let path = self.dir.join("analysis-demand.json");
        if worker != 0 && worker != 2 && path.exists() {
            let mut demand: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs_f64();
            let pid = demand["pid"].as_i64().unwrap_or(0) as i32;
            let alive = pid > 0
                && unsafe { libc::kill(pid, 0) } == 0
                && now - demand["updated"].as_f64().unwrap_or(0.0) < 30.0;
            if alive {
                let pending = demand["pending"].as_u64().unwrap_or(0) as usize;
                let assigned = demand["assigned"]
                    .as_array_mut()
                    .ok_or("invalid analysis reservations")?;
                if assigned.contains(&serde_json::json!(worker)) {
                    return Ok(None);
                }
                if assigned.len() < 6 && pending > assigned.len() {
                    assigned.push(serde_json::json!(worker));
                    let tmp = path.with_extension("rust.tmp");
                    std::fs::write(&tmp, serde_json::to_vec(&demand).unwrap())
                        .map_err(|e| e.to_string())?;
                    std::fs::rename(tmp, path).map_err(|e| e.to_string())?;
                    return Ok(None);
                }
            }
        }
        let lease = lock_file(self.dir.join(format!("cpu-{worker}.lock")), true)?;
        drop(control);
        Ok(lease)
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
    fn eight_cpu_demand_reserves_only_available_flexible_workers() {
        let dir = std::env::temp_dir().join(format!("eight-gate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let gate = ComputeGate {
            dir: dir.clone(),
            cpus: (0..8).collect(),
        };
        let path = dir.join("analysis-demand.json");
        let write = |pending: usize, assigned: Vec<usize>, age: f64| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs_f64();
            std::fs::write(&path, serde_json::to_vec(&serde_json::json!({
                "pid": std::process::id(), "updated": now-age, "pending": pending, "assigned": assigned
            })).unwrap()).unwrap();
        };
        let game = gate.claim_worker(1).unwrap().unwrap();
        write(1, vec![], 0.0);
        assert!(lock_file(dir.join("cpu-1.lock"), true).unwrap().is_none());
        drop(game); // an in-flight game is never interrupted by new demand
        assert!(gate.claim_worker(1).unwrap().is_none());
        assert!(gate.claim_worker(3).unwrap().is_some()); // one position, not six
        assert!(gate.claim_worker(0).unwrap().is_some());
        assert!(gate.claim_worker(2).unwrap().is_some());
        write(100, vec![], 0.0);
        for worker in [1, 3, 4, 5, 6, 7] {
            assert!(gate.claim_worker(worker).unwrap().is_none());
        }
        let state: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(state["assigned"].as_array().unwrap().len(), 6);
        write(0, vec![], 0.0);
        for worker in 0..8 {
            assert!(gate.claim_worker(worker).unwrap().is_some());
        }
        write(100, vec![1, 3, 4, 5, 6, 7], 31.0);
        assert!(gate.claim_worker(1).unwrap().is_some()); // dead manager heartbeat
        let analysis = lock_file(dir.join("cpu-1.lock"), true).unwrap().unwrap();
        assert!(gate.claim_worker(1).unwrap().is_none()); // stale demand cannot break a live lease
        drop(analysis);
        std::fs::remove_dir_all(dir).unwrap();
    }

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

fn lock_file(path: PathBuf, nonblocking: bool) -> Result<Option<File>, String> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    let flags = libc::LOCK_EX | if nonblocking { libc::LOCK_NB } else { 0 };
    if unsafe { libc::flock(file.as_raw_fd(), flags) } != 0 {
        let e = std::io::Error::last_os_error();
        if nonblocking && e.kind() == std::io::ErrorKind::WouldBlock {
            return Ok(None);
        }
        return Err(e.to_string());
    }
    Ok(Some(file))
}
