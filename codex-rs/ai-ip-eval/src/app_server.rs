use std::collections::BTreeMap;
use std::collections::HashMap;
use std::ffi::CString;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context;
use anyhow::bail;
use codex_ai_ip_domain::HeldOutMissionCase;
use codex_ai_ip_runtime::ADDITIONAL_CONTEXT_KEY;
use codex_ai_ip_runtime::content_package_schema;
use codex_ai_ip_runtime::evaluation_context;
use codex_ai_ip_runtime::root_prompt;
use codex_app_server_protocol::AdditionalContextEntry;
use codex_app_server_protocol::AdditionalContextKind;
use codex_app_server_protocol::AskForApproval;
use codex_app_server_protocol::ClientInfo;
use codex_app_server_protocol::Config;
use codex_app_server_protocol::ConfigLayerSource;
use codex_app_server_protocol::ConfigReadParams;
use codex_app_server_protocol::ConfigReadResponse;
use codex_app_server_protocol::ConfigRequirementsReadResponse;
use codex_app_server_protocol::InitializeCapabilities;
use codex_app_server_protocol::InitializeParams;
use codex_app_server_protocol::InitializeResponse;
use codex_app_server_protocol::JSONRPCError;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::JSONRPCNotification;
use codex_app_server_protocol::JSONRPCRequest;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::UserInput;
use rand::TryRngCore;
use rand::rngs::OsRng;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;
use tokio::fs::File as TokioFile;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWrite;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;

use crate::runner::ChildEnvironment;

pub const EVALUATION_PERMISSION_PROFILE: &str = "ai-ip-eval";
pub const PINNED_EFFECTIVE_CONFIG_BUILT_INS: &[(&str, bool)] = &[("allow_login_shell", true)];
const MAX_JSON_LINE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FrozenSharedConfig {
    pub bytes: Vec<u8>,
    pub layer_json: Value,
}

#[derive(Serialize)]
struct SharedConfig<'a> {
    model: &'a str,
    model_provider: &'static str,
    approval_policy: &'static str,
    approvals_reviewer: &'static str,
    default_permissions: &'static str,
    project_doc_max_bytes: u64,
    model_providers: BTreeMap<&'static str, ProofBrokerProvider>,
    agents: AgentsConfig,
    features: FeaturesConfig,
    permissions: BTreeMap<&'static str, PermissionProfile>,
    shell_environment_policy: ShellEnvironmentPolicy,
    skills: SkillsConfig,
}

#[derive(Serialize)]
struct ProofBrokerProvider {
    name: &'static str,
    base_url: String,
    wire_api: &'static str,
    requires_openai_auth: bool,
    request_max_retries: u64,
    stream_max_retries: u64,
    supports_websockets: bool,
}

#[derive(Serialize)]
struct AgentsConfig {
    enabled: bool,
    max_concurrent_threads_per_session: u64,
}

#[derive(Serialize)]
struct FeaturesConfig {
    guardian_approval: bool,
    multi_agent_v2: EnabledConfig,
    guardianv2: EnabledConfig,
}

#[derive(Serialize)]
struct EnabledConfig {
    enabled: bool,
}

#[derive(Serialize)]
struct PermissionProfile {
    filesystem: BTreeMap<&'static str, &'static str>,
    network: NetworkPermission,
}

#[derive(Serialize)]
struct NetworkPermission {
    enabled: bool,
}

#[derive(Serialize)]
struct ShellEnvironmentPolicy {
    inherit: &'static str,
    ignore_default_excludes: bool,
}

#[derive(Serialize)]
struct SkillsConfig {
    bundled: EnabledConfig,
}

pub fn build_shared_config(model: &str, broker_port: u16) -> anyhow::Result<FrozenSharedConfig> {
    if model.is_empty() || model.contains('\r') || model.contains('\n') {
        bail!("approved model label is empty or contains a newline");
    }
    let filesystem = BTreeMap::from([
        (":minimal", "read"),
        (":workspace_roots", "read"),
        ("~/.codex/skills", "read"),
    ]);
    let config = SharedConfig {
        model,
        model_provider: "ai-ip-proof-broker",
        approval_policy: "never",
        approvals_reviewer: "user",
        default_permissions: EVALUATION_PERMISSION_PROFILE,
        project_doc_max_bytes: 0,
        model_providers: BTreeMap::from([(
            "ai-ip-proof-broker",
            ProofBrokerProvider {
                name: "OpenAI",
                base_url: format!("http://127.0.0.1:{broker_port}/v1"),
                wire_api: "responses",
                requires_openai_auth: false,
                request_max_retries: 0,
                stream_max_retries: 0,
                supports_websockets: false,
            },
        )]),
        agents: AgentsConfig {
            enabled: true,
            max_concurrent_threads_per_session: 4,
        },
        features: FeaturesConfig {
            guardian_approval: false,
            multi_agent_v2: EnabledConfig { enabled: true },
            guardianv2: EnabledConfig { enabled: false },
        },
        permissions: BTreeMap::from([(
            EVALUATION_PERMISSION_PROFILE,
            PermissionProfile {
                filesystem,
                network: NetworkPermission { enabled: false },
            },
        )]),
        shell_environment_policy: ShellEnvironmentPolicy {
            inherit: "core",
            ignore_default_excludes: false,
        },
        skills: SkillsConfig {
            bundled: EnabledConfig { enabled: false },
        },
    };
    let text = toml::to_string(&config).context("serialize frozen shared config")?;
    if text.contains('\r') || !text.ends_with('\n') {
        bail!("shared config serializer did not produce UTF-8/LF bytes");
    }
    let layer: toml::Value = toml::from_str(&text).context("round-trip shared config")?;
    Ok(FrozenSharedConfig {
        bytes: text.into_bytes(),
        layer_json: serde_json::to_value(layer)?,
    })
}

pub struct JsonLineClient<R, W> {
    reader: BufReader<R>,
    writer: W,
    next_id: i64,
    notifications: Vec<ServerNotification>,
    poisoned: Option<String>,
}

