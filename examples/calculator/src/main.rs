use light::prelude::*;

#[template("templates/calculator.html")]
#[proxy]
struct Calculator {
    #[default("0".to_owned())]
    display: String, // Хранит полную строку формулы (например, "10 * 5")
    #[default("0".to_owned())]
    current_input: String, // Хранит только текущее вводимое число (например, "5")
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
            self.set_current_input(digit.to_owned());
            if self.pending().is_empty() {
                self.set_display(digit.to_owned());
            } else {
                self.set_display(format!("{} {}", self.display(), digit));
            }
            self.set_waiting(false);
            return;
        }
        let mut input = self.current_input();
        let mut display = self.display();
        if input == "0" {
            input.clear();
            if display == "0" {
                display.clear();
            }
        }
        input.push_str(digit);
        display.push_str(digit);
        self.set_current_input(input);
        self.set_display(display);
    }

    fn dot(&self) {
        if self.waiting() {
            self.set_current_input("0.".to_owned());
            if self.pending().is_empty() {
                self.set_display("0.".to_owned());
            } else {
                self.set_display(format!("{} 0.", self.display()));
            }
            self.set_waiting(false);
            return;
        }
        let input = self.current_input();
        if !input.contains('.') {
            self.set_current_input(format!("{input}."));
            self.set_display(format!("{}.", self.display()));
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
        self.set_display(format!("{} {}", self.display(), op)); // Добавляем оператор в формулу
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
        let res_str = format_number(result);
        self.set_display(res_str.clone());
        self.set_current_input(res_str);
        self.set_accumulator(result);
        self.set_pending(String::new());
        self.set_waiting(true);
    }

    fn clear(&self) {
        self.set_display("0".to_owned());
        self.set_current_input("0".to_owned());
        self.set_accumulator(0.0);
        self.set_pending(String::new());
        self.set_waiting(false);
    }

    fn current_value(&self) -> f64 {
        self.current_input().parse().unwrap_or(0.0)
    }

    fn show_error(&self) {
        self.set_display("Error".to_owned());
        self.set_current_input("0".to_owned());
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
