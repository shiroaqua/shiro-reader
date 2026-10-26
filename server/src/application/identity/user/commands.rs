pub struct UserRegisterCommand {
    pub name: String,
    pub password: String,
}

pub struct UserUnregisterCommand {
    pub id: String,
}

pub struct UserReanmeCommand {
    pub id: String,
    pub name: String
}
pub struct UserChangePasswordCommand {
    pub id: String,
    pub password: String
}
