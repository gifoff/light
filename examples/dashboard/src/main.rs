use light::prelude::*;

#[template("templates/dashboard.html")]
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
    #[default("Алексей Ковалёв".to_owned())]
    profile_name: String,
    #[default(false)]
    alternate_avatar: bool,
    #[default(false)]
    profile_modal_open: bool,
    #[default(String::new())]
    profile_draft: String,
    #[default(false)]
    modal_open: bool,
    #[default(String::new())]
    new_task: String,
    #[default("Алексей".to_owned())]
    new_owner: String,
    tasks: Vec<String>,
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

    fn toggle_avatar(&self) {
        self.set_alternate_avatar(!self.alternate_avatar());
    }

    fn open_profile_modal(&self) {
        self.set_profile_draft(self.profile_name());
        self.set_profile_modal_open(true);
    }

    fn close_profile_modal(&self) {
        self.set_profile_modal_open(false);
        self.set_profile_draft(String::new());
    }

    fn is_profile_draft_empty(&self) -> bool {
        self.profile_draft().trim().is_empty()
    }

    fn save_profile(&self) {
        let name = self.profile_draft().trim().to_owned();
        if name.is_empty() {
            return;
        }
        self.set_profile_name(name);
        self.set_profile_modal_open(false);
        self.set_profile_draft(String::new());
    }

    fn open_task_modal(&self) {
        self.set_modal_open(true);
    }

    fn close_task_modal(&self) {
        self.set_modal_open(false);
        self.set_new_task(String::new());
    }

    fn can_create_task(&self) -> bool {
        !self.new_task().trim().is_empty()
    }

    fn is_new_task_empty(&self) -> bool {
        !self.can_create_task()
    }

    fn create_task(&self) {
        let title = self.new_task().trim().to_owned();
        if title.is_empty() {
            return;
        }
        let mut tasks = self.tasks();
        tasks.insert(0, title);
        self.set_tasks(tasks);
        self.set_open_tasks(self.open_tasks() + 1);
        self.set_new_task(String::new());
        self.set_modal_open(false);
    }

    fn bar_height(&self, index: usize) -> String {
        const BASE: [i64; 7] = [42, 58, 36, 76, 64, 88, 54];
        let bump = self.tasks().len() as i64 * 3 + self.open_tasks() % 5;
        format!("{}%", (BASE[index % BASE.len()] + bump).clamp(12, 100))
    }

    fn bar_0(&self) -> String {
        self.bar_height(0)
    }

    fn bar_1(&self) -> String {
        self.bar_height(1)
    }

    fn bar_2(&self) -> String {
        self.bar_height(2)
    }

    fn bar_3(&self) -> String {
        self.bar_height(3)
    }

    fn bar_4(&self) -> String {
        self.bar_height(4)
    }

    fn bar_5(&self) -> String {
        self.bar_height(5)
    }

    fn bar_6(&self) -> String {
        self.bar_height(6)
    }

    fn weekly_total(&self) -> i64 {
        42 + self.tasks().len() as i64
    }
}

fn main() -> light::Result<()> {
    light::run(Dashboard::new())
}