impl<R, W> JsonLineClient<R, W>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            reader: BufReader::new(reader),
            writer,
            next_id: 1,
            notifications: Vec::new(),
            poisoned: None,
        }
    }

    pub fn is_poisoned(&self) -> bool {
        self.poisoned.is_some()
    }

    pub fn take_notifications(&mut self) -> Vec<ServerNotification> {
        std::mem::take(&mut self.notifications)
    }

    pub fn observe_queued_notifications(
        &mut self,
        mut observe: impl FnMut(&ServerNotification) -> anyhow::Result<bool>,
    ) -> anyhow::Result<bool> {
        self.ensure_usable()?;
        let mut saw_relevant = false;
        for notification in std::mem::take(&mut self.notifications) {
            match observe(&notification) {
                Ok(relevant) => saw_relevant |= relevant,
                Err(error) => {
                    self.poison(format!("queued notification observer failed: {error:#}"));
                    return Err(error);
                }
            }
        }
        Ok(saw_relevant)
    }

    pub async fn observe_until_quiet(
        &mut self,
        quiet_window: Duration,
        absolute_deadline: Instant,
        mut observe: impl FnMut(&ServerNotification) -> anyhow::Result<bool>,
    ) -> anyhow::Result<()> {
        self.ensure_usable()?;
        if quiet_window.is_zero() {
            bail!("App Server quiet window must be positive");
        }
        self.observe_queued_notifications(&mut observe)?;
        let mut quiet_deadline = Instant::now()
            .checked_add(quiet_window)
            .context("App Server quiet-window deadline overflow")?;
        loop {
            let now = Instant::now();
            let absolute_remaining = absolute_deadline
                .checked_duration_since(now)
                .filter(|remaining| !remaining.is_zero())
                .context("absolute pair deadline expired during quiet observation")?;
            let quiet_remaining = quiet_deadline
                .checked_duration_since(now)
                .unwrap_or(Duration::ZERO);
            if quiet_remaining.is_zero() {
                return Ok(());
            }
            let wait = quiet_remaining.min(absolute_remaining);
            match tokio::time::timeout(wait, self.read_message()).await {
                Err(_) if quiet_remaining <= absolute_remaining => return Ok(()),
                Err(_) => {
                    self.poison("absolute pair deadline expired during quiet observation".into());
                    bail!("absolute pair deadline expired during quiet observation")
                }
                Ok(Err(error)) => return Err(error),
                Ok(Ok(JSONRPCMessage::Notification(notification))) => {
                    let typed = ServerNotification::try_from(notification)
                        .context("deserialize typed App Server notification")?;
                    match observe(&typed) {
                        Ok(true) => {
                            quiet_deadline = Instant::now()
                                .checked_add(quiet_window)
                                .context("App Server quiet-window deadline overflow")?;
                        }
                        Ok(false) => {}
                        Err(error) => {
                            self.poison(format!("quiet notification observer failed: {error:#}"));
                            return Err(error);
                        }
                    }
                }
                Ok(Ok(JSONRPCMessage::Request(request))) => {
                    let error = JSONRPCError {
                        id: request.id,
                        error: JSONRPCErrorError {
                            code: -32601,
                            data: None,
                            message: "evaluation client does not implement server requests"
                                .to_string(),
                        },
                    };
                    self.write_message(&error).await?;
                    self.poison(format!("unknown server request {}", request.method));
                    bail!("unknown App Server request {}", request.method)
                }
                Ok(Ok(JSONRPCMessage::Response(response))) => {
                    self.poison(format!(
                        "unexpected response {} during quiet observation",
                        response.id
                    ));
                    bail!("unexpected App Server response {}", response.id)
                }
                Ok(Ok(JSONRPCMessage::Error(error))) => {
                    self.poison(format!(
                        "unexpected error {} during quiet observation",
                        error.id
                    ));
                    bail!("unexpected App Server error {}", error.id)
                }
            }
        }
    }

    pub async fn shutdown_and_observe_until_eof(
        mut self,
        absolute_deadline: Instant,
        mut observe: impl FnMut(&ServerNotification) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        self.ensure_usable()?;
        for notification in std::mem::take(&mut self.notifications) {
            if let Err(error) = observe(&notification) {
                self.poison(format!("shutdown notification observer failed: {error:#}"));
                return Err(error);
            }
        }
        self.writer
            .shutdown()
            .await
            .context("shutdown App Server stdin")?;
        drop(self.writer);
        loop {
            let remaining = absolute_deadline
                .checked_duration_since(Instant::now())
                .filter(|remaining| !remaining.is_zero())
                .context("absolute pair deadline expired during App Server shutdown")?;
            let mut line = Vec::new();
            let read = match tokio::time::timeout(
                remaining,
                (&mut self.reader)
                    .take((MAX_JSON_LINE_BYTES + 1) as u64)
                    .read_until(b'\n', &mut line),
            )
            .await
            {
                Ok(read) => read.context("read App Server stdout during shutdown")?,
                Err(_) => {
                    bail!("absolute pair deadline expired during App Server shutdown")
                }
            };
            if read == 0 {
                return Ok(());
            }
            if line.len() > MAX_JSON_LINE_BYTES || line.last() != Some(&b'\n') {
                bail!("invalid App Server message framing during shutdown");
            }
            let message: JSONRPCMessage = match serde_json::from_slice(&line) {
                Ok(message) => message,
                Err(error) => return Err(error).context("parse App Server JSON during shutdown"),
            };
            match message {
                JSONRPCMessage::Notification(notification) => {
                    let typed = ServerNotification::try_from(notification)
                        .context("deserialize typed App Server shutdown notification")?;
                    observe(&typed)?;
                }
                JSONRPCMessage::Request(request) => {
                    bail!("server request {} arrived during shutdown", request.method)
                }
                JSONRPCMessage::Response(response) => {
                    bail!("response {} arrived during shutdown", response.id)
                }
                JSONRPCMessage::Error(error) => {
                    bail!("error {} arrived during shutdown", error.id)
                }
            }
        }
    }

    pub async fn notify<P>(&mut self, method: &str, params: Option<&P>) -> anyhow::Result<()>
    where
        P: Serialize + ?Sized,
    {
        self.ensure_usable()?;
        let notification = JSONRPCNotification {
            method: method.to_string(),
            params: params.map(serde_json::to_value).transpose()?,
        };
        self.write_message(&notification).await
    }

    pub async fn request<P, T>(
        &mut self,
        method: &str,
        params: Option<&P>,
        request_timeout: Duration,
    ) -> anyhow::Result<T>
    where
        P: Serialize + ?Sized,
        T: for<'de> Deserialize<'de>,
    {
        self.ensure_usable()?;
        let id = RequestId::Integer(self.next_id);
        self.next_id = self.next_id.checked_add(1).context("request ID overflow")?;
        let request = JSONRPCRequest {
            id: id.clone(),
            method: method.to_string(),
            params: params.map(serde_json::to_value).transpose()?,
            trace: None,
        };
        self.write_message(&request).await?;

        match tokio::time::timeout(request_timeout, self.read_response(&id)).await {
            Ok(result) => result,
            Err(_) => {
                self.poison(format!("request {id} timed out"));
                bail!("App Server request {id} timed out")
            }
        }
    }

    pub async fn wait_for_turn_completion(
        &mut self,
        thread_id: &str,
        turn_id: &str,
        request_timeout: Duration,
        mut observe: impl FnMut(&ServerNotification) -> anyhow::Result<()>,
    ) -> anyhow::Result<codex_app_server_protocol::TurnCompletedNotification> {
        self.ensure_usable()?;
        let mut queued_completion = None;
        for typed in std::mem::take(&mut self.notifications) {
            observe(&typed)?;
            if let ServerNotification::TurnCompleted(completed) = &typed
                && completed.thread_id == thread_id
                && completed.turn.id == turn_id
            {
                if queued_completion.replace(completed.clone()).is_some() {
                    bail!("duplicate queued root turn completion");
                }
            }
        }
        if let Some(completed) = queued_completion {
            return Ok(completed);
        }
        let result = tokio::time::timeout(request_timeout, async {
            loop {
                match self.read_message().await? {
                    JSONRPCMessage::Notification(notification) => {
                        let typed = ServerNotification::try_from(notification)
                            .context("deserialize typed App Server notification")?;
                        if let ServerNotification::TurnCompleted(completed) = &typed
                            && completed.thread_id == thread_id
                            && completed.turn.id == turn_id
                        {
                            observe(&typed)?;
                            return Ok(completed.clone());
                        }
                        observe(&typed)?;
                    }
                    JSONRPCMessage::Request(request) => {
                        let error = JSONRPCError {
                            id: request.id,
                            error: JSONRPCErrorError {
                                code: -32601,
                                data: None,
                                message: "evaluation client does not implement server requests"
                                    .to_string(),
                            },
                        };
                        self.write_message(&error).await?;
                        bail!("unknown App Server request while waiting for turn completion");
                    }
                    JSONRPCMessage::Response(response) => {
                        bail!("unexpected App Server response {}", response.id)
                    }
                    JSONRPCMessage::Error(error) => {
                        bail!("unexpected App Server error {}", error.id)
                    }
                }
            }
        })
        .await;
        match result {
            Ok(result) => result,
            Err(_) => {
                self.poison(format!("turn {turn_id} completion timed out"));
                bail!("App Server turn completion timed out")
            }
        }
    }

    async fn read_response<T>(&mut self, expected_id: &RequestId) -> anyhow::Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        loop {
            let message = self.read_message().await?;
            match message {
                JSONRPCMessage::Response(JSONRPCResponse { id, result }) => {
                    if &id != expected_id {
                        self.poison(format!(
                            "received response {id} while waiting for {expected_id}"
                        ));
                        bail!("unexpected App Server response ID {id}");
                    }
                    return serde_json::from_value(result)
                        .context("deserialize typed App Server response");
                }
                JSONRPCMessage::Error(JSONRPCError { id, error }) => {
                    if &id != expected_id {
                        self.poison(format!(
                            "received error {id} while waiting for {expected_id}"
                        ));
                        bail!("unexpected App Server error ID {id}");
                    }
                    bail!("App Server error {}: {}", error.code, error.message);
                }
                JSONRPCMessage::Notification(notification) => {
                    let typed = ServerNotification::try_from(notification)
                        .context("deserialize typed App Server notification")?;
                    self.notifications.push(typed);
                }
                JSONRPCMessage::Request(request) => {
                    let error = JSONRPCError {
                        id: request.id,
                        error: JSONRPCErrorError {
                            code: -32601,
                            data: None,
                            message: "evaluation client does not implement server requests"
                                .to_string(),
                        },
                    };
                    self.write_message(&error).await?;
                    self.poison(format!("unknown server request {}", request.method));
                    bail!("unknown App Server request {}", request.method);
                }
            }
        }
    }

    async fn read_message(&mut self) -> anyhow::Result<JSONRPCMessage> {
        let mut line = Vec::new();
        let read = (&mut self.reader)
            .take((MAX_JSON_LINE_BYTES + 1) as u64)
            .read_until(b'\n', &mut line)
            .await
            .context("read App Server stdout")?;
        if read == 0 {
            self.poison("App Server stdout reached EOF".to_string());
            bail!("App Server stdout reached EOF");
        }
        if line.len() > MAX_JSON_LINE_BYTES {
            self.poison("App Server JSON line exceeded limit".to_string());
            bail!("App Server JSON line exceeded limit");
        }
        if line.last() != Some(&b'\n') {
            self.poison("App Server emitted a non-newline-terminated message".to_string());
            bail!("App Server message was not newline terminated");
        }
        let message = serde_json::from_slice(&line).context("parse App Server JSON line");
        match message {
            Ok(message) => Ok(message),
            Err(error) => {
                self.poison(format!("invalid App Server JSON: {error:#}"));
                Err(error)
            }
        }
    }

    async fn write_message<T: Serialize + ?Sized>(&mut self, message: &T) -> anyhow::Result<()> {
        let mut bytes = serde_json::to_vec(message)?;
        if bytes.len() >= MAX_JSON_LINE_BYTES {
            bail!("outbound App Server JSON line exceeded limit");
        }
        bytes.push(b'\n');
        self.writer
            .write_all(&bytes)
            .await
            .context("write App Server stdin")?;
        self.writer
            .flush()
            .await
            .context("flush App Server stdin")?;
        Ok(())
    }

    fn ensure_usable(&self) -> anyhow::Result<()> {
        if let Some(reason) = &self.poisoned {
            bail!("App Server client is poisoned: {reason}");
        }
        Ok(())
    }

    fn poison(&mut self, reason: String) {
        if self.poisoned.is_none() {
            self.poisoned = Some(reason);
        }
    }
}

