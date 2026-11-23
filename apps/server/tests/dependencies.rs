use std::{
    collections::HashMap,
    io::Write,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use axum::{
    Router,
    extract::{Path as AxumPath, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
    routing::get,
};
use flate2::{Compression, write::GzEncoder};
use shiro_reader_server::infrastructure::dependencies::{
    ArchiveFormat, Dependency, DependencyArtifact, DependencyManager, TargetPlatform, libs_dir,
    pdfium::{DEFAULT_PDFIUM_BASE_URL, DEFAULT_PDFIUM_TAG, PdfiumDependency},
};
use tempfile::TempDir;
use tokio::net::TcpListener;

#[derive(Clone)]
struct MockFileServer {
    files: Arc<HashMap<String, Vec<u8>>>,
    hits: Arc<AtomicUsize>,
}

async fn start_mock_server(files: HashMap<String, Vec<u8>>) -> (String, Arc<AtomicUsize>) {
    let hits = Arc::new(AtomicUsize::new(0));
    let state = MockFileServer {
        files: Arc::new(files),
        hits: hits.clone(),
    };

    let router = Router::new()
        .route(
            "/releases/download/chromium/{build}/{asset}",
            get(redirect_asset),
        )
        .route("/objects/{asset}", get(serve_asset))
        .route("/raw/{asset}", get(serve_asset))
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    (format!("http://{addr}"), hits)
}

async fn redirect_asset(
    AxumPath((_build, asset)): AxumPath<(String, String)>,
) -> impl IntoResponse {
    Redirect::temporary(&format!("/objects/{asset}"))
}

async fn serve_asset(
    State(state): State<MockFileServer>,
    AxumPath(asset): AxumPath<String>,
) -> impl IntoResponse {
    state.hits.fetch_add(1, Ordering::SeqCst);
    match state.files.get(&asset) {
        Some(bytes) => (StatusCode::OK, bytes.clone()).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn build_tgz(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    {
        let mut tar = tar::Builder::new(&mut encoder);
        for (path, data) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, path, *data).unwrap();
        }
        tar.finish().unwrap();
    }
    encoder.finish().unwrap()
}

fn local_manager(libs: std::path::PathBuf) -> DependencyManager {
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    DependencyManager::with_client(libs, client)
}

fn dir_entry_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

struct CustomDependency {
    name: &'static str,
    version: &'static str,
    artifact: DependencyArtifact,
}

impl Dependency for CustomDependency {
    fn name(&self) -> &'static str {
        self.name
    }

    fn version(&self) -> &str {
        self.version
    }

    fn resolve(&self, _platform: TargetPlatform) -> anyhow::Result<DependencyArtifact> {
        Ok(self.artifact.clone())
    }
}

// 验证 Pdfium 对所有受支持平台解析出正确的下载资产、压缩包内路径与目标库名，并拒绝不支持的平台。
#[test]
fn pdfium_resolves_supported_platforms_and_rejects_unsupported() {
    let pdfium = PdfiumDependency::default();
    let expected = [
        (
            TargetPlatform::new("linux", "x86_64", "gnu"),
            "pdfium-linux-x64.tgz",
            "lib/libpdfium.so",
            "libpdfium.so",
        ),
        (
            TargetPlatform::new("linux", "aarch64", "gnu"),
            "pdfium-linux-arm64.tgz",
            "lib/libpdfium.so",
            "libpdfium.so",
        ),
        (
            TargetPlatform::new("linux", "arm", "gnu"),
            "pdfium-linux-arm.tgz",
            "lib/libpdfium.so",
            "libpdfium.so",
        ),
        (
            TargetPlatform::new("linux", "x86", "gnu"),
            "pdfium-linux-x86.tgz",
            "lib/libpdfium.so",
            "libpdfium.so",
        ),
        (
            TargetPlatform::new("linux", "x86_64", "musl"),
            "pdfium-linux-musl-x64.tgz",
            "lib/libpdfium.so",
            "libpdfium.so",
        ),
        (
            TargetPlatform::new("linux", "aarch64", "musl"),
            "pdfium-linux-musl-arm64.tgz",
            "lib/libpdfium.so",
            "libpdfium.so",
        ),
        (
            TargetPlatform::new("linux", "x86", "musl"),
            "pdfium-linux-musl-x86.tgz",
            "lib/libpdfium.so",
            "libpdfium.so",
        ),
        (
            TargetPlatform::new("macos", "x86_64", ""),
            "pdfium-mac-x64.tgz",
            "lib/libpdfium.dylib",
            "libpdfium.dylib",
        ),
        (
            TargetPlatform::new("macos", "aarch64", ""),
            "pdfium-mac-arm64.tgz",
            "lib/libpdfium.dylib",
            "libpdfium.dylib",
        ),
        (
            TargetPlatform::new("windows", "x86_64", "msvc"),
            "pdfium-win-x64.tgz",
            "bin/pdfium.dll",
            "pdfium.dll",
        ),
        (
            TargetPlatform::new("windows", "x86_64", "gnu"),
            "pdfium-win-x64.tgz",
            "bin/pdfium.dll",
            "pdfium.dll",
        ),
        (
            TargetPlatform::new("windows", "aarch64", "msvc"),
            "pdfium-win-arm64.tgz",
            "bin/pdfium.dll",
            "pdfium.dll",
        ),
        (
            TargetPlatform::new("windows", "x86", "msvc"),
            "pdfium-win-x86.tgz",
            "bin/pdfium.dll",
            "pdfium.dll",
        ),
        (
            TargetPlatform::new("android", "aarch64", ""),
            "pdfium-android-arm64.tgz",
            "lib/libpdfium.so",
            "libpdfium.so",
        ),
        (
            TargetPlatform::new("android", "arm", ""),
            "pdfium-android-arm.tgz",
            "lib/libpdfium.so",
            "libpdfium.so",
        ),
        (
            TargetPlatform::new("android", "x86_64", ""),
            "pdfium-android-x64.tgz",
            "lib/libpdfium.so",
            "libpdfium.so",
        ),
        (
            TargetPlatform::new("android", "x86", ""),
            "pdfium-android-x86.tgz",
            "lib/libpdfium.so",
            "libpdfium.so",
        ),
    ];

    for (platform, asset, entry_path, file_name) in expected {
        let artifact = pdfium.resolve(platform).unwrap();
        assert_eq!(artifact.file_name, file_name);
        assert_eq!(
            artifact.download_url,
            format!("{DEFAULT_PDFIUM_BASE_URL}/{DEFAULT_PDFIUM_TAG}/{asset}")
        );
        assert_eq!(
            artifact.archive,
            ArchiveFormat::TarGz {
                entry_path: entry_path.to_owned(),
            }
        );
        assert!(artifact.executable_on_unix);
    }

    for unsupported in [
        TargetPlatform::new("linux", "arm", "musl"),
        TargetPlatform::new("linux", "riscv64", "gnu"),
        TargetPlatform::new("freebsd", "x86_64", ""),
    ] {
        assert!(pdfium.resolve(unsupported).is_err());
    }
}

// 验证不同平台的 Pdfium 压缩包可正确跟随重定向下载、解压到 ROOT_DIRECTORY/libs 且重复初始化会跳过下载。
#[tokio::test]
async fn downloads_extracts_and_skips_existing_pdfium_across_platforms() {
    let linux_payload = b"fake-linux-pdfium-so";
    let android_payload = b"fake-android-pdfium-so";
    let macos_payload = b"fake-macos-pdfium-dylib";
    let windows_payload = b"fake-windows-pdfium-dll";

    let mut files = HashMap::new();
    files.insert(
        "pdfium-linux-x64.tgz".to_owned(),
        build_tgz(&[
            ("include/fpdfview.h", b"header"),
            ("lib/libpdfium.so", linux_payload),
        ]),
    );
    files.insert(
        "pdfium-android-arm64.tgz".to_owned(),
        build_tgz(&[("lib/libpdfium.so", android_payload)]),
    );
    files.insert(
        "pdfium-mac-arm64.tgz".to_owned(),
        build_tgz(&[("./lib/libpdfium.dylib", macos_payload)]),
    );
    files.insert(
        "pdfium-win-x64.tgz".to_owned(),
        build_tgz(&[
            ("lib/pdfium.dll.lib", b"import-lib"),
            ("bin/pdfium.dll", windows_payload),
        ]),
    );

    let (base_url, hits) = start_mock_server(files).await;
    let release_base = format!("{base_url}/releases/download");

    for (platform, expected_file, expected_bytes) in [
        (
            TargetPlatform::new("linux", "x86_64", "gnu"),
            "libpdfium.so",
            linux_payload.as_slice(),
        ),
        (
            TargetPlatform::new("android", "aarch64", ""),
            "libpdfium.so",
            android_payload.as_slice(),
        ),
        (
            TargetPlatform::new("macos", "aarch64", ""),
            "libpdfium.dylib",
            macos_payload.as_slice(),
        ),
        (
            TargetPlatform::new("windows", "x86_64", "msvc"),
            "pdfium.dll",
            windows_payload.as_slice(),
        ),
    ] {
        let root = TempDir::new().unwrap();
        let libs = libs_dir(root.path());
        assert_eq!(
            DependencyManager::with_default_dependencies(root.path())
                .unwrap()
                .libs_dir(),
            libs.as_path()
        );
        let mut manager = local_manager(libs.clone());
        manager.register(PdfiumDependency::new(
            release_base.clone(),
            DEFAULT_PDFIUM_TAG,
        ));

        let hits_before = hits.load(Ordering::SeqCst);
        manager.ensure_all_for(platform).await.unwrap();
        assert_eq!(hits.load(Ordering::SeqCst), hits_before + 1);

        let installed = libs.join(expected_file);
        assert_eq!(std::fs::read(&installed).unwrap(), expected_bytes);
        assert_eq!(dir_entry_names(&libs), vec![expected_file.to_owned()]);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&installed).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o755);
        }

        // 第二次检查应跳过下载。
        manager.ensure_all_for(platform).await.unwrap();
        assert_eq!(hits.load(Ordering::SeqCst), hits_before + 1);
    }
}

