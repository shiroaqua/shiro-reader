pub mod pdfium;

use std::{
    fs::File,
    io::{self, BufReader, Write},
    path::{Component, Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, bail};
use flate2::read::GzDecoder;
use futures_util::StreamExt;
use reqwest::Client;
use tar::Archive;
use tempfile::{NamedTempFile, TempPath};
use tokio::io::AsyncWriteExt;
use tracing::{debug, info};

use self::pdfium::PdfiumDependency;

pub const LIBS_DIR_NAME: &str = "libs";

const DOWNLOAD_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const DOWNLOAD_READ_TIMEOUT: Duration = Duration::from_secs(60);
const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

pub fn libs_dir(root_directory: &Path) -> PathBuf {
    root_directory.join(LIBS_DIR_NAME)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetPlatform {
    pub os: &'static str,
    pub arch: &'static str,
    pub env: &'static str,
}

impl TargetPlatform {
    pub const fn new(os: &'static str, arch: &'static str, env: &'static str) -> Self {
        Self { os, arch, env }
    }

    pub const fn current() -> Self {
        Self {
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            env: current_target_env(),
        }
    }
}

const fn current_target_env() -> &'static str {
    #[cfg(target_env = "gnu")]
    {
        "gnu"
    }
    #[cfg(target_env = "musl")]
    {
        "musl"
    }
    #[cfg(target_env = "msvc")]
    {
        "msvc"
    }
    #[cfg(not(any(target_env = "gnu", target_env = "musl", target_env = "msvc")))]
    {
        ""
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchiveFormat {
    Raw,
    TarGz { entry_path: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyArtifact {
    pub file_name: String,
    pub download_url: String,
    pub archive: ArchiveFormat,
    pub executable_on_unix: bool,
}

pub trait Dependency: Send + Sync {
    fn name(&self) -> &'static str;
    fn version(&self) -> &str;
    fn resolve(&self, platform: TargetPlatform) -> anyhow::Result<DependencyArtifact>;
}

pub struct DependencyManager {
    libs_dir: PathBuf,
    client: Client,
    dependencies: Vec<Box<dyn Dependency>>,
}

impl DependencyManager {
    pub fn new(libs_dir: PathBuf) -> anyhow::Result<Self> {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(DOWNLOAD_CONNECT_TIMEOUT)
            .read_timeout(DOWNLOAD_READ_TIMEOUT)
            .build()
            .context("failed to build dependency download HTTP client")?;

        Ok(Self::with_client(libs_dir, client))
    }

    pub fn with_client(libs_dir: PathBuf, client: Client) -> Self {
        Self {
            libs_dir,
            client,
            dependencies: Vec::new(),
        }
    }

    pub fn with_default_dependencies(root_directory: &Path) -> anyhow::Result<Self> {
        let mut manager = Self::new(libs_dir(root_directory))?;
        manager.register(PdfiumDependency::default());
        Ok(manager)
    }

    pub fn register(&mut self, dependency: impl Dependency + 'static) -> &mut Self {
        self.dependencies.push(Box::new(dependency));
        self
    }

    pub fn libs_dir(&self) -> &Path {
        &self.libs_dir
    }

    pub async fn ensure_all(&self) -> anyhow::Result<()> {
        self.ensure_all_for(TargetPlatform::current()).await
    }

    pub async fn ensure_all_for(&self, platform: TargetPlatform) -> anyhow::Result<()> {
        tokio::fs::create_dir_all(&self.libs_dir)
            .await
            .with_context(|| {
                format!(
                    "failed to create dependency directory {}",
                    self.libs_dir.display()
                )
            })?;

        for dependency in &self.dependencies {
            self.ensure(dependency.as_ref(), platform).await?;
        }

        Ok(())
    }

    pub async fn ensure(
        &self,
        dependency: &dyn Dependency,
        platform: TargetPlatform,
    ) -> anyhow::Result<PathBuf> {
        let artifact = dependency.resolve(platform).with_context(|| {
            format!(
                "failed to resolve dependency '{}' ({})",
                dependency.name(),
                dependency.version()
            )
        })?;

        validate_target_file_name(&artifact.file_name)?;

        tokio::fs::create_dir_all(&self.libs_dir)
            .await
            .with_context(|| {
                format!(
                    "failed to create dependency directory {}",
                    self.libs_dir.display()
                )
            })?;

        let target_path = self.libs_dir.join(&artifact.file_name);
        if is_installed(&target_path).await? {
            debug!(
                dependency = dependency.name(),
                version = dependency.version(),
                path = %target_path.display(),
                "runtime dependency already installed"
            );
            return Ok(target_path);
        }

        info!(
            dependency = dependency.name(),
            version = dependency.version(),
            url = %artifact.download_url,
            target = %target_path.display(),
            "downloading missing runtime dependency"
        );

        let downloaded = self
            .download_to_temp_file(&artifact.download_url)
            .await
            .with_context(|| {
                format!(
                    "failed to download dependency '{}' ({}) from {}",
                    dependency.name(),
                    dependency.version(),
                    artifact.download_url
                )
            })?;

        let libs_dir = self.libs_dir.clone();
        let install_target = target_path.clone();
        let archive = artifact.archive;
        let executable_on_unix = artifact.executable_on_unix;

        tokio::task::spawn_blocking(move || {
            install_downloaded_artifact(
                downloaded,
                &libs_dir,
                &install_target,
                archive,
                executable_on_unix,
            )
        })
        .await
        .context("dependency installation task failed")?
        .with_context(|| {
            format!(
                "failed to install dependency '{}' ({}) to {}",
                dependency.name(),
                dependency.version(),
                target_path.display()
            )
        })?;

        info!(
            dependency = dependency.name(),
            version = dependency.version(),
            path = %target_path.display(),
            "installed runtime dependency"
        );

        Ok(target_path)
    }

    async fn download_to_temp_file(&self, url: &str) -> anyhow::Result<TempPath> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .context("HTTP request failed")?
            .error_for_status()
            .context("server returned unsuccessful HTTP status")?;

        let libs_dir = self.libs_dir.clone();
        let temp_file = tokio::task::spawn_blocking(move || NamedTempFile::new_in(&libs_dir))
            .await
            .context("failed to spawn temp file creation task")?
            .context("failed to create temporary download file")?;

        let (std_file, temp_path) = temp_file.into_parts();
        let mut async_file = tokio::fs::File::from_std(std_file);
        let mut stream = response.bytes_stream();
        let mut downloaded_bytes: u64 = 0;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("failed while reading response body")?;
            async_file
                .write_all(&chunk)
                .await
                .context("failed to write downloaded bytes to temporary file")?;
            downloaded_bytes += chunk.len() as u64;
        }

        async_file
            .flush()
            .await
            .context("failed to flush temporary download file")?;
        drop(async_file);

        if downloaded_bytes == 0 {
            bail!("downloaded file is empty");
        }

        Ok(temp_path)
    }
}

pub async fn ensure_runtime_dependencies(root_directory: &Path) -> anyhow::Result<()> {
    DependencyManager::with_default_dependencies(root_directory)?
        .ensure_all()
        .await
}

fn validate_target_file_name(file_name: &str) -> anyhow::Result<()> {
    let mut components = Path::new(file_name).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(()),
        _ => bail!("invalid dependency target file name: {file_name:?}"),
    }
}

async fn is_installed(target_path: &Path) -> anyhow::Result<bool> {
    match tokio::fs::metadata(target_path).await {
        Ok(metadata) => {
            if metadata.is_dir() {
                bail!(
                    "dependency target path {} is a directory",
                    target_path.display()
                );
            }
            Ok(metadata.is_file() && metadata.len() > 0)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => {
            Err(error).with_context(|| format!("failed to inspect {}", target_path.display()))
        }
    }
}

fn install_downloaded_artifact(
    downloaded: TempPath,
    libs_dir: &Path,
    target_path: &Path,
    archive: ArchiveFormat,
    executable_on_unix: bool,
) -> anyhow::Result<()> {
    match archive {
        ArchiveFormat::Raw => {
            set_unix_permissions(&downloaded, executable_on_unix)?;
            downloaded
                .persist(target_path)
                .map_err(|error| error.error)
                .with_context(|| {
                    format!(
                        "failed to move downloaded file to {}",
                        target_path.display()
                    )
                })?;
            Ok(())
        }
        ArchiveFormat::TarGz { entry_path } => {
            let expected_path = normalize_archive_path(Path::new(&entry_path));
            let archive_file =
                File::open(&downloaded).context("failed to open downloaded archive")?;
            let decoder = GzDecoder::new(BufReader::new(archive_file));
            let mut archive = Archive::new(decoder);

            let mut extracted_temp: Option<TempPath> = None;

            {
                let entries = archive
                    .entries()
                    .context("failed to read tar entries from gzip archive")?;

                for entry in entries {
                    let mut entry = entry.context("failed to read tar archive entry")?;
                    let path = entry.path().context("failed to read tar entry path")?;
                    if !entry.header().entry_type().is_file() {
                        continue;
                    }
                    if normalize_archive_path(&path) != expected_path {
                        continue;
                    }

                    let expected_size = entry.size();
                    if expected_size == 0 {
                        bail!("archive entry '{}' is empty", entry_path);
                    }

                    let mut output = NamedTempFile::new_in(libs_dir)
                        .context("failed to create temporary extraction file")?;
                    let copied = io::copy(&mut entry, output.as_file_mut())
                        .context("failed to extract target file from archive")?;
                    if copied != expected_size {
                        bail!(
                            "incomplete extraction for '{}': expected {expected_size} bytes, got {copied}",
                            entry_path
                        );
                    }
                    output
                        .as_file_mut()
                        .flush()
                        .context("failed to flush extracted file")?;

                    let (file, temp_path) = output.into_parts();
                    drop(file);
                    set_unix_permissions(&temp_path, executable_on_unix)?;
                    extracted_temp = Some(temp_path);
                    break;
                }
            }

            let extracted_temp = extracted_temp
                .with_context(|| format!("archive did not contain expected file '{entry_path}'"))?;

            // Drain remaining gzip stream so the CRC32 and ISIZE trailer are verified.
            let mut decoder = archive.into_inner();
            io::copy(&mut decoder, &mut io::sink())
                .context("gzip integrity check failed after extraction")?;
            drop(decoder);

            extracted_temp
                .persist(target_path)
                .map_err(|error| error.error)
                .with_context(|| {
                    format!(
                        "failed to persist extracted file to {}",
                        target_path.display()
                    )
                })?;

            Ok(())
        }
    }
}

fn normalize_archive_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => normalized.push(part),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}

#[cfg(unix)]
fn set_unix_permissions(path: &Path, executable: bool) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mode = if executable { 0o755 } else { 0o644 };
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .with_context(|| format!("failed to set permissions on {}", path.display()))
}

#[cfg(not(unix))]
fn set_unix_permissions(_path: &Path, _executable: bool) -> anyhow::Result<()> {
    Ok(())
}
