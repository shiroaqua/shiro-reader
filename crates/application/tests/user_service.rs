use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use shiro_application::identity::{
    crypto::errors::CryptoApplicationError,
    errors::IdentityApplicationError,
    ports::{
        IdentityCrypto, UserLibraryProvisioner, UserRepository, UserRepositoryError, UserSession,
        UserTransaction,
    },
    user::{
        commands::{UserChangePasswordCommand, UserRegisterCommand},
        errors::UserApplicationError,
        queries::GetUsersQuery,
        service::UserService,
    },
};
use shiro_domain::identity::user::{
    entity::{User, UserChange},
    value_objects::{UserId, UserName, UserPasswordHash},
};

const VALID_HASH_1: &str =
    "$argon2id$v=19$m=1024,t=1,p=1$c2FsdHNhbHRzYWx0$YWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWE";
const VALID_HASH_2: &str =
    "$argon2id$v=19$m=1024,t=1,p=1$c2FsdHNhbHRzYWx0$YmJiYmJiYmJiYmJiYmJiYmJiYmJiYmJiYmJiYmJiYmI";

#[derive(Clone)]
struct MockUserRepository {
    users: Arc<Mutex<Vec<User>>>,
    create_error: Arc<Mutex<Option<UserRepositoryError>>>,
}

impl Default for MockUserRepository {
    fn default() -> Self {
        Self {
            users: Arc::new(Mutex::new(Vec::new())),
            create_error: Arc::new(Mutex::new(None)),
        }
    }
}

impl UserRepository for MockUserRepository {
    type Session = MockUserSession;

    fn session(&self) -> Self::Session {
        MockUserSession {
            users: Arc::clone(&self.users),
        }
    }

    async fn create(&self, user: User) -> Result<User, UserRepositoryError> {
        if let Some(err) = self.create_error.lock().unwrap().take() {
            return Err(err);
        }
        self.users.lock().unwrap().push(user.clone());
        Ok(user)
    }

    async fn delete(&self, _user: User) -> Result<(), UserRepositoryError> {
        Ok(())
    }

    async fn find_by_id(&self, id: &UserId) -> Result<User, UserRepositoryError> {
        self.users
            .lock()
            .unwrap()
            .iter()
            .find(|user| user.id() == id)
            .cloned()
            .ok_or(UserRepositoryError::NotFound)
    }

    async fn find_by_name(&self, name: UserName) -> Result<Option<User>, UserRepositoryError> {
        Ok(self
            .users
            .lock()
            .unwrap()
            .iter()
            .find(|user| user.name() == &name)
            .cloned())
    }

    async fn list(&self, ids: &[UserId]) -> Result<Vec<User>, UserRepositoryError> {
        let users = self.users.lock().unwrap();
        if ids.is_empty() {
            Ok(users.clone())
        } else {
            Ok(users
                .iter()
                .filter(|user| ids.contains(user.id()))
                .cloned()
                .collect())
        }
    }
}

struct MockUserSession {
    users: Arc<Mutex<Vec<User>>>,
}

impl UserSession for MockUserSession {
    type Tx<'a> = MockUserTransaction;

    async fn begin_transaction(&mut self) -> Result<Self::Tx<'_>, UserRepositoryError> {
        Ok(MockUserTransaction {
            users: Arc::clone(&self.users),
        })
    }
}

struct MockUserTransaction {
    users: Arc<Mutex<Vec<User>>>,
}

impl UserTransaction for MockUserTransaction {
    async fn find_by_id(&mut self, id: &UserId) -> Result<User, UserRepositoryError> {
        self.users
            .lock()
            .unwrap()
            .iter()
            .find(|user| user.id() == id)
            .cloned()
            .ok_or(UserRepositoryError::NotFound)
    }

    async fn apply_user_change(
        &mut self,
        _change: UserChange<'_>,
    ) -> Result<(), UserRepositoryError> {
        Ok(())
    }

    async fn commit(self) -> Result<(), UserRepositoryError> {
        Ok(())
    }
}

