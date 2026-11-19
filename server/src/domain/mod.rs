pub mod identity;
pub mod library;

pub trait EntityId {
    type Id;

    fn id(&self) -> &Self::Id;
}

pub trait EntityCreate: Sized {
    type Create;

    fn create(self) -> Self::Create;
}

pub trait ChangeTarget<M> {
    type Error;

    fn executor(&mut self) -> &mut dyn ::toasty::codegen_support::Executor;
    fn map_error(error: ::toasty::Error) -> Self::Error;
}
