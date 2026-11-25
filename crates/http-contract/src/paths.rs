pub const HEALTHZ: &str = "/healthz";
pub const META: &str = "/api/v1/meta";

pub const AUTH_LOGIN: &str = "/api/v1/auth/login";
pub const AUTH_REFRESH: &str = "/api/v1/auth/refresh";
pub const AUTH_LOGOUT: &str = "/api/v1/auth/logout";
pub const AUTH_ME: &str = "/api/v1/auth/me";

pub const BOOKS: &str = "/api/v1/library/bookshelf/books";
pub const TRASH: &str = "/api/v1/library/bookshelf/trash";
pub const TAGS: &str = "/api/v1/library/tags";
pub const BOOK_FILES: &str = "/api/v1/library/files/book";

pub const LIBRARY_ROUTE: &str = "/library";
pub const AUTH_ROUTE: &str = "/auth";
pub const META_ROUTE: &str = "/meta";

pub const AUTH_LOGIN_ROUTE: &str = "/login";
pub const AUTH_REFRESH_ROUTE: &str = "/refresh";
pub const AUTH_LOGOUT_ROUTE: &str = "/logout";
pub const AUTH_ME_ROUTE: &str = "/me";

pub const BOOKS_ROUTE: &str = "/bookshelf/books";
pub const BOOK_ROUTE: &str = "/bookshelf/books/{book_id}";
pub const BOOK_TAGS_ROUTE: &str = "/bookshelf/books/{book_id}/tags";
pub const BOOK_CHAPTERS_ROUTE: &str = "/bookshelf/books/{book_id}/chapters";
pub const BOOK_CHAPTER_ROUTE: &str = "/bookshelf/books/{book_id}/chapters/{chapter_id}";
pub const BOOK_COVER_ROUTE: &str = "/bookshelf/books/{book_id}/cover";
pub const TRASH_ROUTE: &str = "/bookshelf/trash";
pub const TRASH_BOOK_ROUTE: &str = "/bookshelf/trash/{book_id}";
pub const TRASH_RESTORE_ROUTE: &str = "/bookshelf/trash/{book_id}/restore";
pub const TAGS_ROUTE: &str = "/tags";
pub const TAG_ROUTE: &str = "/tags/{tag_id}";
pub const TAG_NAME_ROUTE: &str = "/tags/{tag_id}/name";
pub const TAG_COLOR_ROUTE: &str = "/tags/{tag_id}/color";
pub const TAG_PARENT_ROUTE: &str = "/tags/{tag_id}/parent";
pub const BOOK_FILES_ROUTE: &str = "/files/book";
pub const BOOK_FILE_ROUTE: &str = "/bookshelf/books/{book_id}/file";

pub fn book(book_id: &str) -> String {
    format!("{BOOKS}/{book_id}")
}

pub fn book_file(book_id: &str) -> String {
    format!("{BOOKS}/{book_id}/file")
}

pub fn book_cover(book_id: &str) -> String {
    format!("{BOOKS}/{book_id}/cover")
}

pub fn book_tags(book_id: &str) -> String {
    format!("{BOOKS}/{book_id}/tags")
}

pub fn book_chapters(book_id: &str) -> String {
    format!("{BOOKS}/{book_id}/chapters")
}

pub fn book_chapter(book_id: &str, chapter_id: &str) -> String {
    format!("{}/{}", book_chapters(book_id), chapter_id)
}

pub fn trash_restore(book_id: &str) -> String {
    format!("{TRASH}/{book_id}/restore")
}

pub fn tag(tag_id: &str) -> String {
    format!("{TAGS}/{tag_id}")
}

pub fn tag_name(tag_id: &str) -> String {
    format!("{TAGS}/{tag_id}/name")
}

pub fn tag_color(tag_id: &str) -> String {
    format!("{TAGS}/{tag_id}/color")
}

pub fn tag_parent(tag_id: &str) -> String {
    format!("{TAGS}/{tag_id}/parent")
}
