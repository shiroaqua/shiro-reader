pub mod library;
pub mod identity;
pub mod errors;

pub trait MapDbResult<T, E> {
    fn map_result(self) -> Result<T, E>;
}
