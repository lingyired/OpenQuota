use std::{
    collections::HashSet,
    fs,
    ops::ControlFlow,
    path::{Path, PathBuf},
    sync::Arc,
};

#[cfg(not(target_os = "macos"))]
use std::{
    io::Read,
    process::Stdio,
    thread,
    time::{Duration, Instant},
};

use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

#[cfg(not(target_os = "macos"))]
use crate::{
    child_process::background_command, providers::credential_store::decode_go_keyring_value,
};

#[cfg(not(target_os = "macos"))]
const GH_KEYRING_SERVICE: &str = "gh:github.com";
#[cfg(not(target_os = "macos"))]
const GH_COMMAND_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
const MAX_TOKEN_BYTES: usize = 4096;

pub(super) struct CopilotToken(Zeroizing<String>);

impl CopilotToken {
    fn new(value: impl Into<String>) -> Option<Self> {
        let value = Zeroizing::new(value.into());
        let trimmed = value.trim();
        if trimmed.is_empty()
            || trimmed.len() > MAX_TOKEN_BYTES
            || trimmed.chars().any(char::is_whitespace)
            || trimmed.chars().any(char::is_control)
        {
            return None;
        }
        if trimmed.len() == value.len() {
            Some(Self(value))
        } else {
            Some(Self(Zeroizing::new(trimmed.to_owned())))
        }
    }

    pub(super) fn as_str(&self) -> &str {
        self.0.as_str()
    }

    fn fingerprint(&self) -> [u8; 32] {
        Sha256::digest(self.as_str().as_bytes()).into()
    }
}

trait TextFileAccess: Send + Sync {
    fn read_text(&self, path: &Path) -> Option<String>;

    #[cfg(target_os = "macos")]
    fn read_text_checked(&self, path: &Path) -> Result<Option<String>, super::CopilotError> {
        Ok(self.read_text(path))
    }
}

#[derive(Default)]
struct LocalTextFiles;

impl TextFileAccess for LocalTextFiles {
    fn read_text(&self, path: &Path) -> Option<String> {
        let metadata = fs::metadata(path).ok()?;
        if !metadata.is_file() || metadata.len() > MAX_CONFIG_BYTES {
            return None;
        }
        fs::read_to_string(path).ok()
    }

    #[cfg(target_os = "macos")]
    fn read_text_checked(&self, path: &Path) -> Result<Option<String>, super::CopilotError> {
        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(super::CopilotError::CredentialRead),
        };
        if !metadata.is_file() {
            return Err(super::CopilotError::CredentialRead);
        }
        if metadata.len() > MAX_CONFIG_BYTES {
            return Err(super::CopilotError::InvalidResponse);
        }
        match fs::read_to_string(path) {
            Ok(text) => Ok(Some(text)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
                Err(super::CopilotError::InvalidResponse)
            }
            Err(_) => Err(super::CopilotError::CredentialRead),
        }
    }
}

#[cfg(any(not(target_os = "macos"), test))]
#[allow(dead_code)]
trait GhTokenCommand: Send + Sync {
    fn token(&self) -> Option<CopilotToken>;
}

#[cfg(not(target_os = "macos"))]
#[derive(Default)]
struct LocalGhTokenCommand;

#[cfg(not(target_os = "macos"))]
impl GhTokenCommand for LocalGhTokenCommand {
    fn token(&self) -> Option<CopilotToken> {
        run_gh_token_command(GH_COMMAND_TIMEOUT)
    }
}

#[cfg(any(not(target_os = "macos"), test))]
#[allow(dead_code)]
trait CredentialAccess: Send + Sync {
    fn read(&self, service: &str, account: &str) -> Option<Vec<u8>>;
    fn read_service(&self, service: &str) -> Option<Vec<u8>>;
}

#[cfg(any(not(target_os = "macos"), test))]
#[derive(Default)]
struct EmptyCredentials;

#[cfg(any(not(target_os = "macos"), test))]
impl CredentialAccess for EmptyCredentials {
    fn read(&self, _service: &str, _account: &str) -> Option<Vec<u8>> {
        None
    }

    fn read_service(&self, _service: &str) -> Option<Vec<u8>> {
        None
    }
}

#[derive(Clone)]
struct AuthPaths {
    editor_configs: Vec<PathBuf>,
    gh_configs: Vec<PathBuf>,
}

