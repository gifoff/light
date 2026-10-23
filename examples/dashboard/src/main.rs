use std::fs;
use std::path::Path;

use light::ImageData;
use light::prelude::*;

const MAX_AVATAR_BYTES: u64 = 2 * 1024 * 1024;
const MAX_AVATAR_DIMENSION: u32 = 1024;
const DEFAULT_TASKS: [&str; 12] = [
    "Редизайн настроек",
    "Экспорт отчётов",
    "Обновление дизайн-системы",
    "Подготовка релиза",
    "Аудит доступности",
    "Мобильная навигация",
    "Интеграция платежей",
    "Оптимизация лендинга",
    "Миграция базы данных",
    "Настройка аналитики",
    "Документация API",
    "Тестирование сценариев",
];

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
    #[default("Алексей Ковалёв — Product Design Lead".to_owned())]
    profile_name: String,
    custom_avatar: Option<ImageData>,
    #[default(String::new())]
    avatar_error: String,
    #[default(false)]
    profile_modal_open: bool,
    #[default(String::new())]
    profile_draft: String,
    #[default(false)]
    tasks_open: bool,
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
        self.set_tasks_open(false);
    }

    fn open_tasks_view(&self) {
        self.set_tasks_open(true);
    }

    fn back_to_overview(&self) {
        self.set_tasks_open(false);
        self.set_active_tab("overview".to_owned());
    }

    fn all_tasks(&self) -> Vec<String> {
        let mut tasks: Vec<String> = DEFAULT_TASKS
            .iter()
            .map(|task| (*task).to_owned())
            .collect();
        tasks.extend(self.tasks());
        tasks
    }

    fn is_overview(&self) -> bool {
        self.active_tab() == "overview"
    }

    fn show_overview_content(&self) -> bool {
        self.is_overview() && !self.tasks_open()
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

    fn pick_avatar(&self) {
        let Some(path) = light::file_dialog::pick_image_file() else {
            return;
        };
        match load_avatar(&path) {
            Ok(image) => {
                self.set_custom_avatar(Some(image));
                self.set_avatar_error(String::new());
            }
            Err(error) => self.set_avatar_error(error),
        }
    }

    fn open_profile_modal(&self) {
        self.set_profile_draft(self.profile_name());
        self.set_avatar_error(String::new());
        self.set_profile_modal_open(true);
    }

    fn close_profile_modal(&self) {
        self.set_profile_modal_open(false);
        self.set_profile_draft(String::new());
    }

    fn is_profile_draft_empty(&self) -> bool {
        self.profile_draft().trim().is_empty()
    }

    fn has_avatar_error(&self) -> bool {
        !self.avatar_error().is_empty()
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

fn load_avatar(path: &Path) -> Result<ImageData, String> {
    let metadata =
        fs::metadata(path).map_err(|error| format!("Не удалось открыть файл: {error}"))?;
    if metadata.len() > MAX_AVATAR_BYTES {
        return Err(String::from("Файл больше 2 МБ"));
    }
    let bytes = fs::read(path).map_err(|error| format!("Не удалось прочитать файл: {error}"))?;
    ImageData::from_png_limited(bytes, MAX_AVATAR_BYTES as usize, MAX_AVATAR_DIMENSION)
        .map_err(|error| format!("Не удалось загрузить PNG: {error}"))
}

fn main() -> light::Result<()> {
    light::run(Dashboard::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_avatar_passes_validation() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/avatar.png");
        let image = load_avatar(&path).expect("встроенный avatar.png");
        assert!(image.width() > 0);
        assert!(image.height() > 0);
    }
}
