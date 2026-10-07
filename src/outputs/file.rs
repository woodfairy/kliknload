use super::Package;
use crate::config::{FileFormat, FileMode, FileOutput, OnConflict};
use crate::template::{Escape, expand_path, render};
use anyhow::{Context, Result, bail};
use serde_json::json;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Target path: every `/`-separated part of the name template is rendered as a single,
/// sanitized file name, so package data can never escape the directory.
fn target_path(cfg: &FileOutput, package: &Package) -> Result<PathBuf> {
    let mut path = expand_path(cfg.directory.trim());
    let vars = package.vars();
    for segment in cfg
        .filename
        .split('/')
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let name = render(segment, &vars, Escape::Filename)?;
        if name == ".." || name == "." {
            bail!("ungültiger Dateiname „{name}“");
        }
        path.push(name);
    }
    if path == expand_path(cfg.directory.trim()) {
        bail!("Dateiname ist leer");
    }
    Ok(path)
}

fn package_json(package: &Package) -> serde_json::Value {
    json!({
        "package": package.name,
        "links": package.links,
        "password": package.password,
        "source": package.source,
        "receivedAt": package.received.to_rfc3339(),
    })
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn one_line(s: &str) -> String {
    s.replace(['\r', '\n'], " ")
}

fn content(cfg: &FileOutput, package: &Package, new_file: bool) -> Result<String> {
    let append = cfg.mode == FileMode::Append;
    Ok(match cfg.format {
        FileFormat::Txt => package.links.join("\n") + "\n",
        FileFormat::Json if append => serde_json::to_string(&package_json(package))? + "\n",
        FileFormat::Json => serde_json::to_string_pretty(&package_json(package))? + "\n",
        FileFormat::Csv => {
            let mut out = String::new();
            if new_file {
                out.push_str("package,link,password,source,received\n");
            }
            for link in &package.links {
                let row = [
                    package.name.as_str(),
                    link,
                    package.password.as_deref().unwrap_or(""),
                    package.source.as_deref().unwrap_or(""),
                    &package.received.to_rfc3339(),
                ];
                out.push_str(&row.map(csv_field).join(","));
                out.push('\n');
            }
            out
        }
        FileFormat::Crawljob => {
            let mut out = String::new();
            for link in &package.links {
                out.push_str(&format!(
                    "text={}\npackageName={}\n",
                    one_line(link),
                    one_line(&package.name)
                ));
                if let Some(pw) = package.password.as_deref().filter(|p| !p.is_empty()) {
                    out.push_str(&format!(
                        "extractPasswords={}\n",
                        serde_json::to_string(&[one_line(pw)])?
                    ));
                }
                out.push_str("enabled=TRUE\nautoStart=TRUE\nautoConfirm=TRUE\n\n");
            }
            out
        }
        FileFormat::Custom => {
            let mut text = render(&cfg.template, &package.vars(), Escape::None)?;
            if !text.ends_with('\n') {
                text.push('\n');
            }
            text
        }
    })
}

fn numbered(path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    (2..)
        .map(|n| path.with_file_name(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .expect("some free name")
}

pub async fn deliver(cfg: &FileOutput, package: &Package) -> Result<String> {
    let mut path = target_path(cfg, package)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("Ordner {} anlegen", dir.display()))?;
    }

    if cfg.mode == FileMode::Append {
        let new_file = std::fs::metadata(&path)
            .map(|m| m.len() == 0)
            .unwrap_or(true);
        let text = content(cfg, package, new_file)?;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("{} öffnen", path.display()))?;
        f.write_all(text.as_bytes())?;
        return Ok(format!("an {} angehängt", path.display()));
    }

    if path.exists() {
        match cfg.on_conflict {
            OnConflict::Skip => {
                return Ok(format!(
                    "{} existiert bereits, übersprungen",
                    path.display()
                ));
            }
            OnConflict::Overwrite => {}
            OnConflict::Number => path = numbered(&path),
        }
    }
    let text = content(cfg, package, true)?;
    std::fs::write(&path, text).with_context(|| format!("{} schreiben", path.display()))?;
    Ok(format!("gespeichert: {}", path.display()))
}

pub fn preview(cfg: &FileOutput, package: &Package) -> Result<String> {
    let path = target_path(cfg, package)?;
    let mode = match cfg.mode {
        FileMode::New => match cfg.on_conflict {
            OnConflict::Number => "neue Datei (bei Konflikt nummerieren)",
            OnConflict::Overwrite => "neue Datei (bei Konflikt überschreiben)",
            OnConflict::Skip => "neue Datei (bei Konflikt überspringen)",
        },
        FileMode::Append => "an Datei anhängen",
    };
    let new_file = cfg.mode == FileMode::New || !path.exists();
    Ok(format!(
        "Datei: {}\nModus: {mode}\n\n{}",
        path.display(),
        content(cfg, package, new_file)?
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(dir: &Path, format: FileFormat, mode: FileMode) -> FileOutput {
        FileOutput {
            directory: dir.display().to_string(),
            filename: "{{host}}/{{package}}.txt".into(),
            format,
            template: "{{package}}: {{links|comma}}".into(),
            mode,
            on_conflict: OnConflict::Number,
        }
    }

    #[tokio::test]
    async fn writes_numbered_files_and_stays_in_directory() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = Package::sample();
        p.name = "../../evil/name".into();
        let c = cfg(dir.path(), FileFormat::Txt, FileMode::New);
        deliver(&c, &p).await.unwrap();
        deliver(&c, &p).await.unwrap();
        let sub = dir.path().join("release.example");
        let mut names: Vec<_> = std::fs::read_dir(&sub)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        names.sort();
        assert_eq!(names, ["_.._evil_name (2).txt", "_.._evil_name.txt"]);
        let text = std::fs::read_to_string(sub.join("_.._evil_name.txt")).unwrap();
        assert_eq!(text.lines().count(), 2);
    }

    #[tokio::test]
    async fn appends_csv_with_single_header() {
        let dir = tempfile::tempdir().unwrap();
        let mut c = cfg(dir.path(), FileFormat::Csv, FileMode::Append);
        c.filename = "links.csv".into();
        deliver(&c, &Package::sample()).await.unwrap();
        deliver(&c, &Package::sample()).await.unwrap();
        let text = std::fs::read_to_string(dir.path().join("links.csv")).unwrap();
        assert_eq!(
            text.lines().filter(|l| l.starts_with("package,")).count(),
            1
        );
        assert_eq!(text.lines().count(), 5);
    }

    #[test]
    fn crawljob_and_custom_formats() {
        let dir = tempfile::tempdir().unwrap();
        let p = Package::sample();
        let job = content(
            &cfg(dir.path(), FileFormat::Crawljob, FileMode::New),
            &p,
            true,
        )
        .unwrap();
        assert_eq!(job.matches("text=https://").count(), 2);
        assert!(job.contains("extractPasswords=[\"geheim\"]"));
        let custom = content(
            &cfg(dir.path(), FileFormat::Custom, FileMode::New),
            &p,
            true,
        )
        .unwrap();
        assert!(custom.starts_with("Beispiel.Paket.2026: https://"));
    }
}