impl AuthPaths {
    fn discover() -> Self {
        let home = home_directory();
        let mut editor_directories = Vec::new();
        if let Some(xdg) = environment_path("XDG_CONFIG_HOME") {
            push_unique(&mut editor_directories, xdg.join("github-copilot"));
        }
        push_unique(
            &mut editor_directories,
            home.join(".config").join("github-copilot"),
        );

        let mut editor_configs = Vec::new();
        for directory in editor_directories {
            push_unique(&mut editor_configs, directory.join("apps.json"));
            push_unique(&mut editor_configs, directory.join("hosts.json"));
        }

        let mut gh_configs = Vec::new();
        if let Some(directory) = environment_path("GH_CONFIG_DIR") {
            gh_configs.push(directory.join("hosts.yml"));
        } else {
            if let Some(xdg) = environment_path("XDG_CONFIG_HOME") {
                push_unique(&mut gh_configs, xdg.join("gh").join("hosts.yml"));
            }
            #[cfg(target_os = "windows")]
            if let Some(app_data) = environment_path("APPDATA") {
                push_unique(
                    &mut gh_configs,
                    app_data.join("GitHub CLI").join("hosts.yml"),
                );
            }
            push_unique(
                &mut gh_configs,
                home.join(".config").join("gh").join("hosts.yml"),
            );
        }

        Self {
            editor_configs,
            gh_configs,
        }
    }
}

pub(super) struct CopilotAuthStore {
    paths: AuthPaths,
    files: Arc<dyn TextFileAccess>,
    #[cfg(any(not(target_os = "macos"), test))]
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    gh_command: Arc<dyn GhTokenCommand>,
    #[cfg(any(not(target_os = "macos"), test))]
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    credentials: Arc<dyn CredentialAccess>,
}

impl CopilotAuthStore {
    pub(super) fn new() -> Self {
        Self {
            paths: AuthPaths::discover(),
            files: Arc::new(LocalTextFiles),
            #[cfg(all(not(target_os = "macos"), not(test)))]
            gh_command: Arc::new(LocalGhTokenCommand),
            #[cfg(test)]
            gh_command: Arc::new(NoGhCommand),
            #[cfg(any(not(target_os = "macos"), test))]
            credentials: Arc::new(EmptyCredentials),
        }
    }

    pub(super) fn visit_candidates<B>(
        &self,
        visit: impl FnMut(CopilotToken) -> ControlFlow<B>,
    ) -> Result<Option<B>, super::CopilotError> {
        #[cfg(target_os = "macos")]
        {
            self.visit_macos_candidates(visit)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let mut visit = visit;
            let mut seen = HashSet::new();
            if let Some(result) = self.visit_editor_candidates(&mut seen, &mut visit) {
                return Ok(Some(result));
            }
            Ok(self.visit_github_cli_candidates(&mut seen, &mut visit))
        }
    }

    #[cfg(target_os = "macos")]
    pub(super) fn visit_macos_candidates<B>(
        &self,
        mut visit: impl FnMut(CopilotToken) -> ControlFlow<B>,
    ) -> Result<Option<B>, super::CopilotError> {
        let mut seen = HashSet::new();
        let mut source_error = None;
        for path in &self.paths.editor_configs {
            let text = match self.files.read_text_checked(path) {
                Ok(Some(text)) => text,
                Ok(None) => continue,
                Err(error) => {
                    source_error.get_or_insert(error);
                    continue;
                }
            };
            let candidate = match editor_oauth_token_checked(&text) {
                Ok(candidate) => candidate,
                Err(error) => {
                    source_error.get_or_insert(error);
                    continue;
                }
            };
            if let Some(result) = visit_candidate(candidate, &mut seen, &mut visit) {
                return Ok(Some(result));
            }
        }
        for path in &self.paths.gh_configs {
            let text = match self.files.read_text_checked(path) {
                Ok(Some(text)) => text,
                Ok(None) => continue,
                Err(error) => {
                    source_error.get_or_insert(error);
                    continue;
                }
            };
            let candidate = match yaml_oauth_token_checked(&text) {
                Ok(candidate) => candidate,
                Err(error) => {
                    source_error.get_or_insert(error);
                    continue;
                }
            };
            if let Some(result) = visit_candidate(candidate, &mut seen, &mut visit) {
                return Ok(Some(result));
            }
        }
        if let Some(error) = source_error {
            Err(error)
        } else {
            Ok(None)
        }
    }

