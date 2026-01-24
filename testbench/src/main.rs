mod config;
mod runner;
mod servers;

pub mod prelude {
    pub use rkt::fairing::*;
    pub use rkt::response::stream::*;
    pub use rkt::*;

    pub use crate::config::*;
    pub use crate::register;
    pub use testbench::{Error, Result, *};
}

pub use runner::Test;

fn main() -> std::process::ExitCode {
    runner::run()
}