// 验证下载失败、压缩包损坏或缺失目标文件时不会在 libs 内遗留半成品，空目标文件会自动重新下载覆盖。
#[tokio::test]
async fn failed_download_or_corrupt_archive_leaves_no_partial_files_and_empty_target_is_replaced() {
    let mut corrupt_tgz = build_tgz(&[("lib/libpdfium.so", b"valid-pdfium-bytes")]);
    // 破坏 GZIP 尾部 CRC32 以验证完整性检查。
    let last = corrupt_tgz.len() - 1;
    corrupt_tgz[last] ^= 0xff;

    let mut files = HashMap::new();
    files.insert("pdfium-linux-arm64.tgz".to_owned(), corrupt_tgz);
    files.insert(
        "pdfium-linux-x86.tgz".to_owned(),
        build_tgz(&[("include/fpdfview.h", b"missing-so")]),
    );
    files.insert(
        "pdfium-linux-x64.tgz".to_owned(),
        build_tgz(&[("lib/libpdfium.so", b"fresh-pdfium")]),
    );

    let (base_url, _hits) = start_mock_server(files).await;
    let release_base = format!("{base_url}/releases/download");

    // 1. 404 错误不会留下临时文件或目标文件。
    let root_404 = TempDir::new().unwrap();
    let libs_404 = libs_dir(root_404.path());
    let mut manager_404 = local_manager(libs_404.clone());
    manager_404.register(PdfiumDependency::new(
        release_base.clone(),
        DEFAULT_PDFIUM_TAG,
    ));
    assert!(
        manager_404
            .ensure_all_for(TargetPlatform::new("linux", "arm", "gnu"))
            .await
            .is_err()
    );
    assert!(dir_entry_names(&libs_404).is_empty());

    // 2. 损坏的 gzip 校验失败且不会落盘。
    let root_corrupt = TempDir::new().unwrap();
    let libs_corrupt = libs_dir(root_corrupt.path());
    let mut manager_corrupt = local_manager(libs_corrupt.clone());
    manager_corrupt.register(PdfiumDependency::new(
        release_base.clone(),
        DEFAULT_PDFIUM_TAG,
    ));
    assert!(
        manager_corrupt
            .ensure_all_for(TargetPlatform::new("linux", "aarch64", "gnu"))
            .await
            .is_err()
    );
    assert!(dir_entry_names(&libs_corrupt).is_empty());

    // 3. 归档中缺失目标动态库时报错且不残留临时文件。
    let root_missing = TempDir::new().unwrap();
    let libs_missing = libs_dir(root_missing.path());
    let mut manager_missing = local_manager(libs_missing.clone());
    manager_missing.register(PdfiumDependency::new(
        release_base.clone(),
        DEFAULT_PDFIUM_TAG,
    ));
    assert!(
        manager_missing
            .ensure_all_for(TargetPlatform::new("linux", "x86", "gnu"))
            .await
            .is_err()
    );
    assert!(dir_entry_names(&libs_missing).is_empty());

    // 4. 已存在的 0 字节损坏目标文件会被自动重新下载并覆盖。
    let root_empty = TempDir::new().unwrap();
    let libs_empty = libs_dir(root_empty.path());
    std::fs::create_dir_all(&libs_empty).unwrap();
    let mut empty_file = std::fs::File::create(libs_empty.join("libpdfium.so")).unwrap();
    empty_file.flush().unwrap();
    drop(empty_file);

    let mut manager_empty = local_manager(libs_empty.clone());
    manager_empty.register(PdfiumDependency::new(release_base, DEFAULT_PDFIUM_TAG));
    manager_empty
        .ensure_all_for(TargetPlatform::new("linux", "x86_64", "gnu"))
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(libs_empty.join("libpdfium.so")).unwrap(),
        b"fresh-pdfium"
    );
}

