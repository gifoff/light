use clap::Parser;

mod cmd_add;
mod cmd_check;
mod cmd_doctor;
mod cmd_init;
mod cmd_lock;
mod cmd_publish;
mod cmd_remove;
mod cmd_suggest;
mod cmd_update;
mod cmd_vendor;

#[derive(Parser)]
#[command(name = "relay", version, about = "Менеджер компонентов Relay")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Создать скелет нового проекта Relay.
    Init(cmd_init::InitArgs),
    /// Проверить проект: используемые и отсутствующие компоненты.
    Check(cmd_check::CheckArgs),
    /// Подсказать, какие компоненты добавить.
    Suggest(cmd_suggest::SuggestArgs),
    /// Диагностика окружения и проекта.
    Doctor(cmd_doctor::DoctorArgs),
    /// Установить компонент в проект.
    Add(cmd_add::AddArgs),
    /// Удалить компонент из проекта.
    Remove(cmd_remove::RemoveArgs),
    /// Сгенерировать или обновить `relay.lock`.
    Lock(cmd_lock::LockArgs),
    /// Обновить компоненты до актуальных версий.
    Update(cmd_update::UpdateArgs),
    /// Скопировать исходники компонентов в `vendor/`.
    Vendor(cmd_vendor::VendorArgs),
    /// Опубликовать крейты воркспейса в crates.io.
    Publish(cmd_publish::PublishArgs),
}

/// Формат вывода отчёта.
#[derive(Clone, Copy, clap::ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Init(args) => cmd_init::run(args),
        Command::Check(args) => cmd_check::run(args),
        Command::Suggest(args) => cmd_suggest::run(args),
        Command::Doctor(args) => cmd_doctor::run(args),
        Command::Add(args) => cmd_add::run(args),
        Command::Remove(args) => cmd_remove::run(args),
        Command::Lock(args) => cmd_lock::run(args),
        Command::Update(args) => cmd_update::run(args),
        Command::Vendor(args) => cmd_vendor::run(args),
        Command::Publish(args) => cmd_publish::run(args),
    };
    match result {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
