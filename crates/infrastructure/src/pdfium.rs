use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

use anyhow::Context;
use pdfium_render::prelude::{Pdfium, PdfiumError};
use tracing::{debug, info, warn};

static BUNDLED_PDFIUM: OnceLock<Option<Arc<Mutex<Pdfium>>>> = OnceLock::new();
static SERVER_PDFIUM: Mutex<Option<Arc<Mutex<Pdfium>>>> = Mutex::new(None);

/// Loads the client's bundled PDFium library once per process.
///
/// Release builds strictly load the bundled library only — no runtime network download,
/// no environment variable override, and no fallback to system libraries.
pub fn load_bundled_pdfium(resource_dir: Option<&Path>) -> Option<Arc<Mutex<Pdfium>>> {
    BUNDLED_PDFIUM
        .get_or_init(|| init_bundled_pdfium(resource_dir))
        .clone()
}

/// Loads PDFium directly from an explicit library file path (primarily for tests/tools).
pub fn load_pdfium_from_path(path: &Path) -> Result<Arc<Mutex<Pdfium>>, PdfiumError> {
    let pdfium = bind_path(path)?;
    Ok(Arc::new(Mutex::new(pdfium)))
}

/// Loads PDFium for the standalone server runtime: checks `libs_dir` first, then falls back
/// to the system library.
pub fn load_server_pdfium(libs_dir: &Path) -> anyhow::Result<Arc<Mutex<Pdfium>>> {
    let mut shared = SERVER_PDFIUM
        .lock()
        .map_err(|_| anyhow::anyhow!("Pdfium initialization lock poisoned"))?;
    if let Some(pdfium) = shared.as_ref() {
        return Ok(Arc::clone(pdfium));
    }

    let local_library = Pdfium::pdfium_platform_library_name_at_path(libs_dir);
    let pdfium = if local_library.is_file() {
        let resolved_path =
            std::path::absolute(&local_library).unwrap_or_else(|_| local_library.clone());
        match bind_path(&resolved_path) {
            Ok(pdfium) => {
                info!(path = %resolved_path.display(), "loaded Pdfium library");
                pdfium
            }
            Err(local_error) => {
                warn!(
                    path = %resolved_path.display(),
                    error = %local_error,
                    "failed to load Pdfium from libs directory, falling back to system library"
                );
                bind_system_pdfium().map_err(|system_error| {
                    anyhow::anyhow!(
                        "failed to load Pdfium from {} ({local_error}) and system library ({system_error})",
                        resolved_path.display()
                    )
                })?
            }
        }
    } else {
        debug!(
            path = %local_library.display(),
            "Pdfium library not found in libs directory, loading system library"
        );
        bind_system_pdfium().with_context(|| {
            format!(
                "failed to load Pdfium (not found at {} and system library unavailable)",
                local_library.display()
            )
        })?
    };

    let pdfium = Arc::new(Mutex::new(pdfium));
    *shared = Some(Arc::clone(&pdfium));
    Ok(pdfium)
}

#[cfg(target_os = "android")]
fn init_bundled_pdfium(_resource_dir: Option<&Path>) -> Option<Arc<Mutex<Pdfium>>> {
    let library_name = "libpdfium.so";
    match bind_path(Path::new(library_name)) {
        Ok(pdfium) => Some(Arc::new(Mutex::new(pdfium))),
        Err(err) => {
            warn!(path = library_name, error = %err, "failed to load bundled Android Pdfium library");
            None
        }
    }
}

