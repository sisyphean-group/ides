use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub schema_version: u64,
    pub set_id: String,
    pub name: String,
    pub auto_start: bool,
    pub runtime_shell: String,
    #[serde(default)]
    pub runtime: ManifestRuntime,
    pub services: BTreeMap<String, Service>,
}

impl Manifest {
    pub fn from_json(raw: &str) -> serde_json::Result<Self> {
        serde_json::from_str(raw)
    }

    pub fn selected_services<'a>(
        &'a self,
        names: &[String],
    ) -> Result<Vec<(&'a str, &'a Service)>, String> {
        if names.is_empty() {
            return Ok(self
                .services
                .iter()
                .map(|(name, service)| (name.as_str(), service))
                .collect());
        }

        names
            .iter()
            .map(|name| {
                self.services
                    .get_key_value(name)
                    .map(|(name, service)| (name.as_str(), service))
                    .ok_or_else(|| format!("manifest has no service `{name}`"))
            })
            .collect()
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    pub unit_name: String,
    pub exec: Exec,
    #[serde(default)]
    pub configs: BTreeMap<String, ConfigFile>,
    #[serde(default)]
    pub activation: Activation,
    #[serde(default)]
    pub runtime: ServiceRuntime,
    #[serde(default)]
    pub systemd: ServiceSystemd,
    #[serde(default)]
    pub systemd_args: String,
    #[serde(default)]
    pub dependencies: Dependencies,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFile {
    pub path: Option<String>,
    pub runtime: Option<RuntimeConfig>,
}

impl ConfigFile {
    pub fn display_path(&self) -> String {
        self.path.clone().unwrap_or_else(|| {
            self.runtime
                .as_ref()
                .map(|runtime| format!("<runtime:{}>", runtime.file_name))
                .unwrap_or_else(|| "<missing>".to_string())
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeConfig {
    pub file_name: String,
    #[serde(default)]
    pub parts: Vec<RuntimeConfigPart>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum RuntimeConfigPart {
    Text(String),
    RuntimePath {
        #[serde(rename = "runtimePath")]
        runtime_path: String,
    },
    Env {
        env: String,
    },
}

impl Service {
    pub fn systemd_unit_name(&self) -> String {
        if self.unit_name.contains('.') {
            self.unit_name.clone()
        } else {
            format!("{}.service", self.unit_name)
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Activation {
    #[serde(default)]
    pub socket: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub path: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub timer: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestRuntime {
    pub base: String,
}

impl Default for ManifestRuntime {
    fn default() -> Self {
        Self {
            base: "ides/unknown".to_string(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceRuntime {
    #[serde(default = "default_ephemeral")]
    pub ephemeral: bool,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub paths: RuntimePaths,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSystemd {
    #[serde(default)]
    pub service_type: Option<String>,
    #[serde(default)]
    pub notify_access: Option<String>,
    #[serde(default)]
    pub restart: Option<String>,
    #[serde(default)]
    pub restart_sec: Option<String>,
    #[serde(default)]
    pub timeout_sec: Option<String>,
    #[serde(default)]
    pub timeout_start_sec: Option<String>,
    #[serde(default)]
    pub timeout_stop_sec: Option<String>,
    #[serde(default)]
    pub watchdog_sec: Option<String>,
    #[serde(default)]
    pub working_directory: Option<String>,
    #[serde(default)]
    pub kill_mode: Option<String>,
    #[serde(default)]
    pub remain_after_exit: Option<bool>,
    #[serde(default)]
    pub exec_start_pre: Vec<SystemdExecCommand>,
    #[serde(default)]
    pub exec_start_post: Vec<SystemdExecCommand>,
    #[serde(default)]
    pub exec_reload: Vec<SystemdExecCommand>,
    #[serde(default)]
    pub exec_stop: Vec<SystemdExecCommand>,
    #[serde(default)]
    pub ignore_start_failure: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum SystemdExecCommand {
    Structured(SystemdExecCommandSpec),
    String(String),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemdExecCommandSpec {
    pub command: String,
    #[serde(default)]
    pub ignore_failure: bool,
}

impl SystemdExecCommand {
    pub fn command(&self) -> &str {
        match self {
            Self::Structured(command) => &command.command,
            Self::String(command) => command,
        }
    }

    pub fn ignore_failure(&self) -> bool {
        match self {
            Self::Structured(command) => command.ignore_failure,
            Self::String(_) => false,
        }
    }
}

impl Default for ServiceRuntime {
    fn default() -> Self {
        Self {
            ephemeral: true,
            env: BTreeMap::new(),
            paths: RuntimePaths::default(),
        }
    }
}

fn default_ephemeral() -> bool {
    true
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimePaths {
    #[serde(default)]
    pub base: String,
    #[serde(default)]
    pub run: String,
    #[serde(default)]
    pub tmp: String,
    #[serde(default)]
    pub cache: String,
    #[serde(default)]
    pub config: String,
    #[serde(default)]
    pub data: String,
    #[serde(default)]
    pub home: String,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dependencies {
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub wants: Vec<String>,
    #[serde(default)]
    pub after: Vec<String>,
    #[serde(default)]
    pub before: Vec<String>,
    #[serde(default)]
    pub part_of: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Exec {
    pub program: Option<String>,
    #[serde(default)]
    pub argv: Vec<ExecArg>,
    pub shell_command: Option<String>,
    pub command: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ExecArg {
    String(String),
    Config { config: String },
}

impl Exec {
    pub fn display_command(&self) -> String {
        self.shell_command
            .clone()
            .unwrap_or_else(|| self.command.clone())
    }

    pub fn systemd_argv(
        &self,
        runtime_shell: &str,
        configs: &BTreeMap<String, String>,
    ) -> Result<(String, Vec<String>), String> {
        if let Some(shell_command) = &self.shell_command {
            return Ok((
                runtime_shell.to_string(),
                vec![
                    runtime_shell.to_string(),
                    "-lc".to_string(),
                    shell_command.clone(),
                ],
            ));
        }

        let program = self
            .program
            .clone()
            .ok_or_else(|| "service exec is missing `program`".to_string())?;
        let mut argv = vec![program.clone()];
        for arg in &self.argv {
            match arg {
                ExecArg::String(value) => argv.push(value.clone()),
                ExecArg::Config { config } => {
                    let path = configs.get(config).ok_or_else(|| {
                        format!("service argv references missing config `{config}`")
                    })?;
                    argv.push(path.clone());
                }
            }
        }
        Ok((program, argv))
    }
}
