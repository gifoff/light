use light_fw::prelude::*;

#[template("templates/todo.html")]
#[proxy]
struct TodoApp {
    #[default(String::new())]
    draft: String,
    items: Vec<String>,
}

impl TodoApp {
    fn add(&self) -> bool {
        let draft = self.draft();
        let task = draft.trim();
        if task.is_empty() {
            return false;
        }
        let mut items = self.items();
        items.push(task.to_owned());
        self.set_items(items);
        self.set_draft(String::new());
        true
    }

    fn clear(&self) -> bool {
        self.set_items(Vec::new());
        true
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

fn main() -> light_fw::Result<()> {
    light_fw::run(TodoApp::new())
}
