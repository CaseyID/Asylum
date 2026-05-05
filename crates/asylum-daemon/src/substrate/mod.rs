use uuid::Uuid;

use asylum_types::node::HarnessKind;

#[derive(Clone)]
pub struct SubstrateContext {
    pub node_id: Uuid,
    pub harness: HarnessKind,
    pub command: String,
    pub args: Vec<String>,
    pub workspace: Option<String>,
    pub env: Vec<(String, String)>,
}

pub trait SubstrateOutput: Fn(Uuid, &str) + Send + Sync {}
impl<T> SubstrateOutput for T where T: Fn(Uuid, &str) + Send + Sync {}

pub mod local;
pub mod loon;

pub use local::LocalSubstrate;
pub use loon::LoonSubstrate;
