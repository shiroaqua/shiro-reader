use super::*;
use std::{path::PathBuf, process::Command};

pub struct Restart {
    root: PathBuf,
    _directory: Option<TempDir>,
    stage: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct SavedReader {
    name: String,
    session: Session,
    book: BookData,
}

#[derive(Serialize, Deserialize)]
struct SavedTags {
    session: Session,
    book: BookData,
    promoted: TagDefinitionData,
    leaf: TagDefinitionData,
    deleted_ids: Vec<String>,
}

impl Restart {
    pub async fn seed_tag_tree(&self) {
        match self.stage.as_deref() {
            None => self.run_stage("seed"),
            Some("seed") => {
                let app = TestApp::at(&self.root).await;
                let session = app.session("Reader").await;
                app.upload(&session, SampleFile::Epub).await;
                let book = app.add_book(&session, "Book", SampleFile::Epub).await.data;
                let root = app.add_tag(&session, "Root", None, false, None).await;
                let promoted = app.add_tag(&session, "Rating", Some(&root.id), true, Some("[0-9]+")).await;
                let leaf = app.add_tag(&session, "Leaf", Some(&promoted.id), false, None).await;
                let removed = app.add_tag(&session, "Removed", None, false, None).await;
                let removed_child = app.add_tag(&session, "RemovedChild", Some(&removed.id), false, None).await;
                app.bind_tag(&session, &book.id, &promoted.id, Some("7")).await.assert_empty(204);
                app.rename_tag(&session, &promoted.id, "Score").await.assert_empty(204);
                app.delete_tag(&session, &root.id, DeleteStrategy::Promote).await.assert_empty(204);
                app.delete_tag(&session, &removed.id, DeleteStrategy::Cascade).await.assert_empty(204);
                let saved = SavedTags {
                    session, book, promoted: promoted.renamed("Score").under(None), leaf,
                    deleted_ids: vec![root.id, removed.id, removed_child.id],
                };
                // 只保存测试通过 API 获得的数据，不读取应用数据库。
                std::fs::write(self.root.join("tag-test-snapshot.json"), serde_json::to_vec(&saved).unwrap()).unwrap();
            }
            Some("restore") => {}
            stage => panic!("unknown restart stage: {stage:?}"),
        }
    }

    pub async fn assert_tag_tree_restored(&self) {
        match self.stage.as_deref() {
            None => self.run_stage("restore"),
            Some("restore") => {
                let saved: SavedTags = serde_json::from_slice(
                    &std::fs::read(self.root.join("tag-test-snapshot.json")).unwrap()).unwrap();
                let app = TestApp::at(&self.root).await;
                assert_eq!(app.tag_tree(&saved.session).await,
                    [saved.promoted.node(vec![saved.leaf.node(vec![])])]);
                assert_eq!(app.books(&saved.session).await, [saved.book.clone()]);
                app.assert_book_tags(&saved.session, &saved.book.id, &[saved.promoted.binding(Some("7"))]).await;
                app.bind_tag(&saved.session, &saved.book.id, &saved.promoted.id, Some("bad"))
                    .await.assert_tag_error(TagError::RegexMismatch);
                app.bind_tag(&saved.session, &saved.book.id, &saved.promoted.id, Some("8"))
                    .await.assert_empty(204);
                for id in saved.deleted_ids {
                    app.rename_tag(&saved.session, &id, "Missing").await.assert_tag_error(TagError::NotFound);
                }
                app.rename_tag(&saved.session, &saved.promoted.id, "AfterRestart").await.assert_empty(204);
                assert_eq!(app.tag_tree(&saved.session).await,
                    [saved.promoted.renamed("AfterRestart").node(vec![saved.leaf.node(vec![])])]);
                app.assert_book_tags(&saved.session, &saved.book.id, &[saved.promoted.binding(Some("8"))]).await;
            }
            Some("seed") => {}
            stage => panic!("unknown restart stage: {stage:?}"),
        }
    }

    pub fn new() -> Self {
        match std::env::var("SHIRO_RESTART_STAGE") {
            Ok(stage) => Self {
                root: std::env::var_os("SHIRO_RESTART_ROOT").unwrap().into(),
                _directory: None, stage: Some(stage),
            },
            Err(_) => {
                let directory = tempfile::tempdir().unwrap();
                Self { root: directory.path().into(), _directory: Some(directory), stage: None }
            }
        }
    }

    pub async fn seed_users_and_books(&self) {
        match self.stage.as_deref() {
            None => self.run_stage("seed"),
            Some("seed") => {
                let app = TestApp::at(&self.root).await;
                let alice = app.session("Alice").await;
                let bob = app.session("Bob").await;
                app.upload(&alice, SampleFile::Epub).await;
                let mut readers = Vec::new();
                for (name, session, title) in [("Alice", alice, "Abook"), ("Bob", bob, "Bbook")] {
                    let book = app.add_book(&session, title, SampleFile::Epub).await.data;
                    readers.push(SavedReader { name: name.into(), session, book });
                }
                // This is a test-owned snapshot of API results, not application storage.
                std::fs::write(self.root.join("test-snapshot.json"), serde_json::to_vec(&readers).unwrap()).unwrap();
            }
            Some("restore") => {}
            stage => panic!("unknown restart stage: {stage:?}"),
        }
    }

    pub async fn assert_users_books_and_tokens_restored(&self) {
        match self.stage.as_deref() {
            None => self.run_stage("restore"),
            Some("restore") => {
                let readers: Vec<SavedReader> = serde_json::from_slice(
                    &std::fs::read(self.root.join("test-snapshot.json")).unwrap()).unwrap();
                let app = TestApp::at(&self.root).await;
                for reader in readers {
                    let login = app.login(&reader.name, PASSWORD).await;
                    assert_eq!(login.user_id, reader.session.user_id);
                    assert_eq!(app.books(&login).await, [reader.book.clone()]);
                    assert_eq!(app.books(&reader.session).await, [reader.book.clone()]);
                    let refreshed = app.refresh(&reader.session).await;
                    assert_eq!(app.books(&refreshed).await, [reader.book]);
                    app.download(&login, &SampleFile::Epub.hash()).await.assert_file(&SampleFile::Epub);
                }
            }
            Some("seed") => {}
            stage => panic!("unknown restart stage: {stage:?}"),
        }
    }

    fn run_stage(&self, stage: &str) {
        let name = std::thread::current().name().unwrap().to_owned();
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &name, "--nocapture"])
            .env("SHIRO_RESTART_STAGE", stage)
            .env("SHIRO_RESTART_ROOT", &self.root).output().expect("restart application process");
        assert!(output.status.success(), "restart {stage} failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    }
}
