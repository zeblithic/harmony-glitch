// Quick verification of serialization format
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HiVariant {
    Bats,
    Birds,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmoteKind {
    Hi(HiVariant),
    Dance,
    Hug,
}

fn main() {
    let hi = EmoteKind::Hi(HiVariant::Bats);
    let dance = EmoteKind::Dance;
    
    println!("Hi: {}", serde_json::to_string(&hi).unwrap());
    println!("Dance: {}", serde_json::to_string(&dance).unwrap());
}
