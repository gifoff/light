use light_fw_cli_core::fetch::CratesIoRegistry;
use light_fw_cli_core::search;

/// Аргументы команды `light search`.
#[derive(clap::Args)]
pub struct SearchArgs {
    /// Поисковый запрос (например, `datepicker`).
    pub query: String,
}

/// Ищет компоненты Light на crates.io.
pub fn run(args: SearchArgs) -> anyhow::Result<i32> {
    let results = search::search(&CratesIoRegistry, &args.query)?;
    if results.is_empty() {
        println!("No components found matching \"{}\"", args.query);
        return Ok(0);
    }
    print!("{}", search::render_table(&results));
    Ok(0)
}
