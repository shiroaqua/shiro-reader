/// Public user metadata; deliberately excludes password hashes and storage details.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserProfile {
    pub id: String,
    pub name: String,
    pub created_at: i128,
    pub updated_at: i128,
}

pub type GetUserOutput = UserProfile;

/// Storage health observable without exposing database paths or connections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserStorageStatus {
    pub registered_users: usize,
    pub orphaned_libraries: usize,
}
