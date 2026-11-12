#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use light_fw::prelude::*;

#[template("templates/counter.html")]
#[proxy]
struct Counter {
    count: i64,
}

impl Counter {
    fn increment(&self) -> bool {
        self.set_count(self.count() + 1);
        true
    }

    fn decrement(&self) -> bool {
        self.set_count(self.count() - 1);
        true
    }

    fn reset(&self) -> bool {
        self.set_count(0);
        true
    }

    fn is_zero(&self) -> bool {
        self.count() == 0
    }

    fn status(&self) -> String {
        if self.count() == 0 {
            "Начните счёт".to_owned()
        } else if self.count() > 0 {
            format!("Положительное значение: {}", self.count())
        } else {
            format!("Отрицательное значение: {}", self.count())
        }
    }
}

fn main() -> light_fw::Result<()> {
    light_fw::run(Counter::new())
}
