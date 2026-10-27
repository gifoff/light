use light::prelude::*;

#[template("templates/app.html")]
#[proxy]
struct App {
    count: u32,
    loading: bool,
}

impl App {
    fn start_async(&self) {
        if self.loading() {
            return;
        }

        self.set_loading(true);

        let next = self.count() + 1;
        let _ = light::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(2));
            let _ = light::post_update::<App>(move |app: &mut App| {
                app.set_count(next);
                app.set_loading(false);
            });
        });
    }
}

fn main() -> light::Result<()> {
    light::run(App::new())
}
