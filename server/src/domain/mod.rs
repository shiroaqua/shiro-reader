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