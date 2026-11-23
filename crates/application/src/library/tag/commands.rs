use shiro_domain::library::tag::value_objects::TagDeleteStrategy;

#[derive(Debug)]
pub struct CreateTagDefinitionCommand {
    pub name: String,
    pub parent_id: Option<String>,
    pub color: String,
    pub allows_value: bool,
    pub value_regex: Option<String>,
}

#[derive(Debug)]
pub struct RenameTagCommand {
    pub id: String,
    pub name: String,
}

#[derive(Debug)]
pub struct ChangeTagColorCommand {
    pub id: String,
    pub color: String,
}

#[derive(Debug)]
pub struct MoveTagCommand {
    pub id: String,
    pub parent_id: Option<String>,
}

#[derive(Debug)]
pub struct BindBookTagCommand {
    pub book_id: String,
    pub tag_id: String,
    pub value: Option<String>,
}

#[derive(Debug)]
pub struct UnbindBookTagCommand {
    pub book_id: String,
    pub tag_id: String,
}

#[derive(Debug)]
pub struct UndefineTagCommand {
    pub id: String,
    pub strategy: TagDeleteStrategy,
}