    fn visit_file_candidates<B>(
        &self,
        seen: &mut HashSet<[u8; 32]>,
        visit: &mut impl FnMut(CopilotToken) -> ControlFlow<B>,
    ) -> Option<B> {
        if let Some(result) = self.visit_editor_candidates(seen, visit) {
            return Some(result);
        }
        for text in self.gh_config_texts() {
            let candidate = yaml_value(&text, "oauth_token").and_then(CopilotToken::new);
            if let Some(result) = visit_candidate(candidate, seen, visit) {
                return Some(result);
            }
        }
        None
    }

    fn visit_editor_candidates<B>(
        &self,
        seen: &mut HashSet<[u8; 32]>,
        visit: &mut impl FnMut(CopilotToken) -> ControlFlow<B>,
    ) -> Option<B> {
        for path in &self.paths.editor_configs {
            let candidate = self
                .files
                .read_text(path)
                .and_then(|text| editor_oauth_token(&text))
                .and_then(CopilotToken::new);
            if let Some(result) = visit_candidate(candidate, seen, visit) {
                return Some(result);
            }
        }
        None
    }

    #[cfg(not(target_os = "macos"))]
    fn visit_github_cli_candidates<B>(
        &self,
        seen: &mut HashSet<[u8; 32]>,
        visit: &mut impl FnMut(CopilotToken) -> ControlFlow<B>,
    ) -> Option<B> {
        let gh_configs = self.gh_config_texts().collect::<Vec<_>>();
        for text in &gh_configs {
            let candidate = yaml_value(text, "oauth_token").and_then(CopilotToken::new);
            if let Some(result) = visit_candidate(candidate, seen, visit) {
                return Some(result);
            }
        }

        if let Some(result) = visit_candidate(self.gh_command.token(), seen, visit) {
            return Some(result);
        }

        for text in &gh_configs {
            let Some(account) = yaml_value(text, "user") else {
                continue;
            };
            let candidate = self
                .credentials
                .read(GH_KEYRING_SERVICE, &account)
                .and_then(|raw| token_from_keyring(&raw));
            if let Some(result) = visit_candidate(candidate, seen, visit) {
                return Some(result);
            }
        }

        let service_candidate = self
            .credentials
            .read_service(GH_KEYRING_SERVICE)
            .and_then(|raw| token_from_keyring(&raw));
        visit_candidate(service_candidate, seen, visit)
    }

    pub(super) fn visit_detection_candidates<B>(
        &self,
        mut visit: impl FnMut(CopilotToken) -> ControlFlow<B>,
    ) -> Option<B> {
        let mut seen = HashSet::new();
        self.visit_file_candidates(&mut seen, &mut visit)
    }

    #[cfg(test)]
    #[cfg(not(target_os = "macos"))]
    pub(super) fn load(&self) -> Option<CopilotToken> {
        self.visit_candidates(ControlFlow::Break).ok().flatten()
    }

    fn gh_config_texts(&self) -> impl Iterator<Item = String> + '_ {
        self.paths
            .gh_configs
            .iter()
            .filter_map(|path| self.files.read_text(path))
    }

    #[cfg(test)]
    pub(super) fn for_test_token(token: Option<&str>) -> Self {
        match token {
            Some(token) => Self::for_test_tokens(&[token]),
            None => Self::for_test_tokens(&[]),
        }
    }

    #[cfg(test)]
    pub(super) fn for_test_gh_token(token: &str) -> Self {
        let path = PathBuf::from("hosts.yml");
        let files = MemoryFiles::from_pairs(vec![(
            path.clone(),
            format!("github.com:\n    oauth_token: {token}\n"),
        )]);
        Self {
            paths: AuthPaths {
                editor_configs: Vec::new(),
                gh_configs: vec![path],
            },
            files: Arc::new(files),
            gh_command: Arc::new(NoGhCommand),
            credentials: Arc::new(NoCredentials),
        }
    }

    #[cfg(test)]
    pub(super) fn for_test_tokens(tokens: &[&str]) -> Self {
        let values = tokens
            .iter()
            .enumerate()
            .map(|(index, token)| {
                let path = PathBuf::from(format!("editor-apps-{index}.json"));
                (
                    path,
                    format!(r#"{{"github.com":{{"oauth_token":"{token}"}}}}"#),
                )
            })
            .collect::<Vec<_>>();
        let editor_configs = values.iter().map(|(path, _)| path.clone()).collect();
        let files = MemoryFiles::from_pairs(values);
        Self {
            paths: AuthPaths {
                editor_configs,
                gh_configs: Vec::new(),
            },
            files: Arc::new(files),
            gh_command: Arc::new(NoGhCommand),
            credentials: Arc::new(NoCredentials),
        }
    }
}

