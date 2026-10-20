use light::prelude::*;

#[template("templates/calculator.html")]
#[proxy]
struct Calculator {
    #[default("0".to_owned())]
    display: String,
    #[default(0.0)]
    accumulator: f64,
    #[default(String::new())]
    pending: String,
    #[default(false)]
    waiting: bool,
}

impl Calculator {
    fn digit(&self, digit: &str) {
        if self.waiting() {
            self.set_display(digit.to_owned());
            self.set_waiting(false);
            return;
        }
        let mut display = self.display();
        if display == "0" {
            display.clear();
        }
        display.push_str(digit);
        self.set_display(display);
    }

    fn dot(&self) {
        if self.waiting() {
            self.set_display("0.".to_owned());
            self.set_waiting(false);
            return;
        }
        let display = self.display();
        if !display.contains('.') {
            self.set_display(format!("{display}."));
        }
    }

    fn op(&self, op: &str) {
        let current = self.current_value();
        if self.pending().is_empty() {
            self.set_accumulator(current);
        } else {
            let left = self.accumulator();
            let Some(result) = apply(left, current, &self.pending()) else {
                self.show_error();
                return;
            };
            self.set_display(format_number(result));
            self.set_accumulator(result);
        }
        self.set_pending(op.to_owned());
        self.set_waiting(true);
    }

    fn equals(&self) {
        if self.pending().is_empty() {
            return;
        }
        let current = self.current_value();
        let left = self.accumulator();
        let Some(result) = apply(left, current, &self.pending()) else {
            self.show_error();
            return;
        };
        self.set_display(format_number(result));
        self.set_accumulator(result);
        self.set_pending(String::new());
        self.set_waiting(true);
    }

    fn clear(&self) {
        self.set_display("0".to_owned());
        self.set_accumulator(0.0);
        self.set_pending(String::new());
        self.set_waiting(false);
    }

    fn current_value(&self) -> f64 {
        self.display().parse().unwrap_or(0.0)
    }

    fn show_error(&self) {
        self.set_display("Error".to_owned());
        self.set_accumulator(0.0);
        self.set_pending(String::new());
        self.set_waiting(true);
    }
}

fn apply(left: f64, right: f64, op: &str) -> Option<f64> {
    match op {
        "+" => Some(left + right),
        "-" => Some(left - right),
        "*" => Some(left * right),
        "/" if right == 0.0 => None,
        "/" => Some(left / right),
        _ => Some(right),
    }
}

fn format_number(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

fn main() -> light::Result<()> {
    light::run(Calculator::new())
}