#[derive(Default)]
struct MockProvisioner {
    fail_provision: AtomicBool,
    fail_remove: AtomicBool,
    provisioned: Mutex<Vec<String>>,
    removed: Mutex<Vec<String>>,
    cleaned_up: Mutex<Vec<String>>,
}

impl UserLibraryProvisioner for MockProvisioner {
    async fn provision_library(&self, user_id: &str) -> anyhow::Result<()> {
        if self.fail_provision.load(Ordering::SeqCst) {
            anyhow::bail!("provision disk full");
        }
        self.provisioned.lock().unwrap().push(user_id.to_owned());
        Ok(())
    }

    async fn remove_library(&self, user_id: &str) -> anyhow::Result<()> {
        self.removed.lock().unwrap().push(user_id.to_owned());
        if self.fail_remove.load(Ordering::SeqCst) {
            anyhow::bail!("remove permission denied");
        }
        Ok(())
    }

    fn cleanup_failed_provision(&self, user_id: &str) -> anyhow::Result<()> {
        self.cleaned_up.lock().unwrap().push(user_id.to_owned());
        Ok(())
    }
}

struct MockCrypto {
    hash_result: Mutex<String>,
}

impl MockCrypto {
    fn returning(hash: &str) -> Self {
        Self {
            hash_result: Mutex::new(hash.to_owned()),
        }
    }
}

impl IdentityCrypto for MockCrypto {
    fn hash_password(&self, _plaintext: &str) -> Result<String, CryptoApplicationError> {
        Ok(self.hash_result.lock().unwrap().clone())
    }

    fn verify_password(
        &self,
        _plaintext: &str,
        _stored_hash: &str,
    ) -> Result<bool, CryptoApplicationError> {
        Ok(true)
    }

    fn issue_access_token(&self, _user_id: String) -> Result<String, CryptoApplicationError> {
        Ok("access".into())
    }

    fn issue_refresh_token(&self, _user_id: String) -> Result<String, CryptoApplicationError> {
        Ok("refresh".into())
    }

    fn verify_access_token(&self, _token: &str) -> Result<String, CryptoApplicationError> {
        Ok(String::new())
    }

    fn verify_refresh_token(&self, _token: &str) -> Result<String, CryptoApplicationError> {
        Ok(String::new())
    }
}

#[tokio::test]
async fn register_compensates_provisioned_library_when_repository_create_fails() {
    let repo = MockUserRepository::default();
    *repo.create_error.lock().unwrap() = Some(UserRepositoryError::NameConflict);
    let provisioner = Arc::new(MockProvisioner::default());
    let crypto = Arc::new(MockCrypto::returning(VALID_HASH_1));
    let service = UserService::new(repo.clone(), Arc::clone(&provisioner), crypto);

    let err = service
        .register(UserRegisterCommand {
            name: "Alice".into(),
            password: "pw".into(),
        })
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        IdentityApplicationError::User(UserApplicationError::NameConflict)
    ));
    let provisioned = provisioner.provisioned.lock().unwrap().clone();
    let removed = provisioner.removed.lock().unwrap().clone();
    assert_eq!(provisioned.len(), 1);
    assert_eq!(removed, provisioned);
}

#[tokio::test]
async fn register_reports_cleanup_context_when_both_create_and_remove_fail() {
    let repo = MockUserRepository::default();
    *repo.create_error.lock().unwrap() = Some(UserRepositoryError::NameConflict);
    let provisioner = Arc::new(MockProvisioner::default());
    provisioner.fail_remove.store(true, Ordering::SeqCst);
    let crypto = Arc::new(MockCrypto::returning(VALID_HASH_1));
    let service = UserService::new(repo, Arc::clone(&provisioner), crypto);

    let err = service
        .register(UserRegisterCommand {
            name: "Alice".into(),
            password: "pw".into(),
        })
        .await
        .unwrap_err();
    match err {
        IdentityApplicationError::User(UserApplicationError::Storage(inner)) => {
            let msg = format!("{inner:#}");
            assert!(msg.contains("failed to clean up registration"), "{msg}");
            assert!(msg.contains("remove permission denied"), "{msg}");
        }
        other => panic!("expected storage error, got {other:?}"),
    }
}

