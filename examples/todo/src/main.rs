use light::prelude::*;

#[template("templates/todo.html")]
#[proxy]
struct TodoApp {
    #[default(String::new())]
    draft: String,
    items: Vec<String>,
}

impl TodoApp {
    fn add(&self) {
        let draft = self.draft();
        let task = draft.trim();
        if task.is_empty() {
            return;
        }
        let mut items = self.items();
        items.push(task.to_owned());
        self.set_items(items);
        self.set_draft(String::new());
    }

    fn clear(&self) {
        self.set_items(Vec::new());
    }

    fn is_empty(&self) -> bool {
        self.draft().trim().is_empty()
    }

    fn has_items(&self) -> bool {
        !self.items().is_empty()
    }

    fn is_list_empty(&self) -> bool {
        self.items().is_empty()
    }

    fn count_label(&self) -> String {
        format!("Задач: {}", self.items().len())
    }
}

fn main() -> light::Result<()> {
    light::run(TodoApp::new())
}
