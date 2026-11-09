//! Configuration defaults and validation.

#[derive(Debug, Clone)]
pub struct ScanConfig {
    pub runs: u32,
    pub timeout_secs: Option<u64>,
    pub output_dir: Option<std::path::PathBuf>,
    pub no_history: bool,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            runs: 10,
            timeout_secs: None,
            output_dir: None,
            no_history: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ScanConfig;

    #[test]
    fn default_runs_is_ten() {
        let cfg = ScanConfig::default();
        assert_eq!(cfg.runs, 10);
        assert!(!cfg.no_history);
    }
}
