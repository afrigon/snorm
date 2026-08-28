use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::PoisonError;

use anyhow::Context;

use crate::utils::errors::SnormResult;
use crate::utils::shell::Shell;

pub struct SnormContext {
    shell: Arc<Mutex<Shell>>,
    pub cwd: PathBuf,
    pub log_level: tracing::Level
}

impl SnormContext {
    pub fn new(shell: Shell, cwd: PathBuf) -> SnormContext {
        SnormContext {
            shell: Arc::new(Mutex::new(shell)),
            cwd,
            log_level: tracing::Level::WARN
        }
    }

    pub fn default() -> SnormResult<SnormContext> {
        let shell = Shell::new();

        let cwd = env::current_dir().context("could not get the current working directory")?;

        Ok(SnormContext::new(shell, cwd))
    }

    pub fn shell(&self) -> MutexGuard<'_, Shell> {
        self.shell.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
