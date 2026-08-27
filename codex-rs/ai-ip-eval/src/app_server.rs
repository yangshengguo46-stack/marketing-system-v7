use std::collections::BTreeMap;
use std::collections::HashMap;
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
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWrite;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::process::Child;
use tokio::process::ChildStdin;
use tokio::process::ChildStdout;
use tokio::process::Command;

use crate::runner::ChildEnvironment;

pub const EVALUATION_PERMISSION_PROFILE: &str = "ai-ip-eval";
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
        for typed in std::mem::take(&mut self.notifications) {
            if let ServerNotification::TurnCompleted(completed) = &typed
                && completed.thread_id == thread_id
                && completed.turn.id == turn_id
            {
                return Ok(completed.clone());
            }
            observe(&typed)?;
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
    child: Child,
    protocol: Option<JsonLineClient<ChildStdout, ChildStdin>>,
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
}

impl Drop for PrivateExecutableImage {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

impl AppServerClient {
    /// Spawns the pinned executable through the retained verified descriptor.
    pub(crate) async fn spawn_verified(
        codex_binary: &Path,
        verified_executable: VerifiedExecutable,
        stderr_path: &Path,
        environment: &ChildEnvironment,
    ) -> anyhow::Result<Self> {
        let stderr = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(stderr_path)
            .with_context(|| format!("create {}", stderr_path.display()))?;
        let private_root = stderr_path
            .parent()
            .context("App Server stderr path has no coordinator directory")?;
        let private_executable =
            PrivateExecutableImage::create(&verified_executable, private_root)?;
        let mut command = Command::new(&private_executable.path);
        environment.apply_tokio(&mut command);
        let mut child = command
            .arg("app-server")
            .arg("--listen")
            .arg("stdio://")
            .arg("--strict-config")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(stderr)
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("spawn {} app-server", codex_binary.display()))?;
        let stdin = child
            .stdin
            .take()
            .context("App Server stdin was not piped")?;
        let stdout = child
            .stdout
            .take()
            .context("App Server stdout was not piped")?;
        Ok(Self {
            child,
            protocol: Some(JsonLineClient::new(stdout, stdin)),
            private_executable: Some(private_executable),
        })
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

    pub fn protocol_mut(&mut self) -> anyhow::Result<&mut JsonLineClient<ChildStdout, ChildStdin>> {
        self.protocol
            .as_mut()
            .context("App Server client is closed")
    }

    pub async fn close(mut self, wait_timeout: Duration) -> anyhow::Result<ExitStatus> {
        self.protocol.take();
        let status = match tokio::time::timeout(wait_timeout, self.child.wait()).await {
            Ok(status) => status.context("wait for App Server exit"),
            Err(_) => {
                self.child
                    .start_kill()
                    .context("kill timed-out App Server")?;
                let _ = self.child.wait().await;
                bail!("App Server did not exit after stdin EOF and was killed");
            }
        }?;
        self.private_executable
            .as_mut()
            .context("private App Server executable image is unavailable")?
            .cleanup()?;
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
        protocol
            .shutdown_and_observe_until_eof(absolute_deadline, observe)
            .await?;
        let remaining = absolute_deadline
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .context("absolute pair deadline expired waiting for App Server exit")?;
        let status = match tokio::time::timeout(remaining, self.child.wait()).await {
            Ok(status) => status.context("wait for App Server exit"),
            Err(_) => {
                self.child
                    .start_kill()
                    .context("kill timed-out App Server")?;
                let _ = self.child.wait().await;
                bail!("App Server did not exit before the absolute pair deadline");
            }
        }?;
        self.private_executable
            .as_mut()
            .context("private App Server executable image is unavailable")?
            .cleanup()?;
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
    let layers_json = serde_json::to_value(layers)?;
    Ok(ConfigAuditEvidence {
        effective_config_sha256: sha256(&serde_json::to_vec(&effective)?),
        config_layers_sha256: sha256(&serde_json::to_vec(&layers_json)?),
    })
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
