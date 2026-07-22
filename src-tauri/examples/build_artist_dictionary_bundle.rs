use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::PathBuf;

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use serde::Deserialize;
use smart_spreadsheet_lib::db::{
    ArtistDictionaryInput, BundledArtistDictionaryHeader, build_artist_dictionary,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSnapshot {
    snapshot_at: String,
    dictionary: ArtistDictionaryInput,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1).map(PathBuf::from);
    let input_path = arguments.next().ok_or("missing input snapshot path")?;
    let output_path = arguments.next().ok_or("missing output bundle path")?;
    if arguments.next().is_some() {
        return Err("expected exactly two paths".into());
    }

    let input = File::open(&input_path)?;
    let snapshot =
        serde_json::from_reader::<_, RawSnapshot>(BufReader::new(GzDecoder::new(input)))?;
    let tag_count = u64::try_from(snapshot.dictionary.tags.len())?;
    let artist_count = u64::try_from(snapshot.dictionary.artists.len())?;
    let alias_count = u64::try_from(snapshot.dictionary.aliases.len())?;
    let entries = build_artist_dictionary(&snapshot.dictionary);
    let header = BundledArtistDictionaryHeader {
        snapshot_at: snapshot.snapshot_at,
        tag_count,
        artist_count,
        alias_count,
        name_count: u64::try_from(entries.len())?,
    };

    let output = File::create(&output_path)?;
    let encoder = GzEncoder::new(BufWriter::new(output), Compression::best());
    let mut writer = BufWriter::new(encoder);
    serde_json::to_writer(&mut writer, &header)?;
    writer.write_all(b"\n")?;
    for entry in entries {
        serde_json::to_writer(&mut writer, &entry)?;
        writer.write_all(b"\n")?;
    }
    writer.flush()?;
    let encoder = writer.into_inner()?.finish()?;
    encoder.into_inner()?.sync_all()?;
    Ok(())
}
