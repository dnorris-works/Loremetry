//! Export story assets + reports as a zip with synthetic folder layout.

use std::io::{Cursor, Write};

use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::assets::slot_folder;
use crate::db::{self, Db};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ManifestEntry {
    pub filename:   String,
    pub slot:       String,
    pub sort_order: i32,
}

pub struct StoryZipExport {
    pub bytes:    Vec<u8>,
    pub filename: String,
}

pub async fn export_story_zip(db: &Db, story_id: &str, story_name: &str) -> Result<StoryZipExport, String> {
    if !crate::stories::story_exists(db, story_id).await {
        return Err("Story not found.".into());
    }

    let assets = crate::assets::list_assets(&db.pool, story_id).await?;
    let reports = db::list_documents(&db.pool, story_id).await;

    let buf = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(buf);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let mut manifest: Vec<ManifestEntry> = Vec::new();

    for asset in &assets {
        let folder = slot_folder(&asset.slot);
        let zip_path = format!("{folder}/{}", asset.filename);
        zip.start_file(zip_path, opts).map_err(|e| e.to_string())?;
        zip.write_all(asset.content.as_bytes()).map_err(|e| e.to_string())?;
        manifest.push(ManifestEntry {
            filename:   asset.filename.clone(),
            slot:       asset.slot.clone(),
            sort_order: asset.sort_order,
        });
    }

    for doc in &reports {
        if let Ok(envelope) = db::get_report_cmd(db, doc.id).await {
            let safe_type = doc.doc_type.replace('/', "_");
            let zip_path = format!(
                "Reports/{}_{}.md",
                safe_type,
                doc.generated_at.replace(':', "-")
            );
            zip.start_file(zip_path, opts).map_err(|e| e.to_string())?;
            zip.write_all(envelope.content.as_bytes()).map_err(|e| e.to_string())?;
        }
    }

    let manifest_json = serde_json::to_string_pretty(&manifest).unwrap_or_else(|_| "[]".into());
    zip.start_file("manifest.json", opts).map_err(|e| e.to_string())?;
    zip.write_all(manifest_json.as_bytes()).map_err(|e| e.to_string())?;

    let cursor = zip.finish().map_err(|e| e.to_string())?;
    let bytes = cursor.into_inner();

    let safe_name = story_name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect::<String>();
    let filename = if safe_name.is_empty() {
        format!("{story_id}.zip")
    } else {
        format!("{safe_name}.zip")
    };

    Ok(StoryZipExport { bytes, filename })
}

pub fn parse_manifest(bytes: &[u8]) -> Result<Vec<ManifestEntry>, String> {
    serde_json::from_slice(bytes).map_err(|e| format!("Invalid manifest.json: {e}"))
}

pub fn read_zip_entries(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| format!("Invalid zip: {e}"))?;
    let mut out = Vec::new();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        if file.is_dir() || file.name().ends_with('/') {
            continue;
        }
        let name = file.name().replace('\\', "/");
        if name == "manifest.json" || name.ends_with("/manifest.json") {
            continue;
        }
        let mut data = Vec::new();
        std::io::copy(&mut file, &mut data).map_err(|e| e.to_string())?;
        out.push((name, data));
    }
    Ok(out)
}

pub fn read_zip_manifest(bytes: &[u8]) -> Option<Vec<ManifestEntry>> {
    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).ok()?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).ok()?;
        let name = file.name().replace('\\', "/");
        if name == "manifest.json" || name.ends_with("/manifest.json") {
            let mut data = Vec::new();
            if std::io::copy(&mut file, &mut data).is_ok() {
                return parse_manifest(&data).ok();
            }
        }
    }
    None
}
