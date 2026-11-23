use shiro_http_contract::DeleteTagStrategy;

use crate::domain::library::tag::value_objects::TagDeleteStrategy;

pub fn map_delete_strategy(strategy: DeleteTagStrategy) -> TagDeleteStrategy {
    match strategy {
        DeleteTagStrategy::Cascade => TagDeleteStrategy::Cascade,
        DeleteTagStrategy::Promote => TagDeleteStrategy::Promote,
    }
}
