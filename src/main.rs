mod api;
mod app;
mod auth;
mod cli;
mod core;
mod db;
mod env;

fn main() {
    let env_file = env::load();
    cli::init();
    let _ = env_file;
}
