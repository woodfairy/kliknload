use super::Package;
use crate::config::ClipboardOutput;
use crate::platform;
use crate::template::{Escape, render};
use anyhow::Result;

fn content(cfg: &ClipboardOutput, package: &Package) -> Result<String> {
    let template = if cfg.template.trim().is_empty() {
        "{{links}}"
    } else {
        &cfg.template
    };
    render(template, &package.vars(), Escape::None)
}

pub fn deliver(cfg: &ClipboardOutput, package: &Package) -> Result<String> {
    platform::copy_to_clipboard(&content(cfg, package)?)?;
    Ok(format!("{} Link(s) kopiert", package.links.len()))
}

pub fn preview(cfg: &ClipboardOutput, package: &Package) -> Result<String> {
    Ok(format!(
        "In die Zwischenablage:\n\n{}",
        content(cfg, package)?
    ))
}
