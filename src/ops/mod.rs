pub mod data_status;
pub mod extract;
pub mod inspect;
pub mod normalize;
pub mod regions;

use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum OutputTarget {
    InPlace,
    Path(PathBuf)
}

impl OutputTarget {
    pub fn resolve(&self, input: &Path) -> PathBuf {
        match self {
            OutputTarget::InPlace => input.to_path_buf(),
            OutputTarget::Path(path) => path.clone()
        }
    }
}
