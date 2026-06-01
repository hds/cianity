mod nodes;
mod traits;

pub use nodes::{
    Attr, AttrList, AttrValue, Job, JobBodyInline, JobBodySteps, Ref, RefList, Root, Stage,
    StageBody, Step, StepsKeyword, TemplateDef, UseDecl, WorkflowBody, WorkflowDef,
};
pub use traits::{AstNode, HasAttrList, HasName};
