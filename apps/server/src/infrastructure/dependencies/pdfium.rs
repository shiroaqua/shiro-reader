use anyhow::bail;

use super::{ArchiveFormat, Dependency, DependencyArtifact, TargetPlatform};

/// Matches `pdfium-render = "0.9.5"` (`pdfium_7763`).
pub const DEFAULT_PDFIUM_TAG: &str = "chromium/7763";
pub const DEFAULT_PDFIUM_BASE_URL: &str =
    "https://github.com/bblanchon/pdfium-binaries/releases/download";

#[derive(Debug, Clone)]
pub struct PdfiumDependency {
    base_url: String,
    tag: String,
}

impl PdfiumDependency {
    pub fn new(base_url: impl Into<String>, tag: impl Into<String>) -> Self {
        let base_url = base_url.into().trim_end_matches('/').to_owned();
        let tag = tag.into().trim_matches('/').to_owned();
        Self { base_url, tag }
    }
}

impl Default for PdfiumDependency {
    fn default() -> Self {
        Self::new(DEFAULT_PDFIUM_BASE_URL, DEFAULT_PDFIUM_TAG)
    }
}

impl Dependency for PdfiumDependency {
    fn name(&self) -> &'static str {
        "pdfium"
    }

    fn version(&self) -> &str {
        &self.tag
    }

    fn resolve(&self, platform: TargetPlatform) -> anyhow::Result<DependencyArtifact> {
        let (asset_name, entry_path, file_name) = match (platform.os, platform.arch, platform.env) {
            ("android", "aarch64", _) => (
                "pdfium-android-arm64.tgz",
                "lib/libpdfium.so",
                "libpdfium.so",
            ),
            ("android", "arm", _) => ("pdfium-android-arm.tgz", "lib/libpdfium.so", "libpdfium.so"),
            ("android", "x86_64", _) => {
                ("pdfium-android-x64.tgz", "lib/libpdfium.so", "libpdfium.so")
            }
            ("android", "x86", _) => ("pdfium-android-x86.tgz", "lib/libpdfium.so", "libpdfium.so"),
            ("linux", "x86_64", "musl") => (
                "pdfium-linux-musl-x64.tgz",
                "lib/libpdfium.so",
                "libpdfium.so",
            ),
            ("linux", "aarch64", "musl") => (
                "pdfium-linux-musl-arm64.tgz",
                "lib/libpdfium.so",
                "libpdfium.so",
            ),
            ("linux", "x86", "musl") => (
                "pdfium-linux-musl-x86.tgz",
                "lib/libpdfium.so",
                "libpdfium.so",
            ),
            ("linux", "x86_64", "gnu") => {
                ("pdfium-linux-x64.tgz", "lib/libpdfium.so", "libpdfium.so")
            }
            ("linux", "aarch64", "gnu") => {
                ("pdfium-linux-arm64.tgz", "lib/libpdfium.so", "libpdfium.so")
            }
            ("linux", "arm", "gnu") => ("pdfium-linux-arm.tgz", "lib/libpdfium.so", "libpdfium.so"),
            ("linux", "x86", "gnu") => ("pdfium-linux-x86.tgz", "lib/libpdfium.so", "libpdfium.so"),
            ("macos", "x86_64", _) => (
                "pdfium-mac-x64.tgz",
                "lib/libpdfium.dylib",
                "libpdfium.dylib",
            ),
            ("macos", "aarch64", _) => (
                "pdfium-mac-arm64.tgz",
                "lib/libpdfium.dylib",
                "libpdfium.dylib",
            ),
            ("windows", "x86_64", _) => ("pdfium-win-x64.tgz", "bin/pdfium.dll", "pdfium.dll"),
            ("windows", "aarch64", _) => ("pdfium-win-arm64.tgz", "bin/pdfium.dll", "pdfium.dll"),
            ("windows", "x86", _) => ("pdfium-win-x86.tgz", "bin/pdfium.dll", "pdfium.dll"),
            _ => bail!(
                "unsupported platform for pdfium: os={}, arch={}, env={}",
                platform.os,
                platform.arch,
                platform.env
            ),
        };

        Ok(DependencyArtifact {
            file_name: file_name.to_owned(),
            download_url: format!("{}/{}/{}", self.base_url, self.tag, asset_name),
            archive: ArchiveFormat::TarGz {
                entry_path: entry_path.to_owned(),
            },
            executable_on_unix: true,
        })
    }
}
