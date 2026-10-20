use light::prelude::*;

#[template("templates/counter.html")]
#[proxy]
struct Counter {
    count: i64,
}

impl Counter {
    fn increment(&self) {
        self.set_count(self.count() + 1);
    }

    fn decrement(&self) {
        self.set_count(self.count() - 1);
    }

    fn reset(&self) {
        self.set_count(0);
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

fn main() -> light::Result<()> {
    light::run(Counter::new())
}
