use light::prelude::*;

#[template("templates/dashboard.html", modal = TaskModal)]
#[proxy]
struct Dashboard {
    #[default("overview".to_owned())]
    active_tab: String,
    #[default(12)]
    open_tasks: i64,
    #[default(8)]
    done_tasks: i64,
    #[default(3)]
    active_projects: i64,
}

impl Dashboard {
    fn show_tab(&self, tab: &str) {
        self.set_active_tab(tab.to_owned());
    }

    fn is_overview(&self) -> bool {
        self.active_tab() == "overview"
    }

    fn is_projects(&self) -> bool {
        self.active_tab() == "projects"
    }

    fn nav_background(&self, tab: &str) -> String {
        if self.active_tab() == tab {
            String::from("#2563EB")
        } else {
            String::from("#111827")
        }
    }

    fn overview_bg(&self) -> String {
        self.nav_background("overview")
    }

    fn projects_bg(&self) -> String {
        self.nav_background("projects")
    }

    fn activity_bg(&self) -> String {
        self.nav_background("activity")
    }

    fn task_label(&self) -> String {
        format!("{} открытых задач", self.open_tasks())
    }

    fn done_label(&self) -> String {
        format!("{} завершено", self.done_tasks())
    }

    fn projects_label(&self) -> String {
        format!("{} активных проекта", self.active_projects())
    }

    fn progress_width(&self, percent: i64) -> String {
        format!("{}%", percent.clamp(0, 100))
    }

    fn atlas_progress(&self) -> String {
        self.progress_width(72)
    }

    fn mobile_progress(&self) -> String {
        self.progress_width(45)
    }

    fn library_progress(&self) -> String {
        self.progress_width(88)
    }
}

#[template("templates/task_modal.html")]
#[proxy]
struct TaskModal {
    #[default(String::new())]
    title: String,
    #[default("Алексей".to_owned())]
    owner: String,
}

impl TaskModal {
    fn is_title_empty(&self) -> bool {
        self.title().trim().is_empty()
    }

    fn create(&self) {
        self.set_title(String::new());
        self.close();
    }
}

fn main() -> light::Result<()> {
    light::run(Dashboard::new())
}
