//! Implementation of the `#[template]` attribute macro.
//!
//! Splits the template HTML file into nodes, resolves `<include>` directives,
//! and generates a Rust `Component` implementation from the parsed tree.

mod attrs;
mod codegen;
mod elements;
mod forms;
mod handlers;
mod keys;
mod methods;
mod parser;
mod pipeline;
mod state;
mod tokens;
mod types;
mod validation;
mod window;

pub use pipeline::expand;
