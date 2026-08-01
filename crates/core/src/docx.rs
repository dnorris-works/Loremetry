//! DOCX → markdown conversion at ingest (pandoc).

use std::process::Command;

pub fn convert_docx_to_markdown(bytes: &[u8]) -> Result<String, String> {
    if bytes.is_empty() {
        return Err("Empty DOCX file".into());
    }
    let tmp_dir = std::env::temp_dir();
    let stem = uuid::Uuid::new_v4().to_string();
    let docx_path = tmp_dir.join(format!("loremetry-{stem}.docx"));
    let md_path = tmp_dir.join(format!("loremetry-{stem}.md"));

    if let Err(e) = std::fs::write(&docx_path, bytes) {
        return Err(format!("Could not write temp DOCX: {e}"));
    }

    let output = Command::new("pandoc")
        .args([
            "-f",
            "docx",
            "-t",
            "markdown",
            "--wrap=none",
            docx_path.to_str().unwrap_or(""),
            "-o",
            md_path.to_str().unwrap_or(""),
        ])
        .output();

    let _ = std::fs::remove_file(&docx_path);

    match output {
        Ok(out) if out.status.success() => {
            let md = std::fs::read_to_string(&md_path).unwrap_or_default();
            let _ = std::fs::remove_file(&md_path);
            if md.trim().is_empty() {
                return Err("DOCX conversion produced empty content".into());
            }
            Ok(md)
        }
        Ok(out) => {
            let _ = std::fs::remove_file(&md_path);
            let stderr = String::from_utf8_lossy(&out.stderr);
            Err(format!(
                "pandoc failed: {}",
                stderr.trim().lines().last().unwrap_or("unknown error")
            ))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Err("pandoc is not installed on the server — DOCX upload unavailable".into())
        }
        Err(e) => Err(format!("Could not run pandoc: {e}")),
    }
}

/// Rewrite .docx filename to .md for canonical storage identity.
pub fn docx_to_md_filename(filename: &str) -> String {
    let lower = filename.to_lowercase();
    if lower.ends_with(".docx") {
        let stem = &filename[..filename.len().saturating_sub(5)];
        format!("{stem}.md")
    } else {
        filename.to_string()
    }
}