#[tokio::test]
async fn register_cleans_up_failed_provision_when_provisioning_fails() {
    let repo = MockUserRepository::default();
    let provisioner = Arc::new(MockProvisioner::default());
    provisioner.fail_provision.store(true, Ordering::SeqCst);
    let crypto = Arc::new(MockCrypto::returning(VALID_HASH_1));
    let service = UserService::new(repo, Arc::clone(&provisioner), crypto);

    let err = service
        .register(UserRegisterCommand {
            name: "Alice".into(),
            password: "pw".into(),
        })
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        IdentityApplicationError::User(UserApplicationError::Storage(_))
    ));
    assert_eq!(provisioner.cleaned_up.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn invalid_or_unchanged_password_hash_maps_to_user_application_errors() {
    let repo = MockUserRepository::default();
    let user = User::new(
        UserId::new(),
        UserName::parse("Alice").unwrap(),
        UserPasswordHash::parse(VALID_HASH_1).unwrap(),
    );
    let user_id = user.id().to_string();
    repo.users.lock().unwrap().push(user);

    let provisioner = Arc::new(MockProvisioner::default());
    let bad_crypto = Arc::new(MockCrypto::returning("not-a-valid-phc"));
    let bad_service = UserService::new(repo.clone(), Arc::clone(&provisioner), bad_crypto);

    let reg_err = bad_service
        .register(UserRegisterCommand {
            name: "Bob".into(),
            password: "pw".into(),
        })
        .await
        .unwrap_err();
    assert!(matches!(
        reg_err,
        IdentityApplicationError::User(UserApplicationError::InvalidPasswordHash)
    ));

    let change_bad = bad_service
        .change_password(UserChangePasswordCommand {
            id: user_id.clone(),
            password: "pw".into(),
        })
        .await
        .unwrap_err();
    assert!(matches!(
        change_bad,
        IdentityApplicationError::User(UserApplicationError::InvalidPasswordHash)
    ));

    // Same hash returned as current stored hash -> PasswordHashUnchanged.
    let same_crypto = Arc::new(MockCrypto::returning(VALID_HASH_1));
    let same_service = UserService::new(repo, provisioner, same_crypto);
    let unchanged = same_service
        .change_password(UserChangePasswordCommand {
            id: user_id,
            password: "pw".into(),
        })
        .await
        .unwrap_err();
    assert!(matches!(
        unchanged,
        IdentityApplicationError::User(UserApplicationError::PasswordHashUnchanged)
    ));
}

#[tokio::test]
async fn get_users_filters_by_ids_and_rejects_invalid_id() {
    let repo = MockUserRepository::default();
    let alice = User::new(
        UserId::new(),
        UserName::parse("Alice").unwrap(),
        UserPasswordHash::parse(VALID_HASH_1).unwrap(),
    );
    let bob = User::new(
        UserId::new(),
        UserName::parse("Bob").unwrap(),
        UserPasswordHash::parse(VALID_HASH_2).unwrap(),
    );
    let alice_id = alice.id().to_string();
    let bob_id = bob.id().to_string();
    repo.users.lock().unwrap().extend([alice, bob]);

    let service = UserService::new(
        repo,
        Arc::new(MockProvisioner::default()),
        Arc::new(MockCrypto::returning(VALID_HASH_1)),
    );

    let all = service
        .get_users(GetUsersQuery { ids: vec![] })
        .await
        .unwrap();
    assert_eq!(all.len(), 2);

    let filtered = service
        .get_users(GetUsersQuery {
            ids: vec![bob_id.clone()],
        })
        .await
        .unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, bob_id);
    assert_eq!(filtered[0].name, "Bob");

    let mut both = service
        .get_users(GetUsersQuery {
            ids: vec![alice_id, bob_id],
        })
        .await
        .unwrap();
    both.sort_by(|a, b| a.name.cmp(&b.name));
    assert_eq!(both.len(), 2);

    let err = service
        .get_users(GetUsersQuery {
            ids: vec!["not-a-uuid".into()],
        })
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        IdentityApplicationError::User(UserApplicationError::InvalidId)
    ));
}
