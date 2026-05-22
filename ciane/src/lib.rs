pub mod ast;
pub mod error;
pub mod parser;
pub mod syntax;
pub mod validation;

mod lexer;

pub use parser::parse;
