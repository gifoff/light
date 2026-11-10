use light_fw::prelude::*;

#[template("templates/app.html")]
#[proxy]
struct App {}

fn main() -> light_fw::Result<()> {
    light_fw::run_with_config("light.config.json", App::new())
}
