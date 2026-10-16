use clap::Parser;

mod cmd_check;
mod cmd_suggest;

#[derive(Parser)]
#[command(name = "relay", version, about = "Менеджер компонентов Relay")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Проверить проект: используемые и отсутствующие компоненты.
    Check(cmd_check::CheckArgs),
    /// Подсказать, какие компоненты добавить.
    Suggest(cmd_suggest::SuggestArgs),
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
        Command::Check(args) => cmd_check::run(args),
        Command::Suggest(args) => cmd_suggest::run(args),
    };
    match result {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
