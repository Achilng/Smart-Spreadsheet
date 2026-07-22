use std::path::PathBuf;
use std::time::Instant;

use smart_spreadsheet_lib::db::Database;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("missing artist dictionary bundle path")?;
    let started = Instant::now();
    let mut database = Database::open_in_memory()?;
    let status = database.install_bundled_artist_dictionary(path)?;
    let parsley = database.artist_dictionary_entries_by_names(["parsley-f", "parsley_f"])?;
    if parsley.len() != 2 || parsley.iter().any(|entry| !entry.is_banned) {
        return Err("bundled parsley-f identity check failed".into());
    }
    println!(
        "installed {} names in {:.2?}; parsley-f identity verified",
        status.name_count,
        started.elapsed()
    );
    Ok(())
}
