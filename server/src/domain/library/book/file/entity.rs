use derive_more::Display;

#[derive(Debug, Clone, Display)]
pub enum BookFileType {
    PDF,
    EPUB,
}