fn visit_candidate<B>(
    candidate: Option<CopilotToken>,
    seen: &mut HashSet<[u8; 32]>,
    visit: &mut impl FnMut(CopilotToken) -> ControlFlow<B>,
) -> Option<B> {
    let candidate = candidate?;
    if !seen.insert(candidate.fingerprint()) {
        return None;
    }
    match visit(candidate) {
        ControlFlow::Break(result) => Some(result),
        ControlFlow::Continue(()) => None,
    }
}

impl Default for CopilotAuthStore {
    fn default() -> Self {
        Self::new()
    }
}

fn editor_oauth_token(text: &str) -> Option<String> {
    let object = serde_json::from_str::<serde_json::Value>(text)
        .ok()?
        .as_object()?
        .clone();
    object.into_iter().find_map(|(host, value)| {
        if host != "github.com" && !host.starts_with("github.com:") {
            return None;
        }
        value
            .get("oauth_token")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    })
}

#[cfg(target_os = "macos")]
fn editor_oauth_token_checked(text: &str) -> Result<Option<CopilotToken>, super::CopilotError> {
    let document = serde_json::from_str::<serde_json::Value>(text)
        .map_err(|_| super::CopilotError::InvalidResponse)?;
    let object = document
        .as_object()
        .ok_or(super::CopilotError::InvalidResponse)?;
    for (host, value) in object {
        if host != "github.com" && !host.starts_with("github.com:") {
            continue;
        }
        let Some(token_value) = value.get("oauth_token") else {
            continue;
        };
        let token = token_value
            .as_str()
            .ok_or(super::CopilotError::InvalidResponse)?;
        return CopilotToken::new(token.to_owned())
            .map(Some)
            .ok_or(super::CopilotError::InvalidResponse);
    }
    Ok(None)
}

#[cfg(target_os = "macos")]
fn yaml_oauth_token_checked(text: &str) -> Result<Option<CopilotToken>, super::CopilotError> {
    let Some(token) = yaml_value(text, "oauth_token") else {
        return Ok(None);
    };
    CopilotToken::new(token)
        .map(Some)
        .ok_or(super::CopilotError::InvalidResponse)
}

fn yaml_value(text: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    let mut in_github = false;
    for line in text.lines() {
        if line
            .chars()
            .next()
            .is_some_and(|character| !character.is_whitespace())
        {
            in_github = line.trim() == "github.com:";
            continue;
        }
        if !in_github {
            continue;
        }
        let trimmed = line.trim();
        let Some(raw) = trimmed.strip_prefix(&prefix) else {
            continue;
        };
        let raw = raw.trim();
        let raw = if raw.len() >= 2
            && ((raw.starts_with('"') && raw.ends_with('"'))
                || (raw.starts_with('\'') && raw.ends_with('\'')))
        {
            &raw[1..raw.len() - 1]
        } else {
            raw
        };
        let value = raw.trim();
        if !value.is_empty() {
            return Some(value.to_owned());
        }
    }
    None
}

#[cfg(not(target_os = "macos"))]
fn token_from_keyring(raw: &[u8]) -> Option<CopilotToken> {
    let text = std::str::from_utf8(raw).ok()?.trim();
    if text.starts_with("go-keyring-base64:") {
        decode_go_keyring_value(raw).and_then(CopilotToken::new)
    } else {
        CopilotToken::new(text)
    }
}

