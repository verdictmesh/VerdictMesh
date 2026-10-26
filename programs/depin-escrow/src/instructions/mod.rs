pub mod dispute;
pub mod epoch;
pub mod pay;
pub mod settle;

// `#[program]` looks for the generated `#[derive(Accounts)]` modules at the
// crate root, hence the globs — and hence each handler is an associated
// function of its context rather than a free `fn handler`.
pub use dispute::*;
pub use epoch::*;
pub use settle::*;
