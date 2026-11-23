use derive_more::Display;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
pub enum BookFileType {
    PDF,
    EPUB,
}
