use super::*;
use std::ffi::OsString;
use std::path::PathBuf;

/// OpenBitFun ACP adapter. Python bridges Craft Anything's ACP client
/// to the user's existing OpenBitFun ACP executable.
pub struct OpenBitFun;

impl AgentAdapter for OpenBitFun {
    fn ids(&self) -> &'static [&'static str] {
        &["openbitfun"]
    }

    fn args(&self, _cx: &ArgsCx) -> Vec<String> {
        Vec::new()
    }

    fn requires_acp(&self) -> bool {
        true
    }

    fn plain_fallback(&self) -> bool {
        false
    }

    fn answers_questions(&self) -> bool {
        false
    }

    /// OpenBitFun ACP does not expose the session-scoped permission controls needed
    /// to guarantee Clean Room, analysis or read-only question access.
    fn validate_access(&self, access: Access) -> Result<(), ToolError> {
        match access {
            Access::Build => Ok(()),
            _ => Err(ToolError::invalid(
                "OpenBitFun currently supports Deep Build only. Its ACP interface does not expose the per-session permission controls required to guarantee this access mode.",
            )
            .with_remediation(
                "Use Deep Build. Other access modes require OpenBitFun to expose session-scoped permission controls.",
            )),
        }
    }

    fn resumes(&self) -> bool {
        true
    }

    /// Extra CLI arguments are refused to prevent permission-mode overrides.
    fn validate_options(&self, options: &BackendOptions) -> Result<(), ToolError> {
        if options.args.is_empty() {
            Ok(())
        } else {
            Err(ToolError::invalid("Extra OpenBitFun arguments are disabled to protect ACP permission handling.")
                .with_remediation("Remove extra arguments. Configure OpenBitFun models, authentication and permissions in OpenBitFun itself."))
        }
    }

    /// Never forward arbitrary backend arguments to the OpenBitFun process.
    fn acp_options(&self, _options: &BackendOptions, _access: Access) -> Vec<String> {
        Vec::new()
    }

    /// Launch Python -> bridge -> configured OpenBitFun executable.
    ///
    /// The generic ACP runner appends the registered `acp` argument
    /// after these arguments, producing:
    /// python bridge.py <openbitfun-exe> acp
    fn process(&self, program: &str) -> Process {
        let python = std::env::var_os("CRAFT_OPENBITFUN_PYTHON").unwrap_or_else(|| OsString::from(if cfg!(windows) { "python" } else { "python3" }));

        let bridge = bridge_path();

        Process::new(python).args([bridge.into_os_string(), OsString::from(program)])
    }

    fn sign_in_fix(&self) -> Option<String> {
        Some(
            "Run the configured OpenBitFun executable with `acp doctor` in a terminal, then try again. Craft Anything preserves your existing OpenBitFun authentication and configuration."
                .into(),
        )
    }
}

/// Find the bridge script without requiring it to be installed beside
/// the OpenBitFun executable.
fn bridge_path() -> PathBuf {
    const FILE: &str = "openbitfun-acp-bridge.py";

    if let Some(path) = std::env::var_os("CRAFT_OPENBITFUN_ACP_BRIDGE") {
        return PathBuf::from(path);
    }

    let mut candidates = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(directory) = exe.parent() {
            candidates.push(directory.join("scripts").join(FILE));
            candidates.push(directory.join(FILE));
        }
    }

    let source_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts").join(FILE);

    candidates.push(source_path.clone());

    candidates.into_iter().find(|path| path.is_file()).unwrap_or(source_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openbitfun_is_registered_as_acp_only() {
        let adapter = AgentAdapters::built_in().for_agent("openbitfun");

        assert!(adapter.ids().contains(&"openbitfun"));
        assert!(adapter.requires_acp());
        assert!(!adapter.plain_fallback());
        assert!(!adapter.answers_questions());
        assert!(adapter.validate_access(Access::Build).is_ok());

        for access in [Access::Analyse, Access::CleanRoomAnalyse, Access::CleanRoomBuild, Access::CleanRoomQuestion, Access::Question] {
            assert!(adapter.validate_access(access).is_err());
        }
        assert!(adapter.resumes());

        assert_eq!(crate::cli_agents::acp::acp_args("openbitfun"), Some(&["acp"][..]));
    }

    #[test]
    fn extra_arguments_are_rejected() {
        let options = BackendOptions { args: vec!["--some-override".into()], ..Default::default() };

        assert!(OpenBitFun.validate_options(&options).is_err());
        assert!(OpenBitFun.validate_options(&BackendOptions::default()).is_ok());
    }
}