pub struct AppServerHandshake {
    pub initialize: InitializeResponse,
    pub config: ConfigReadResponse,
    pub requirements: ConfigRequirementsReadResponse,
}

pub struct AppServerClient {
    child: DarwinLoadedChild,
    protocol: Option<JsonLineClient<TokioFile, TokioFile>>,
    private_executable: Option<PrivateExecutableImage>,
}

pub(crate) struct VerifiedExecutable {
    read_descriptor: File,
    expected_sha256: String,
}

impl VerifiedExecutable {
    #[cfg(unix)]
    pub(crate) fn from_retained(
        read_descriptor: File,
        executable_path: &Path,
        expected_sha256: String,
    ) -> anyhow::Result<Self> {
        use std::os::unix::fs::MetadataExt;

        let current_descriptor = File::open(executable_path).with_context(|| {
            format!("open {} for retained execution", executable_path.display())
        })?;
        let read_metadata = read_descriptor.metadata()?;
        let current_metadata = current_descriptor.metadata()?;
        if read_metadata.dev() != current_metadata.dev()
            || read_metadata.ino() != current_metadata.ino()
            || read_metadata.len() != current_metadata.len()
        {
            bail!("verified executable descriptors do not identify the same file");
        }
        Ok(Self {
            read_descriptor,
            expected_sha256,
        })
    }

    #[cfg(not(unix))]
    pub(crate) fn from_retained(
        _read_descriptor: File,
        _executable_path: &Path,
        _expected_sha256: String,
    ) -> anyhow::Result<Self> {
        bail!("descriptor-backed executable launch requires Unix semantics")
    }
}

struct PrivateExecutableImage {
    path: PathBuf,
    directory: PathBuf,
    file: File,
    directory_handle: File,
    expected_sha256: String,
    active: bool,
}

