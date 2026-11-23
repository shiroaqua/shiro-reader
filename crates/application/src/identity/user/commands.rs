#[derive(Debug)]
pub struct UserRegisterCommand {
    pub name: String,
    pub password: String,
}

#[derive(Debug)]
pub struct UserUnregisterCommand {
    pub id: String,
}

#[derive(Debug)]
pub struct UserRenameCommand {
    pub id: String,
    pub name: String,
}

#[derive(Debug)]
pub struct UserChangePasswordCommand {
    pub id: String,
    pub password: String,
}