#[cfg(not(target_os = "android"))]
fn init_bundled_pdfium(resource_dir: Option<&Path>) -> Option<Arc<Mutex<Pdfium>>> {
    let env_override = std::env::var("SHIRO_PDFIUM_PATH").ok();
    let candidates = resolve_desktop_candidates(resource_dir, env_override.as_deref());

    for (strategy, candidate) in &candidates {
        if candidate.is_file() {
            let location = std::path::absolute(candidate)
                .unwrap_or_else(|_| candidate.clone())
                .display()
                .to_string();
            return match bind_path(candidate) {
                Ok(pdfium) => {
                    info!(strategy, path = %location, "loaded bundled Pdfium library");
                    Some(Arc::new(Mutex::new(pdfium)))
                }
                Err(err) => {
                    warn!(strategy, path = %location, error = %err, "failed to load bundled Pdfium library");
                    None
                }
            };
        }
    }

    let expected = candidates
        .iter()
        .find(|(strategy, _)| *strategy == "bundled_resource_dir")
        .or_else(|| candidates.first())
        .map(|(_, path)| path.display().to_string())
        .unwrap_or_else(|| "native/pdfium".to_owned());

    warn!(path = %expected, "bundled Pdfium library not found");
    None
}

/// Computes ordered candidate paths for the bundled desktop PDFium library.
///
/// In debug/test builds, an explicit `SHIRO_PDFIUM_PATH` override takes precedence so
/// developers and tests can supply a custom binary even when a staging copy exists.
/// In release builds, only the bundled resource directory is considered.
#[cfg(not(target_os = "android"))]
pub fn resolve_desktop_candidates(
    resource_dir: Option<&Path>,
    env_override: Option<&str>,
) -> Vec<(&'static str, PathBuf)> {
    let mut candidates = Vec::new();

    #[cfg(any(debug_assertions, test))]
    {
        if let Some(explicit_path) = env_override {
            let trimmed = explicit_path.trim();
            if !trimmed.is_empty() {
                candidates.push(("dev_env_override", PathBuf::from(trimmed)));
            }
        }
    }
    #[cfg(not(any(debug_assertions, test)))]
    let _ = env_override;

    if let Some(res_dir) = resource_dir {
        let native_dir = res_dir.join("native");
        candidates.push((
            "bundled_resource_dir",
            Pdfium::pdfium_platform_library_name_at_path(&native_dir),
        ));
    }

    #[cfg(all(target_os = "windows", any(debug_assertions, test)))]
    {
        let staging = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src-tauri/staging/windows-x64/native/pdfium.dll");
        candidates.push(("dev_staging", staging));
    }

    #[cfg(all(target_os = "linux", any(debug_assertions, test)))]
    {
        let staging = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src-tauri/staging/linux-x64/native/libpdfium.so");
        candidates.push(("dev_staging", staging));
    }

    candidates
}

fn bind_path(path: &Path) -> Result<Pdfium, PdfiumError> {
    match Pdfium::bind_to_library(path) {
        Ok(bindings) => Ok(Pdfium::new(bindings)),
        Err(PdfiumError::PdfiumLibraryBindingsAlreadyInitialized) => Ok(Pdfium::default()),
        Err(err) => Err(err),
    }
}

fn bind_system_pdfium() -> Result<Pdfium, PdfiumError> {
    match Pdfium::bind_to_system_library() {
        Ok(bindings) => Ok(Pdfium::new(bindings)),
        Err(PdfiumError::PdfiumLibraryBindingsAlreadyInitialized) => Ok(Pdfium::default()),
        Err(err) => Err(err),
    }
}

#[cfg(all(test, not(target_os = "android")))]
mod tests {
    use super::*;

    #[test]
    fn desktop_candidates_prioritize_dev_override_before_resource_dir() {
        let candidates = resolve_desktop_candidates(
            Some(Path::new("/app/resources")),
            Some("/custom/libpdfium.so"),
        );
        assert_eq!(candidates[0].0, "dev_env_override");
        assert_eq!(candidates[0].1, PathBuf::from("/custom/libpdfium.so"));
        assert_eq!(candidates[1].0, "bundled_resource_dir");
        assert!(candidates[1].1.starts_with("/app/resources/native"));
    }

    #[test]
    fn desktop_candidates_ignore_blank_env_override() {
        let candidates = resolve_desktop_candidates(Some(Path::new("/app/resources")), Some("   "));
        assert_eq!(candidates[0].0, "bundled_resource_dir");
        assert!(candidates[0].1.starts_with("/app/resources/native"));
    }
}
