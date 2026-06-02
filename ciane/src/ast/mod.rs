mod nodes;
mod traits;

pub use nodes::{
    Attr, AttrList, AttrValue, Job, JobBodyInline, JobBodySteps, PathItem, PathList, Ref, RefList,
    ReturnAnnotation, Root, Stage, StageBody, Step, StepsKeyword, TemplateDef, UseDecl,
    WorkflowBody, WorkflowDef,
};
pub use traits::{AstNode, HasAttrList, HasName};
