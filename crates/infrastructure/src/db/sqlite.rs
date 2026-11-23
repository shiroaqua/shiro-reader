use std::error::Error;

pub fn sqlite_error(error: &toasty::Error) -> Option<&rusqlite::Error> {
    let mut source: Option<&(dyn Error + 'static)> = Some(error);
    while let Some(current) = source {
        if let Some(sqlite) = current.downcast_ref::<rusqlite::Error>() {
            return Some(sqlite);
        }
        source = current.source();
    }
    None
}
