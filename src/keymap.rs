use crate::command::{ArgsProfile, Command};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Debug)]
pub struct KeymapEntry {
    pub keys: Vec<String>,
    // pub command: Command,
    // pub args: ArgsProfile,
    pub context: Vec<ContextEntry>,
}

#[derive(Deserialize, Debug)]
pub struct ContextEntry {
    pub name: Context,
    // pub operator: ContextOperator,
    // pub operand: ContextOperand,
}

#[derive(Deserialize, Debug)]
pub enum Context {
    Other(String),
}