// 验证依赖管理器支持注册多个依赖（含裸文件类型）并拒绝非法目标文件名。
#[tokio::test]
async fn supports_multiple_dependencies_and_rejects_path_traversal_file_names() {
    let mut files = HashMap::new();
    files.insert("extra.dat".to_owned(), b"extra-dependency-data".to_vec());
    files.insert(
        "pdfium-linux-x64.tgz".to_owned(),
        build_tgz(&[("lib/libpdfium.so", b"pdfium-bytes")]),
    );

    let (base_url, _hits) = start_mock_server(files).await;
    let root = TempDir::new().unwrap();
    let libs = libs_dir(root.path());

    let mut manager = local_manager(libs.clone());
    manager
        .register(PdfiumDependency::new(
            format!("{base_url}/releases/download"),
            DEFAULT_PDFIUM_TAG,
        ))
        .register(CustomDependency {
            name: "extra",
            version: "1.0.0",
            artifact: DependencyArtifact {
                file_name: "extra.dat".to_owned(),
                download_url: format!("{base_url}/raw/extra.dat"),
                archive: ArchiveFormat::Raw,
                executable_on_unix: false,
            },
        });

    manager
        .ensure_all_for(TargetPlatform::new("linux", "x86_64", "gnu"))
        .await
        .unwrap();

    assert_eq!(
        std::fs::read(libs.join("libpdfium.so")).unwrap(),
        b"pdfium-bytes"
    );
    assert_eq!(
        std::fs::read(libs.join("extra.dat")).unwrap(),
        b"extra-dependency-data"
    );

    let invalid = CustomDependency {
        name: "bad",
        version: "1.0.0",
        artifact: DependencyArtifact {
            file_name: "../escape.so".to_owned(),
            download_url: format!("{base_url}/raw/extra.dat"),
            archive: ArchiveFormat::Raw,
            executable_on_unix: false,
        },
    };
    assert!(
        manager
            .ensure(&invalid, TargetPlatform::new("linux", "x86_64", "gnu"))
            .await
            .is_err()
    );
}