impl PrivateExecutableImage {
    #[cfg(target_os = "macos")]
    fn create(verified: &VerifiedExecutable, coordinator_dir: &Path) -> anyhow::Result<Self> {
        use std::os::unix::fs::FileExt;
        use std::os::unix::fs::OpenOptionsExt;
        use std::os::unix::fs::PermissionsExt;

        let root = coordinator_dir.join("private-executable-images");
        match std::fs::create_dir(&root) {
            Ok(()) => std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let metadata = std::fs::symlink_metadata(&root)?;
                if !metadata.file_type().is_dir()
                    || metadata.file_type().is_symlink()
                    || metadata.permissions().mode() & 0o077 != 0
                {
                    bail!("private executable image root is not an owner-only directory");
                }
            }
            Err(error) => return Err(error).context("create private executable image root"),
        }
        let directory = loop {
            let mut random = [0_u8; 16];
            OsRng
                .try_fill_bytes(&mut random)
                .context("generate private executable image name")?;
            let name = random
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            let candidate = root.join(name);
            match std::fs::create_dir(&candidate) {
                Ok(()) => break candidate,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(error).context("create private executable image directory");
                }
            }
        };
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
        let path = directory.join("codex-app-server");
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o700)
            .open(&path)
            .context("create private executable image")?;
        let before = verified.read_descriptor.metadata()?;
        let mut digest = Sha256::new();
        let mut offset = 0_u64;
        loop {
            let mut chunk = [0_u8; 8192];
            let read = verified.read_descriptor.read_at(&mut chunk, offset)?;
            if read == 0 {
                break;
            }
            file.write_all(&chunk[..read])?;
            digest.update(&chunk[..read]);
            offset = offset
                .checked_add(u64::try_from(read)?)
                .context("private executable image length overflow")?;
        }
        let after = verified.read_descriptor.metadata()?;
        if before.len() != offset
            || before.len() != after.len()
            || format!("{:x}", digest.finalize()) != verified.expected_sha256
        {
            bail!("retained executable changed while materializing private image");
        }
        file.flush()?;
        file.sync_all()?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o500))?;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o500))?;
        let directory_handle = File::open(&directory)?;
        unsafe {
            use std::os::fd::AsRawFd;
            if libc::fchflags(file.as_raw_fd(), libc::UF_IMMUTABLE) != 0
                || libc::fchflags(directory_handle.as_raw_fd(), libc::UF_IMMUTABLE) != 0
            {
                return Err(std::io::Error::last_os_error())
                    .context("make private executable image immutable");
            }
        }
        Ok(Self {
            path,
            directory,
            file,
            directory_handle,
            expected_sha256: verified.expected_sha256.clone(),
            active: true,
        })
    }

    #[cfg(not(target_os = "macos"))]
    fn create(_verified: &VerifiedExecutable, _coordinator_dir: &Path) -> anyhow::Result<Self> {
        bail!("atomic private executable image launch is unsupported on this platform")
    }

    fn cleanup(&mut self) -> anyhow::Result<()> {
        if !self.active {
            return Ok(());
        }
        #[cfg(target_os = "macos")]
        unsafe {
            use std::os::fd::AsRawFd;
            if libc::fchflags(self.directory_handle.as_raw_fd(), 0) != 0
                || libc::fchflags(self.file.as_raw_fd(), 0) != 0
            {
                return Err(std::io::Error::last_os_error())
                    .context("clear private executable image immutability");
            }
        }
        #[cfg(target_os = "macos")]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.directory, std::fs::Permissions::from_mode(0o700))?;
        }
        std::fs::remove_file(&self.path)?;
        std::fs::remove_dir(&self.directory)?;
        self.active = false;
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn verify_unchanged(&self) -> anyhow::Result<()> {
        use std::os::fd::AsRawFd;
        use std::os::unix::fs::FileExt;
        use std::os::unix::fs::MetadataExt;

        let path_metadata = std::fs::symlink_metadata(&self.path)?;
        let retained_metadata = self.file.metadata()?;
        let directory_path_metadata = std::fs::symlink_metadata(&self.directory)?;
        let retained_directory_metadata = self.directory_handle.metadata()?;
        if !path_metadata.file_type().is_file()
            || path_metadata.file_type().is_symlink()
            || path_metadata.dev() != retained_metadata.dev()
            || path_metadata.ino() != retained_metadata.ino()
            || path_metadata.len() != retained_metadata.len()
        {
            bail!("private executable image pathname no longer identifies the retained vnode");
        }
        if !directory_path_metadata.file_type().is_dir()
            || directory_path_metadata.file_type().is_symlink()
            || directory_path_metadata.dev() != retained_directory_metadata.dev()
            || directory_path_metadata.ino() != retained_directory_metadata.ino()
        {
            bail!("private executable directory pathname no longer identifies the retained vnode");
        }
        let mut image_stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        let mut directory_stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        if unsafe { libc::fstat(self.file.as_raw_fd(), image_stat.as_mut_ptr()) } != 0
            || unsafe {
                libc::fstat(
                    self.directory_handle.as_raw_fd(),
                    directory_stat.as_mut_ptr(),
                )
            } != 0
        {
            return Err(std::io::Error::last_os_error())
                .context("read private executable immutable flags");
        }
        let image_flags =
            unsafe { std::ptr::addr_of!((*image_stat.as_ptr()).st_flags).read_unaligned() };
        let directory_flags =
            unsafe { std::ptr::addr_of!((*directory_stat.as_ptr()).st_flags).read_unaligned() };
        if image_flags & libc::UF_IMMUTABLE == 0 || directory_flags & libc::UF_IMMUTABLE == 0 {
            bail!("private executable image or directory lost immutable protection");
        }
        let mut digest = Sha256::new();
        let mut offset = 0_u64;
        loop {
            let mut chunk = [0_u8; 8192];
            let read = self.file.read_at(&mut chunk, offset)?;
            if read == 0 {
                break;
            }
            digest.update(&chunk[..read]);
            offset = offset
                .checked_add(u64::try_from(read)?)
                .context("private executable verification length overflow")?;
        }
        if offset != retained_metadata.len()
            || format!("{:x}", digest.finalize()) != self.expected_sha256
        {
            bail!("private executable image bytes differ from the frozen executable");
        }
        Ok(())
    }
}

impl Drop for PrivateExecutableImage {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

#[cfg(target_os = "macos")]
struct DarwinVnodeWatcher {
    queue: File,
}

#[cfg(target_os = "macos")]
impl DarwinVnodeWatcher {
    fn new(image: &PrivateExecutableImage) -> anyhow::Result<Self> {
        use std::os::fd::AsRawFd;
        use std::os::fd::FromRawFd;

        let queue_fd = unsafe { libc::kqueue() };
        if queue_fd < 0 {
            return Err(std::io::Error::last_os_error()).context("create private-image kqueue");
        }
        let queue = unsafe { File::from_raw_fd(queue_fd) };
        let flags = libc::NOTE_WRITE
            | libc::NOTE_DELETE
            | libc::NOTE_EXTEND
            | libc::NOTE_LINK
            | libc::NOTE_RENAME
            | libc::NOTE_REVOKE;
        let changes = [
            libc::kevent {
                ident: image.file.as_raw_fd() as libc::uintptr_t,
                filter: libc::EVFILT_VNODE,
                flags: libc::EV_ADD | libc::EV_ENABLE | libc::EV_CLEAR,
                fflags: flags,
                data: 0,
                udata: std::ptr::null_mut(),
            },
            libc::kevent {
                ident: image.directory_handle.as_raw_fd() as libc::uintptr_t,
                filter: libc::EVFILT_VNODE,
                flags: libc::EV_ADD | libc::EV_ENABLE | libc::EV_CLEAR,
                fflags: flags,
                data: 0,
                udata: std::ptr::null_mut(),
            },
        ];
        let result = unsafe {
            libc::kevent(
                queue.as_raw_fd(),
                changes.as_ptr(),
                i32::try_from(changes.len())?,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
            )
        };
        if result < 0 {
            return Err(std::io::Error::last_os_error())
                .context("watch private executable image vnode");
        }
        Ok(Self { queue })
    }

