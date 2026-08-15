//! Persistent `think-loop` child for pinned historical binaries.

use crate::board_position::BoardPosition;
use crate::game_state::{GameState, Move};
use crate::notation::{move_decode, tsfen_encode};
use crate::player::{MoveAnnotation, Player};
use crate::training::record::AgentSpec;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Mutex;

struct ThinkChild {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

pub struct ExternalAbPlayer {
    inner: Mutex<ThinkChild>,
    depth: Option<u32>,
    max_time_ms: Option<u64>,
    model: Option<String>,
}

impl ExternalAbPlayer {
    pub fn spawn(engine: &str, spec: &AgentSpec) -> Result<Self, String> {
        let mut child = Command::new(engine)
            .arg("think-loop")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("spawn {engine} think-loop: {e}"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| format!("{engine}: no stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| format!("{engine}: no stdout"))?;
        Ok(Self {
            inner: Mutex::new(ThinkChild {
                child,
                stdin,
                stdout: BufReader::new(stdout),
            }),
            depth: spec.depth,
            max_time_ms: spec.max_time_ms,
            model: spec.model.clone(),
        })
    }

    fn ask_locked(child: &mut ThinkChild, state: &GameState, spec: &Self) -> Result<Move, String> {
        let fen = tsfen_encode(&BoardPosition::from_state(state));
        writeln!(child.stdin, "position fen {fen}")
            .map_err(|e| format!("think-loop write position: {e}"))?;
        let mut go = String::from("go");
        if let Some(d) = spec.depth {
            go.push_str(&format!(" depth {d}"));
        }
        if let Some(t) = spec.max_time_ms {
            go.push_str(&format!(" time_ms {t}"));
        }
        if let Some(m) = &spec.model {
            go.push_str(&format!(" model {m}"));
        }
        writeln!(child.stdin, "{go}").map_err(|e| format!("think-loop write go: {e}"))?;
        child
            .stdin
            .flush()
            .map_err(|e| format!("think-loop flush: {e}"))?;

        let mut line = String::new();
        loop {
            line.clear();
            let n = child
                .stdout
                .read_line(&mut line)
                .map_err(|e| format!("think-loop read: {e}"))?;
            if n == 0 {
                return Err("think-loop EOF".into());
            }
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("bestmove ") {
                if rest == "(none)" {
                    return Err("think-loop returned no move".into());
                }
                return move_decode(rest);
            }
        }
    }
}

impl Player for ExternalAbPlayer {
    fn name(&self) -> &'static str {
        "ab-ext"
    }

    fn choose_move(&self, state: &GameState) -> Option<Move> {
        self.choose_move_annotated(state).map(|(mv, _)| mv)
    }

    fn choose_move_annotated(&self, state: &GameState) -> Option<(Move, MoveAnnotation)> {
        let mut guard = self.inner.lock().ok()?;
        match Self::ask_locked(&mut guard, state, self) {
            Ok(mv) => Some((mv, MoveAnnotation::default())),
            Err(e) => {
                eprintln!("external engine: {e}");
                None
            }
        }
    }
}

impl Drop for ExternalAbPlayer {
    fn drop(&mut self) {
        if let Ok(mut g) = self.inner.lock() {
            let _ = writeln!(g.stdin, "quit");
            let _ = g.child.kill();
            let _ = g.child.wait();
        }
    }
}