#[cfg(not(target_os = "macos"))]
fn run_gh_token_command(timeout: Duration) -> Option<CopilotToken> {
    let mut child = background_command("gh");
    child
        .args(["auth", "token", "--hostname", "github.com"])
        .env("GH_PROMPT_DISABLED", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = child.spawn().ok()?;
    let stdout = child.stdout.take()?;
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout
            .take((MAX_TOKEN_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        let _ = sender.send(result);
    });

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < timeout => {
                thread::sleep(Duration::from_millis(10));
            }
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    if !status.success() {
        return None;
    }
    let bytes = receiver
        .recv_timeout(Duration::from_millis(100))
        .ok()?
        .ok()?;
    if bytes.len() > MAX_TOKEN_BYTES {
        return None;
    }
    let bytes = Zeroizing::new(bytes);
    let token = std::str::from_utf8(bytes.as_slice()).ok()?;
    CopilotToken::new(token)
}

fn environment_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn home_directory() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn push_unique(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.contains(&path) {
        paths.push(path);
    }
}

#[cfg(test)]
#[derive(Default)]
struct MemoryFiles(std::collections::HashMap<PathBuf, String>);

#[cfg(test)]
impl MemoryFiles {
    fn from_pairs(values: Vec<(PathBuf, String)>) -> Self {
        Self(values.into_iter().collect())
    }
}

#[cfg(test)]
impl TextFileAccess for MemoryFiles {
    fn read_text(&self, path: &Path) -> Option<String> {
        self.0.get(path).cloned()
    }
}

#[cfg(test)]
struct NoGhCommand;

#[cfg(test)]
impl GhTokenCommand for NoGhCommand {
    fn token(&self) -> Option<CopilotToken> {
        None
    }
}

#[cfg(test)]
struct NoCredentials;

#[cfg(test)]
impl CredentialAccess for NoCredentials {
    fn read(&self, _service: &str, _account: &str) -> Option<Vec<u8>> {
        None
    }

    fn read_service(&self, _service: &str) -> Option<Vec<u8>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        fs,
        path::PathBuf,
        sync::{Arc, Mutex},
        time::Duration,
    };

    use tempfile::tempdir;

    #[cfg(target_os = "macos")]
    use crate::providers::UsageProvider;

    #[cfg(not(target_os = "macos"))]
    use base64::{engine::general_purpose::STANDARD, Engine};

    #[cfg(not(target_os = "macos"))]
    use super::token_from_keyring;
    use super::{
        editor_oauth_token, yaml_value, AuthPaths, CopilotAuthStore, CopilotToken,
        CredentialAccess, GhTokenCommand, LocalTextFiles, MemoryFiles, NoCredentials, NoGhCommand,
    };

    #[cfg(target_os = "macos")]
    fn refresh_error_for_local_paths(
        editor_configs: Vec<PathBuf>,
        gh_configs: Vec<PathBuf>,
    ) -> crate::models::ProviderErrorKind {
        let auth = CopilotAuthStore {
            paths: AuthPaths {
                editor_configs,
                gh_configs,
            },
            files: Arc::new(LocalTextFiles),
            gh_command: Arc::new(NoGhCommand),
            credentials: Arc::new(NoCredentials),
        };
        let provider = super::super::CopilotProvider::with_dependencies(
            auth,
            super::super::client::CopilotClient::for_test(
                "http://127.0.0.1:1",
                "http://127.0.0.1:1",
                "http://127.0.0.1:1/",
                Duration::from_millis(100),
            ),
        );
        provider.refresh().unwrap_err().kind()
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_configured_copilot_file_failures_are_distinct_from_missing_credentials() {
        use crate::models::ProviderErrorKind as Kind;

        let directory = tempdir().unwrap();
        let missing = directory.path().join("missing-apps.json");
        let unreadable = directory.path().join("unreadable-apps.json");
        fs::create_dir(&unreadable).unwrap();
        let malformed = directory.path().join("malformed-apps.json");
        fs::write(&malformed, b"{broken").unwrap();
        let invalid_token = directory.path().join("invalid-token-apps.json");
        fs::write(
            &invalid_token,
            br#"{"github.com":{"oauth_token":"token with spaces"}}"#,
        )
        .unwrap();
        let unavailable = directory.path().join("empty-apps.json");
        fs::write(&unavailable, br#"{"github.com":{}}"#).unwrap();
        let invalid_hosts_token = directory.path().join("invalid-hosts.yml");
        fs::write(
            &invalid_hosts_token,
            "github.com:\n    oauth_token: \"token with spaces\"\n",
        )
        .unwrap();

        assert_eq!(
            refresh_error_for_local_paths(vec![missing], vec![]),
            Kind::CredentialsUnavailable
        );
        assert_eq!(
            refresh_error_for_local_paths(vec![unavailable], vec![]),
            Kind::CredentialsUnavailable
        );
        assert_eq!(
            refresh_error_for_local_paths(vec![unreadable], vec![]),
            Kind::CredentialStorage
        );
        assert_eq!(
            refresh_error_for_local_paths(vec![malformed], vec![]),
            Kind::InvalidResponse
        );
        assert_eq!(
            refresh_error_for_local_paths(vec![invalid_token], vec![]),
            Kind::InvalidResponse
        );
        assert_eq!(
            refresh_error_for_local_paths(vec![], vec![invalid_hosts_token]),
            Kind::InvalidResponse
        );
    }

    #[cfg_attr(target_os = "macos", allow(dead_code))]
    enum FakeGhResult {
        Token(String),
        Failed,
        TimedOut,
    }

    #[cfg_attr(target_os = "macos", allow(dead_code))]
    struct FakeGh {
        result: FakeGhResult,
        calls: Mutex<usize>,
    }

    impl GhTokenCommand for FakeGh {
        fn token(&self) -> Option<CopilotToken> {
            *self.calls.lock().unwrap() += 1;
            match &self.result {
                FakeGhResult::Token(token) => CopilotToken::new(token.clone()),
                FakeGhResult::Failed | FakeGhResult::TimedOut => None,
            }
        }
    }

    #[cfg_attr(target_os = "macos", allow(dead_code))]
    struct FakeCredentials {
        values: HashMap<(String, String), Vec<u8>>,
        service_values: HashMap<String, Vec<u8>>,
        calls: Mutex<usize>,
        service_calls: Mutex<usize>,
    }

    impl CredentialAccess for FakeCredentials {
        fn read(&self, service: &str, account: &str) -> Option<Vec<u8>> {
            *self.calls.lock().unwrap() += 1;
            self.values
                .get(&(service.to_owned(), account.to_owned()))
                .cloned()
        }

        fn read_service(&self, service: &str) -> Option<Vec<u8>> {
            *self.service_calls.lock().unwrap() += 1;
            self.service_values.get(service).cloned()
        }
    }

    fn store(
        editor: &[(&str, &str)],
        gh: &[(&str, &str)],
        command: Arc<FakeGh>,
        credentials: Arc<FakeCredentials>,
    ) -> CopilotAuthStore {
        let editor_paths = editor
            .iter()
            .map(|(path, _)| PathBuf::from(path))
            .collect::<Vec<_>>();
        let gh_paths = gh
            .iter()
            .map(|(path, _)| PathBuf::from(path))
            .collect::<Vec<_>>();
        let files = editor
            .iter()
            .chain(gh)
            .map(|(path, text)| (PathBuf::from(path), (*text).to_owned()))
            .collect::<Vec<_>>();
        CopilotAuthStore {
            paths: AuthPaths {
                editor_configs: editor_paths,
                gh_configs: gh_paths,
            },
            files: Arc::new(MemoryFiles::from_pairs(files)),
            gh_command: command,
            credentials,
        }
    }

    fn command(value: Option<&str>) -> Arc<FakeGh> {
        Arc::new(FakeGh {
            result: value
                .map(|value| FakeGhResult::Token(value.to_owned()))
                .unwrap_or(FakeGhResult::Failed),
            calls: Mutex::new(0),
        })
    }

    fn credentials(value: Option<(&str, &[u8])>) -> Arc<FakeCredentials> {
        Arc::new(FakeCredentials {
            values: value
                .map(|(account, value)| {
                    HashMap::from([(
                        ("gh:github.com".to_owned(), account.to_owned()),
                        value.to_vec(),
                    )])
                })
                .unwrap_or_default(),
            service_values: HashMap::new(),
            calls: Mutex::new(0),
            service_calls: Mutex::new(0),
        })
    }

    #[cfg(not(target_os = "macos"))]
    fn service_credentials(value: Option<&[u8]>) -> Arc<FakeCredentials> {
        Arc::new(FakeCredentials {
            values: HashMap::new(),
            service_values: value
                .map(|value| HashMap::from([("gh:github.com".to_owned(), value.to_vec())]))
                .unwrap_or_default(),
            calls: Mutex::new(0),
            service_calls: Mutex::new(0),
        })
    }

    #[test]
    fn editor_parser_uses_only_github_dot_com_entries() {
        assert_eq!(
            editor_oauth_token(
                r#"{"ghe.example:app":{"oauth_token":"enterprise"},
                    "github.com:app":{"oauth_token":"dotcom"}}"#
            )
            .as_deref(),
            Some("dotcom")
        );
        assert_eq!(
            editor_oauth_token(r#"{"ghe.example":{"oauth_token":"enterprise"}}"#),
            None
        );
        assert_eq!(editor_oauth_token("{broken"), None);
    }

    #[test]
    fn yaml_parser_is_scoped_to_github_dot_com_and_ignores_nested_users() {
        let text = r#"
ghe.example:
    oauth_token: enterprise
github.com:
    users:
        octocat:
    user: "octocat"
    oauth_token: 'dotcom'
"#;
        assert_eq!(yaml_value(text, "oauth_token").as_deref(), Some("dotcom"));
        assert_eq!(yaml_value(text, "user").as_deref(), Some("octocat"));
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn source_precedence_is_editor_then_gh_config_then_command_then_keyring() {
        let gh = command(Some("command-token"));
        let vault = credentials(Some(("octocat", b"vault-token")));
        let auth = store(
            &[(
                "apps.json",
                r#"{"github.com":{"oauth_token":"editor-token"}}"#,
            )],
            &[(
                "hosts.yml",
                "github.com:\n    user: octocat\n    oauth_token: config-token\n",
            )],
            gh.clone(),
            vault.clone(),
        );
        assert_eq!(auth.load().unwrap().as_str(), "editor-token");
        assert_eq!(*gh.calls.lock().unwrap(), 0);
        assert_eq!(*vault.calls.lock().unwrap(), 0);

        let gh = command(Some("command-token"));
        let auth = store(
            &[("apps.json", "{}")],
            &[(
                "hosts.yml",
                "github.com:\n    user: octocat\n    oauth_token: config-token\n",
            )],
            gh.clone(),
            vault,
        );
        assert_eq!(auth.load().unwrap().as_str(), "config-token");
        assert_eq!(*gh.calls.lock().unwrap(), 0);

        let gh = command(Some("command-token"));
        let vault = credentials(Some(("octocat", b"vault-token")));
        let auth = store(
            &[],
            &[("hosts.yml", "github.com:\n    user: octocat\n")],
            gh.clone(),
            vault.clone(),
        );
        assert_eq!(auth.load().unwrap().as_str(), "command-token");
        assert_eq!(*gh.calls.lock().unwrap(), 1);
        assert_eq!(*vault.calls.lock().unwrap(), 0);
    }

    #[test]
    fn detection_checks_local_files_without_command_or_credential_lookups() {
        let gh = command(Some("command-token"));
        let vault = credentials(Some(("octocat", b"vault-token")));
        let auth = store(
            &[(
                "apps.json",
                r#"{"github.com":{"oauth_token":"editor-token"}}"#,
            )],
            &[("hosts.yml", "github.com:\n    user: octocat\n")],
            gh.clone(),
            vault.clone(),
        );
        let mut candidates = Vec::new();

        let completed: Option<()> = auth.visit_detection_candidates(|token| {
            candidates.push(token.as_str().to_owned());
            std::ops::ControlFlow::Continue(())
        });

        assert!(completed.is_none());
        assert_eq!(candidates, ["editor-token"]);
        assert_eq!(*gh.calls.lock().unwrap(), 0);
        assert_eq!(*vault.calls.lock().unwrap(), 0);
        assert_eq!(*vault.service_calls.lock().unwrap(), 0);
    }

    #[test]
    fn detection_includes_direct_hosts_yaml_tokens() {
        let gh = command(Some("command-token"));
        let vault = credentials(Some(("octocat", b"vault-token")));
        let auth = store(
            &[],
            &[("hosts.yml", "github.com:\n    oauth_token: config-token\n")],
            gh.clone(),
            vault.clone(),
        );
        let mut candidates = Vec::new();

        let completed: Option<()> = auth.visit_detection_candidates(|token| {
            candidates.push(token.as_str().to_owned());
            std::ops::ControlFlow::Continue(())
        });

        assert!(completed.is_none());
        assert_eq!(candidates, ["config-token"]);
        assert_eq!(*gh.calls.lock().unwrap(), 0);
        assert_eq!(*vault.calls.lock().unwrap(), 0);
        assert_eq!(*vault.service_calls.lock().unwrap(), 0);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn unavailable_runtime_refresh_does_not_query_external_credentials() {
        use crate::providers::copilot::{client::CopilotClient, CopilotProvider, UsageProvider};

        let gh = command(Some("command-token"));
        let vault = credentials(Some(("octocat", b"vault-token")));
        let auth = store(
            &[],
            &[("hosts.yml", "github.com:\n    user: octocat\n")],
            gh.clone(),
            vault.clone(),
        );
        let provider = CopilotProvider::with_dependencies(
            auth,
            CopilotClient::for_test(
                "http://127.0.0.1:1",
                "http://127.0.0.1:1",
                "http://127.0.0.1:1/",
                std::time::Duration::from_millis(100),
            ),
        );

        let error = provider.refresh().unwrap_err();
        assert_eq!(
            error.kind(),
            crate::models::ProviderErrorKind::CredentialsUnavailable
        );
        assert_eq!(*gh.calls.lock().unwrap(), 0);
        assert_eq!(*vault.calls.lock().unwrap(), 0);
        assert_eq!(*vault.service_calls.lock().unwrap(), 0);
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn failed_or_timed_out_gh_fallback_can_use_the_scoped_system_credential() {
        for result in [FakeGhResult::Failed, FakeGhResult::TimedOut] {
            let gh = Arc::new(FakeGh {
                result,
                calls: Mutex::new(0),
            });
            let wrapped = format!("go-keyring-base64:{}", STANDARD.encode("vault-token"));
            let vault = credentials(Some(("octocat", wrapped.as_bytes())));
            let auth = store(
                &[],
                &[("hosts.yml", "github.com:\n    user: octocat\n")],
                gh.clone(),
                vault.clone(),
            );

            assert_eq!(auth.load().unwrap().as_str(), "vault-token");
            assert_eq!(*gh.calls.lock().unwrap(), 1);
            assert_eq!(*vault.calls.lock().unwrap(), 1);
        }
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn service_scoped_keyring_fallback_works_without_a_github_username() {
        let vault = service_credentials(Some(b"vault-token"));
        let auth = store(&[], &[], command(None), vault.clone());

        assert_eq!(auth.load().unwrap().as_str(), "vault-token");
        assert_eq!(*vault.calls.lock().unwrap(), 0);
        assert_eq!(*vault.service_calls.lock().unwrap(), 1);
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn candidates_keep_source_order_and_deduplicate_token_values() {
        let gh = command(Some("shared-token"));
        let vault = service_credentials(Some(b"vault-token"));
        let auth = store(
            &[
                (
                    "apps.json",
                    r#"{"github.com":{"oauth_token":"editor-token"}}"#,
                ),
                (
                    "hosts.json",
                    r#"{"github.com":{"oauth_token":"shared-token"}}"#,
                ),
            ],
            &[("hosts.yml", "github.com:\n    oauth_token: config-token\n")],
            gh,
            vault,
        );
        let mut candidates = Vec::new();
        let completed: Option<()> = auth
            .visit_candidates(|token| {
                candidates.push(token.as_str().to_owned());
                std::ops::ControlFlow::Continue(())
            })
            .unwrap();

        assert!(completed.is_none());
        assert_eq!(
            candidates,
            [
                "editor-token",
                "shared-token",
                "config-token",
                "vault-token"
            ]
        );
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn wrapped_plain_and_invalid_tokens_are_handled_without_exposing_them() {
        let wrapped = format!("go-keyring-base64:{}", STANDARD.encode("wrapped-token"));
        assert_eq!(
            token_from_keyring(wrapped.as_bytes()).unwrap().as_str(),
            "wrapped-token"
        );
        assert_eq!(
            token_from_keyring(b" plain-token ").unwrap().as_str(),
            "plain-token"
        );
        assert!(token_from_keyring(b"go-keyring-base64:not-base64").is_none());
        assert!(CopilotToken::new("line1\nline2").is_none());
    }
}
