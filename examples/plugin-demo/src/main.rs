#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use light_fw::prelude::*;

#[template("templates/app.html")]
#[proxy]
struct App {
    #[default(0)]
    count: i64,
}

fn main() -> light_fw::Result<()> {
    light_fw::run(App::new())
}