    fn require_no_mutation(&self) -> anyhow::Result<()> {
        use std::os::fd::AsRawFd;

        let mut event = std::mem::MaybeUninit::<libc::kevent>::uninit();
        let timeout = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        let result = unsafe {
            libc::kevent(
                self.queue.as_raw_fd(),
                std::ptr::null(),
                0,
                event.as_mut_ptr(),
                1,
                &timeout,
            )
        };
        if result < 0 {
            return Err(std::io::Error::last_os_error())
                .context("read private executable image mutation watch");
        }
        if result != 0 {
            let event = unsafe { event.assume_init() };
            let flags = unsafe { std::ptr::addr_of!(event.fflags).read_unaligned() };
            bail!("private executable image or directory mutated during launch: {flags:#x}");
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
struct DarwinLoadedChild {
    pid: libc::pid_t,
    watcher: DarwinVnodeWatcher,
    waited: bool,
}

#[cfg(target_os = "macos")]
impl DarwinLoadedChild {
    fn verify_loaded_image(
        &self,
        private_executable: &PrivateExecutableImage,
    ) -> anyhow::Result<()> {
        use std::os::unix::ffi::OsStringExt;
        use std::os::unix::fs::MetadataExt;

        private_executable.verify_unchanged()?;
        self.watcher.require_no_mutation()?;
        let mut buffer = vec![0_u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
        let length = unsafe {
            libc::proc_pidpath(
                self.pid,
                buffer.as_mut_ptr().cast(),
                u32::try_from(buffer.len())?,
            )
        };
        if length <= 0 {
            return Err(std::io::Error::last_os_error())
                .context("query suspended child loaded executable path");
        }
        buffer.truncate(usize::try_from(length)?);
        if buffer.last() == Some(&0) {
            buffer.pop();
        }
        let loaded_path = PathBuf::from(std::ffi::OsString::from_vec(buffer));
        let loaded_file = File::open(&loaded_path)
            .with_context(|| format!("open loaded image {}", loaded_path.display()))?;
        let loaded_metadata = loaded_file.metadata()?;
        let retained_metadata = private_executable.file.metadata()?;
        if loaded_metadata.dev() != retained_metadata.dev()
            || loaded_metadata.ino() != retained_metadata.ino()
            || loaded_metadata.len() != retained_metadata.len()
        {
            bail!("kernel-loaded executable vnode differs from retained frozen image");
        }
        self.watcher.require_no_mutation()
    }

    fn resume(&self) -> anyhow::Result<()> {
        if unsafe { libc::kill(self.pid, libc::SIGCONT) } != 0 {
            return Err(std::io::Error::last_os_error()).context("resume verified suspended child");
        }
        Ok(())
    }

    fn start_kill(&self) -> anyhow::Result<()> {
        if unsafe { libc::kill(self.pid, libc::SIGKILL) } != 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) {
                return Err(error).context("kill App Server child");
            }
        }
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<ExitStatus> {
        use std::os::unix::process::ExitStatusExt;

        if self.waited {
            bail!("App Server child was already waited");
        }
        let pid = self.pid;
        let raw = tokio::task::spawn_blocking(move || {
            let mut status = 0;
            let waited = unsafe { libc::waitpid(pid, &mut status, 0) };
            if waited != pid {
                return Err(std::io::Error::last_os_error());
            }
            Ok(status)
        })
        .await
        .context("join App Server wait")??;
        self.waited = true;
        Ok(ExitStatus::from_raw(raw))
    }

    fn verify_no_mutation(
        &self,
        private_executable: &PrivateExecutableImage,
    ) -> anyhow::Result<()> {
        private_executable.verify_unchanged()?;
        self.watcher.require_no_mutation()
    }
}

#[cfg(target_os = "macos")]
impl Drop for DarwinLoadedChild {
    fn drop(&mut self) {
        if !self.waited {
            let _ = self.start_kill();
            let mut status = 0;
            unsafe { libc::waitpid(self.pid, &mut status, 0) };
            self.waited = true;
        }
    }
}

#[cfg(not(target_os = "macos"))]
struct DarwinLoadedChild;

#[cfg(not(target_os = "macos"))]
impl DarwinLoadedChild {
    fn start_kill(&self) -> anyhow::Result<()> {
        bail!("Darwin loaded child is unavailable on this platform")
    }

    async fn wait(&mut self) -> anyhow::Result<ExitStatus> {
        bail!("Darwin loaded child is unavailable on this platform")
    }

    fn verify_no_mutation(
        &self,
        _private_executable: &PrivateExecutableImage,
    ) -> anyhow::Result<()> {
        bail!("Darwin loaded child is unavailable on this platform")
    }
}

#[cfg(target_os = "macos")]
struct PosixSpawnFileActions {
    raw: libc::posix_spawn_file_actions_t,
}

#[cfg(target_os = "macos")]
impl PosixSpawnFileActions {
    fn new() -> anyhow::Result<Self> {
        let mut raw = std::ptr::null_mut();
        check_posix_spawn(
            unsafe { libc::posix_spawn_file_actions_init(&mut raw) },
            "initialize posix_spawn file actions",
        )?;
        Ok(Self { raw })
    }

    fn dup2(&mut self, source: libc::c_int, destination: libc::c_int) -> anyhow::Result<()> {
        check_posix_spawn(
            unsafe { libc::posix_spawn_file_actions_adddup2(&mut self.raw, source, destination) },
            "add posix_spawn dup2 action",
        )
    }

    fn close(&mut self, descriptor: libc::c_int) -> anyhow::Result<()> {
        check_posix_spawn(
            unsafe { libc::posix_spawn_file_actions_addclose(&mut self.raw, descriptor) },
            "add posix_spawn close action",
        )
    }

    fn as_ptr(&self) -> *const libc::posix_spawn_file_actions_t {
        &self.raw
    }
}

#[cfg(target_os = "macos")]
impl Drop for PosixSpawnFileActions {
    fn drop(&mut self) {
        unsafe { libc::posix_spawn_file_actions_destroy(&mut self.raw) };
    }
}

#[cfg(target_os = "macos")]
struct PosixSpawnAttributes {
    raw: libc::posix_spawnattr_t,
}

#[cfg(target_os = "macos")]
impl PosixSpawnAttributes {
    fn suspended() -> anyhow::Result<Self> {
        let mut raw = std::ptr::null_mut();
        check_posix_spawn(
            unsafe { libc::posix_spawnattr_init(&mut raw) },
            "initialize posix_spawn attributes",
        )?;
        if let Err(error) = check_posix_spawn(
            unsafe {
                libc::posix_spawnattr_setflags(
                    &mut raw,
                    libc::POSIX_SPAWN_START_SUSPENDED as libc::c_short,
                )
            },
            "request suspended posix_spawn",
        ) {
            unsafe { libc::posix_spawnattr_destroy(&mut raw) };
            return Err(error);
        }
        Ok(Self { raw })
    }

    fn as_ptr(&mut self) -> *const libc::posix_spawnattr_t {
        &self.raw
    }
}

#[cfg(target_os = "macos")]
impl Drop for PosixSpawnAttributes {
    fn drop(&mut self) {
        unsafe { libc::posix_spawnattr_destroy(&mut self.raw) };
    }
}

#[cfg(target_os = "macos")]
fn check_posix_spawn(code: libc::c_int, operation: &str) -> anyhow::Result<()> {
    if code != 0 {
        return Err(std::io::Error::from_raw_os_error(code)).context(operation.to_string());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn darwin_pipe() -> anyhow::Result<(File, File)> {
    use std::os::fd::FromRawFd;

    let mut descriptors = [-1, -1];
    if unsafe { libc::pipe(descriptors.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error()).context("create App Server pipe");
    }
    for descriptor in descriptors {
        if unsafe { libc::fcntl(descriptor, libc::F_SETFD, libc::FD_CLOEXEC) } != 0 {
            let error = std::io::Error::last_os_error();
            unsafe {
                libc::close(descriptors[0]);
                libc::close(descriptors[1]);
            }
            return Err(error).context("make App Server pipe close-on-exec");
        }
    }
    Ok(unsafe {
        (
            File::from_raw_fd(descriptors[0]),
            File::from_raw_fd(descriptors[1]),
        )
    })
}

impl AppServerClient {
    /// Spawns the pinned executable as a suspended Darwin image, verifies the
    /// kernel-loaded vnode, and only then permits it to execute user code.
    pub(crate) async fn spawn_verified(
        codex_binary: &Path,
        verified_executable: VerifiedExecutable,
        stderr_path: &Path,
        environment: &ChildEnvironment,
    ) -> anyhow::Result<Self> {
        Self::spawn_verified_inner(
            codex_binary,
            verified_executable,
            stderr_path,
            environment,
            None,
        )
        .await
    }

    #[cfg(all(test, target_os = "macos"))]
    pub(crate) async fn spawn_verified_with_private_image_hook(
        codex_binary: &Path,
        verified_executable: VerifiedExecutable,
        stderr_path: &Path,
        environment: &ChildEnvironment,
        mut hook: impl FnMut(&Path, &Path) -> anyhow::Result<()>,
    ) -> anyhow::Result<Self> {
        Self::spawn_verified_inner(
            codex_binary,
            verified_executable,
            stderr_path,
            environment,
            Some(&mut hook),
        )
        .await
    }

    #[cfg(target_os = "macos")]
    async fn spawn_verified_inner(
        _codex_binary: &Path,
        verified_executable: VerifiedExecutable,
        stderr_path: &Path,
        environment: &ChildEnvironment,
        mut hook: Option<&mut dyn FnMut(&Path, &Path) -> anyhow::Result<()>>,
    ) -> anyhow::Result<Self> {
        use std::os::fd::AsRawFd;

        let stderr = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(stderr_path)
            .with_context(|| format!("create {}", stderr_path.display()))?;
        let private_root = stderr_path
            .parent()
            .context("App Server stderr path has no coordinator directory")?;
        let mut private_executable =
            PrivateExecutableImage::create(&verified_executable, private_root)?;
        let watcher = DarwinVnodeWatcher::new(&private_executable)?;
        let (stdin_read, stdin_write) = darwin_pipe()?;
        let (stdout_read, stdout_write) = darwin_pipe()?;
        #[cfg(test)]
        let test_fixture = _codex_binary.file_name()
            == Some(std::ffi::OsStr::new("native-mock-app-server-test-harness"));
        #[cfg(not(test))]
        let test_fixture = false;
        let mut environment_values = environment.variables().clone();
        if test_fixture {
            environment_values.insert(
                "AI_IP_NATIVE_APP_SERVER_FIXTURE".to_string(),
                "1".to_string(),
            );
        }
        let arguments = if test_fixture {
            vec![
                private_executable
                    .path
                    .as_os_str()
                    .to_string_lossy()
                    .into_owned(),
                "--exact".to_string(),
                "tests::native_app_server_fixture".to_string(),
                "--nocapture".to_string(),
                "--test-threads=1".to_string(),
            ]
        } else {
            vec![
                private_executable
                    .path
                    .as_os_str()
                    .to_string_lossy()
                    .into_owned(),
                "app-server".to_string(),
                "--listen".to_string(),
                "stdio://".to_string(),
                "--strict-config".to_string(),
            ]
        };
        let path = CString::new(private_executable.path.as_os_str().as_encoded_bytes())?;
        let argument_storage = arguments
            .iter()
            .map(|argument| CString::new(argument.as_bytes()))
            .collect::<Result<Vec<_>, _>>()?;
        let mut argument_pointers = argument_storage
            .iter()
            .map(|argument| argument.as_ptr().cast_mut())
            .collect::<Vec<_>>();
        argument_pointers.push(std::ptr::null_mut());
        let environment_storage = environment_values
            .iter()
            .map(|(name, value)| CString::new(format!("{name}={value}")))
            .collect::<Result<Vec<_>, _>>()?;
        let mut environment_pointers = environment_storage
            .iter()
            .map(|value| value.as_ptr().cast_mut())
            .collect::<Vec<_>>();
        environment_pointers.push(std::ptr::null_mut());

        let mut actions = PosixSpawnFileActions::new()?;
        actions.dup2(stdin_read.as_raw_fd(), libc::STDIN_FILENO)?;
        let protocol_fd = if test_fixture { 3 } else { libc::STDOUT_FILENO };
        actions.dup2(stdout_write.as_raw_fd(), protocol_fd)?;
        actions.dup2(stderr.as_raw_fd(), libc::STDERR_FILENO)?;
        if test_fixture {
            actions.dup2(stderr.as_raw_fd(), libc::STDOUT_FILENO)?;
        }
        for fd in [
            stdin_read.as_raw_fd(),
            stdin_write.as_raw_fd(),
            stdout_read.as_raw_fd(),
            stdout_write.as_raw_fd(),
            stderr.as_raw_fd(),
        ] {
            if ![
                libc::STDIN_FILENO,
                libc::STDOUT_FILENO,
                libc::STDERR_FILENO,
                protocol_fd,
            ]
            .contains(&fd)
            {
                actions.close(fd)?;
            }
        }
        let mut attributes = PosixSpawnAttributes::suspended()?;
        if let Some(hook) = hook.as_mut()
            && let Err(error) = hook(&private_executable.path, &private_executable.directory)
        {
            let cleanup = private_executable.cleanup();
            if let Err(cleanup_error) = cleanup {
                return Err(error).context(format!(
                    "private image hook failed; cleanup also failed: {cleanup_error:#}"
                ));
            }
            return Err(error).context("private image hook failed");
        }
        let mut pid = 0;
        let spawn_result = unsafe {
            libc::posix_spawn(
                &mut pid,
                path.as_ptr(),
                actions.as_ptr(),
                attributes.as_ptr(),
                argument_pointers.as_ptr(),
                environment_pointers.as_ptr(),
            )
        };
        check_posix_spawn(spawn_result, "spawn suspended App Server image")?;
        let mut child = DarwinLoadedChild {
            pid,
            watcher,
            waited: false,
        };
        if let Err(error) = child.verify_loaded_image(&private_executable) {
            let _ = child.start_kill();
            let _ = child.wait().await;
            let cleanup = private_executable.cleanup();
            if let Err(cleanup_error) = cleanup {
                return Err(error).context(format!(
                    "loaded image verification failed; cleanup also failed: {cleanup_error:#}"
                ));
            }
            return Err(error).context("verify kernel-loaded suspended App Server image");
        }
        if let Err(error) = child.resume() {
            let _ = child.start_kill();
            let _ = child.wait().await;
            let cleanup = private_executable.cleanup();
            if let Err(cleanup_error) = cleanup {
                return Err(error).context(format!(
                    "resume failed; cleanup also failed: {cleanup_error:#}"
                ));
            }
            return Err(error);
        }
        drop(stdin_read);
        drop(stdout_write);
        let stdin = TokioFile::from_std(stdin_write);
        let stdout = TokioFile::from_std(stdout_read);
        Ok(Self {
            child,
            protocol: Some(JsonLineClient::new(stdout, stdin)),
            private_executable: Some(private_executable),
        })
    }

    #[cfg(not(target_os = "macos"))]
    async fn spawn_verified_inner(
        _codex_binary: &Path,
        _verified_executable: VerifiedExecutable,
        _stderr_path: &Path,
        _environment: &ChildEnvironment,
        _hook: Option<&mut dyn FnMut(&Path, &Path) -> anyhow::Result<()>>,
    ) -> anyhow::Result<Self> {
        bail!("kernel-loaded executable verification is unsupported on this platform")
    }

    pub async fn handshake(
        &mut self,
        expected_codex_home: &Path,
        canonical_eval_tree: &Path,
        request_timeout: Duration,
    ) -> anyhow::Result<AppServerHandshake> {
        let protocol = self
            .protocol
            .as_mut()
            .context("App Server client is closed")?;
        let initialize: InitializeResponse = protocol
            .request("initialize", Some(&initialize_params()), request_timeout)
            .await?;
        if initialize.codex_home.as_path() != expected_codex_home {
            bail!("App Server codexHome differs from the isolated evaluation Home");
        }
        protocol
            .notify::<serde_json::Value>("initialized", None)
            .await?;
        let config: ConfigReadResponse = protocol
            .request(
                "config/read",
                Some(&config_read_params(canonical_eval_tree)?),
                request_timeout,
            )
            .await?;
        let requirements: ConfigRequirementsReadResponse = protocol
            .request::<serde_json::Value, _>("configRequirements/read", None, request_timeout)
            .await?;
        Ok(AppServerHandshake {
            initialize,
            config,
            requirements,
        })
    }

    pub fn protocol_mut(&mut self) -> anyhow::Result<&mut JsonLineClient<TokioFile, TokioFile>> {
        self.protocol
            .as_mut()
            .context("App Server client is closed")
    }

    pub async fn close(mut self, wait_timeout: Duration) -> anyhow::Result<ExitStatus> {
        self.protocol.take();
        let status = match tokio::time::timeout(wait_timeout, self.child.wait()).await {
            Ok(status) => status.context("wait for App Server exit"),
            Err(_) => {
                let kill = self.child.start_kill().context("kill timed-out App Server");
                let _ = self.child.wait().await;
                let cleanup = self
                    .private_executable
                    .as_mut()
                    .context("private App Server executable image is unavailable")?
                    .cleanup();
                kill?;
                cleanup?;
                bail!("App Server did not exit after stdin EOF and was killed");
            }
        }?;
        let private_executable = self
            .private_executable
            .as_mut()
            .context("private App Server executable image is unavailable")?;
        let verification = self.child.verify_no_mutation(private_executable);
        let cleanup = private_executable.cleanup();
        verification?;
        cleanup?;
        Ok(status)
    }

    pub(crate) async fn close_observing(
        mut self,
        absolute_deadline: Instant,
        observe: impl FnMut(&ServerNotification) -> anyhow::Result<()>,
    ) -> anyhow::Result<ExitStatus> {
        let protocol = self
            .protocol
            .take()
            .context("App Server client is closed")?;
        let observation = protocol
            .shutdown_and_observe_until_eof(absolute_deadline, observe)
            .await;
        if let Err(error) = observation {
            let _ = self.child.start_kill();
            let _ = self.child.wait().await;
            let cleanup = self
                .private_executable
                .as_mut()
                .context("private App Server executable image is unavailable")?
                .cleanup();
            if let Err(cleanup_error) = cleanup {
                return Err(error).context(format!(
                    "App Server shutdown observation failed; cleanup also failed: {cleanup_error:#}"
                ));
            }
            return Err(error);
        }
        let remaining = absolute_deadline
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .context("absolute pair deadline expired waiting for App Server exit")?;
        let status = match tokio::time::timeout(remaining, self.child.wait()).await {
            Ok(status) => status.context("wait for App Server exit"),
            Err(_) => {
                let kill = self.child.start_kill().context("kill timed-out App Server");
                let _ = self.child.wait().await;
                let cleanup = self
                    .private_executable
                    .as_mut()
                    .context("private App Server executable image is unavailable")?
                    .cleanup();
                kill?;
                cleanup?;
                bail!("App Server did not exit before the absolute pair deadline");
            }
        }?;
        let private_executable = self
            .private_executable
            .as_mut()
            .context("private App Server executable image is unavailable")?;
        let verification = self.child.verify_no_mutation(private_executable);
        let cleanup = private_executable.cleanup();
        verification?;
        cleanup?;
        Ok(status)
    }
}

pub fn initialize_params() -> InitializeParams {
    InitializeParams {
        client_info: ClientInfo {
            name: "codex_ai_ip_eval".to_string(),
            title: None,
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        capabilities: Some(InitializeCapabilities {
            experimental_api: true,
            ..Default::default()
        }),
    }
}

pub fn config_read_params(canonical_eval_tree: &Path) -> anyhow::Result<ConfigReadParams> {
    let cwd = canonical_eval_tree
        .to_str()
        .context("non-UTF-8 eval tree")?
        .to_string();
    Ok(ConfigReadParams {
        include_layers: true,
        cwd: Some(cwd),
    })
}

pub fn build_thread_start(
    model: &str,
    model_provider: &str,
    canonical_eval_tree: &Path,
) -> anyhow::Result<ThreadStartParams> {
    let cwd = canonical_eval_tree
        .to_str()
        .context("non-UTF-8 eval tree")?
        .to_string();
    Ok(ThreadStartParams {
        model: Some(model.to_string()),
        model_provider: Some(model_provider.to_string()),
        cwd: Some(cwd),
        approval_policy: Some(AskForApproval::Never),
        permissions: Some(EVALUATION_PERMISSION_PROFILE.to_string()),
        ephemeral: Some(false),
        experimental_raw_events: true,
        ..Default::default()
    })
}

pub fn build_turn_start(
    thread_id: &str,
    mission_case: &HeldOutMissionCase,
) -> anyhow::Result<TurnStartParams> {
    let additional_context = HashMap::from([(
        ADDITIONAL_CONTEXT_KEY.to_string(),
        AdditionalContextEntry {
            value: evaluation_context(mission_case)?,
            kind: AdditionalContextKind::Untrusted,
        },
    )]);
    Ok(TurnStartParams {
        thread_id: thread_id.to_string(),
        input: vec![UserInput::Text {
            text: root_prompt().to_string(),
            text_elements: Vec::new(),
        }],
        additional_context: Some(additional_context),
        output_schema: Some(content_package_schema()?),
        permissions: Some(EVALUATION_PERMISSION_PROFILE.to_string()),
        approval_policy: Some(AskForApproval::Never),
        ..Default::default()
    })
}

pub struct ConfigAuditExpectation {
    pub canonical_config_path: PathBuf,
    pub expected_config_bytes: Vec<u8>,
    pub expected_layer_config: Value,
    pub expected_effective_config: Value,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ConfigAuditEvidence {
    pub effective_config_sha256: String,
    pub config_layers_sha256: String,
}

pub fn audit_config(
    response: &ConfigReadResponse,
    requirements: &ConfigRequirementsReadResponse,
    expectation: &ConfigAuditExpectation,
) -> anyhow::Result<ConfigAuditEvidence> {
    if requirements.requirements.is_some() {
        bail!("config requirements must be absent");
    }
    if expectation.expected_config_bytes.contains(&b'\r') {
        bail!("expected config must use LF line endings");
    }
    reject_forbidden_config_text(&expectation.expected_config_bytes)?;
    let auth_path = expectation
        .canonical_config_path
        .parent()
        .context("config.toml does not have a parent directory")?
        .join("auth.json");
    if auth_path.exists() {
        bail!("auth.json is forbidden in the isolated evaluation CODEX_HOME");
    }
    // The caller supplies these bytes from the retained, descriptor-anchored
    // config commitment. Reopening the pathname here would create a second
    // verification-to-consumption race.
    let disk_bytes = &expectation.expected_config_bytes;
    let disk_toml: toml::Value =
        toml::from_str(std::str::from_utf8(disk_bytes).context("config is not UTF-8")?)
            .context("parse config.toml")?;
    let disk_json = serde_json::to_value(disk_toml)?;
    if disk_json != expectation.expected_layer_config {
        bail!("user layer config differs from config.toml");
    }

    let layers = response
        .layers
        .as_ref()
        .context("config layers are missing")?;
    if layers.len() != 1 {
        bail!("expected exactly one User config layer");
    }
    let layer = &layers[0];
    let ConfigLayerSource::User { file, profile } = &layer.name else {
        bail!("non-User config layer is not allowed in evaluation");
    };
    if profile.is_some() {
        bail!("config profile is not allowed in evaluation");
    }
    if layer.disabled_reason.is_some() {
        bail!("User config layer is disabled");
    }
    if file.as_path() != expectation.canonical_config_path {
        bail!("User config layer path does not match config.toml");
    }
    if layer.config != expectation.expected_layer_config {
        bail!("typed User config layer differs from config.toml");
    }
    if response
        .origins
        .values()
        .any(|origin| origin.name != layer.name || origin.version != layer.version)
    {
        bail!("effective config origin metadata differs from the User layer");
    }

    let effective = serde_json::to_value(&response.config)?;
    if effective != expectation.expected_effective_config {
        bail!("effective config differs from the frozen expected config");
    }
    let mut layers_json = serde_json::to_value(layers)?;
    let mut origins_json = serde_json::to_value(&response.origins)?;
    let canonical_path = expectation
        .canonical_config_path
        .to_str()
        .context("config.toml path is not UTF-8")?;
    normalize_config_evidence_path(&mut layers_json, canonical_path);
    normalize_config_evidence_path(&mut origins_json, canonical_path);
    let normalized_sources = serde_json::json!({
        "layers": layers_json,
        "origins": origins_json
    });
    let canonical_effective = canonicalize_config_evidence(&effective);
    let canonical_sources = canonicalize_config_evidence(&normalized_sources);
    Ok(ConfigAuditEvidence {
        effective_config_sha256: sha256(&serde_json::to_vec(&canonical_effective)?),
        config_layers_sha256: sha256(&serde_json::to_vec(&canonical_sources)?),
    })
}

fn canonicalize_config_evidence(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| (key.clone(), canonicalize_config_evidence(value)))
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect(),
        ),
        Value::Array(values) => {
            Value::Array(values.iter().map(canonicalize_config_evidence).collect())
        }
        value => value.clone(),
    }
}

fn normalize_config_evidence_path(value: &mut Value, canonical_path: &str) {
    match value {
        Value::Object(object) => {
            for value in object.values_mut() {
                normalize_config_evidence_path(value, canonical_path);
            }
        }
        Value::Array(values) => {
            for value in values {
                normalize_config_evidence_path(value, canonical_path);
            }
        }
        Value::String(value) if value == canonical_path => {
            *value = "$CODEX_HOME/config.toml".to_string();
        }
        _ => {}
    }
}

pub fn audit_frozen_config(
    response: &ConfigReadResponse,
    requirements: &ConfigRequirementsReadResponse,
    canonical_config_path: PathBuf,
    expected_config_bytes: Vec<u8>,
    expected_layer_config: Value,
) -> anyhow::Result<ConfigAuditEvidence> {
    let mut expected_effective_config = expected_layer_config.clone();
    let expected = expected_effective_config
        .as_object_mut()
        .context("frozen shared config layer must be an object")?;
    for (name, value) in PINNED_EFFECTIVE_CONFIG_BUILT_INS {
        if expected
            .insert((*name).to_string(), Value::Bool(*value))
            .is_some()
        {
            bail!("pinned built-in {name} must not be supplied by the User layer");
        }
    }
    let typed: Config = serde_json::from_value(expected_effective_config)
        .context("construct typed effective config expectation")?;
    audit_config(
        response,
        requirements,
        &ConfigAuditExpectation {
            canonical_config_path,
            expected_config_bytes,
            expected_layer_config,
            expected_effective_config: serde_json::to_value(typed)?,
        },
    )
}

fn reject_forbidden_config_text(bytes: &[u8]) -> anyhow::Result<()> {
    let lower = std::str::from_utf8(bytes)
        .context("config is not UTF-8")?
        .to_ascii_lowercase();
    for forbidden in [
        "env_key",
        "bearer",
        "headers",
        "[mcp",
        "[plugins",
        "developer_instructions",
        "model_instructions_file",
        "[profile",
        "auth.json",
    ] {
        if lower.contains(forbidden) {
            bail!("config contains forbidden field {forbidden}");
        }
    }
    Ok(())
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
