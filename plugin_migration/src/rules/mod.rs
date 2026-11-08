mod decorators;
mod default_plugin_access;
mod enum_constants;
mod enum_imports;
mod event_names;
mod module_rename;
mod permission_levels;
mod redis_keywords;
mod slay_with_mod;

pub(crate) use decorators::Decorators;
pub(crate) use default_plugin_access::DefaultPluginAccess;
pub(crate) use enum_constants::EnumConstants;
pub(crate) use enum_imports::EnumImports;
pub(crate) use event_names::EventNames;
pub(crate) use module_rename::ModuleRename;
pub(crate) use permission_levels::PermissionLevels;
pub(crate) use redis_keywords::RedisKeywords;
pub(crate) use slay_with_mod::SlayWithMod;

use crate::{edit::Edit, source::Source};

/// A remark about a place in a plugin that a rule could not migrate on its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Warning {
    pub(crate) line: usize,
    pub(crate) message: String,
}

impl Warning {
    pub(crate) fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct RuleOutput {
    pub(crate) edits: Vec<Edit>,
    pub(crate) warnings: Vec<Warning>,
}

/// One migration step. Each rule runs on the text produced by the rules before it.
pub(crate) trait Rule: Send + Sync {
    fn name(&self) -> &'static str;

    fn apply(&self, source: &Source<'_>) -> RuleOutput;
}

/// All rules in the order they run.
pub(crate) fn default_rules() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(RedisKeywords),
        Box::new(SlayWithMod),
        Box::new(DefaultPluginAccess),
        Box::new(EventNames),
        Box::new(EnumConstants),
        Box::new(EnumImports),
        Box::new(PermissionLevels),
        Box::new(Decorators),
        Box::new(ModuleRename),
    ]
}
