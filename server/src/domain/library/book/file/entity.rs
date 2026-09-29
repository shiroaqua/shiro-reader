use derive_more::Display;

#[derive(Debug, Clone, PartialEq, Eq, Display)]
pub enum BookFileType {
    PDF,
    EPUB,
}
