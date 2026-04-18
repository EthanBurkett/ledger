mod api;
mod app;
mod cli;
mod db;

pub use app::{app, App};

fn main() {
    cli::init();
}
