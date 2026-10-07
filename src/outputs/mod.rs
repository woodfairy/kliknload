//! Outputs: where received packages go.

mod clipboard;
mod command;
mod file;
mod http;
pub mod pyload;

pub use http::presets as http_presets;

use crate::config::{Output, OutputKind};
use crate::template::{Value, Vars};
use anyhow::{Result, bail};
use chrono::{DateTime, Local};

#[derive(Debug, Clone)]
pub struct Package {
    pub name: String,
    pub links: Vec<String>,
    pub password: Option<String>,
    pub source: Option<String>,
    pub received: DateTime<Local>,
}

impl Package {
    pub fn new(
        name: String,
        links: Vec<String>,
        password: Option<String>,
        source: Option<String>,
    ) -> Self {
        Self {
            name,
            links,
            password,
            source,
            received: Local::now(),
        }
    }

    /// Example data for previews and tests in the settings window.
    pub fn sample() -> Self {
        Self::new(
            "Example.Package.2026".into(),
            vec![
                "https://hoster.example/file/abc123/Example.part1.rar".into(),
                "https://hoster.example/file/def456/Example.part2.rar".into(),
            ],
            Some("secret".into()),
            Some("https://release.example/example-package".into()),
        )
    }

    pub fn host(&self) -> String {
        self.source
            .as_deref()
            .and_then(|s| reqwest::Url::parse(s).ok())
            .and_then(|u| u.host_str().map(str::to_string))
            .unwrap_or_default()
    }

    /// Template variables for this package. `link`/`index` are set per link.
    pub fn vars(&self) -> Vars {
        let mut v = Vars::new();
        let s = |x: &str| Value::Str(x.to_string());
        v.insert("package", s(&self.name));
        v.insert("links", Value::List(self.links.clone()));
        v.insert(
            "link",
            s(self.links.first().map(String::as_str).unwrap_or("")),
        );
        v.insert("index", s("1"));
        v.insert("count", s(&self.links.len().to_string()));
        v.insert("password", s(self.password.as_deref().unwrap_or("")));
        v.insert("source", s(self.source.as_deref().unwrap_or("")));
        v.insert("host", s(&self.host()));
        v.insert("date", s(&self.received.format("%Y-%m-%d").to_string()));
        v.insert("time", s(&self.received.format("%H-%M-%S").to_string()));
        v.insert(
            "datetime",
            s(&self
                .received
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, false)),
        );
        v.insert("timestamp", s(&self.received.timestamp().to_string()));
        v
    }

    pub fn vars_for_link(&self, index: usize) -> Vars {
        let mut v = self.vars();
        v.insert("link", Value::Str(self.links[index].clone()));
        v.insert("index", Value::Str((index + 1).to_string()));
        v
    }
}

/// Applies the include/exclude filters of an output.
fn filtered(output: &Output, package: &Package) -> Result<Package> {
    let compile = |re: &Option<String>| -> Result<Option<regex::Regex>> {
        Ok(
            match re.as_deref().map(str::trim).filter(|r| !r.is_empty()) {
                Some(r) => Some(regex::Regex::new(r)?),
                None => None,
            },
        )
    };
    let (include, exclude) = (compile(&output.include)?, compile(&output.exclude)?);
    let mut p = package.clone();
    p.links.retain(|l| {
        include.as_ref().is_none_or(|re| re.is_match(l))
            && !exclude.as_ref().is_some_and(|re| re.is_match(l))
    });
    Ok(p)
}

/// Outcome of one output for one package.
#[derive(Debug, Clone)]
pub enum Outcome {
    Done(String),
    /// Nothing to do, e.g. all links filtered out.
    Skipped(String),
}

/// Sends a package to an output.
pub async fn deliver(output: &Output, package: &Package) -> Result<Outcome> {
    let p = filtered(output, package)?;
    if p.links.is_empty() {
        return Ok(Outcome::Skipped(t!(OutNoMatchingLinks)));
    }
    let msg = match &output.kind {
        OutputKind::Pyload(cfg) => pyload::deliver(cfg, &p).await?,
        OutputKind::Clipboard(cfg) => clipboard::deliver(cfg, &p)?,
        OutputKind::File(cfg) => file::deliver(cfg, &p).await?,
        OutputKind::Http(cfg) => http::deliver(cfg, &p).await?,
        OutputKind::Command(cfg) => command::deliver(cfg, &p).await?,
    };
    Ok(Outcome::Done(msg))
}

/// Describes what the output would do with `package`, without side effects.
pub fn preview(output: &Output, package: &Package) -> Result<String> {
    let p = filtered(output, package)?;
    if p.links.is_empty() {
        bail!("{}", t!(OutAllFiltered));
    }
    match &output.kind {
        OutputKind::Pyload(cfg) => pyload::preview(cfg, &p),
        OutputKind::Clipboard(cfg) => clipboard::preview(cfg, &p),
        OutputKind::File(cfg) => file::preview(cfg, &p),
        OutputKind::Http(cfg) => http::preview(cfg, &p),
        OutputKind::Command(cfg) => command::preview(cfg, &p),
    }
}

/// Tries the output for real. pyLoad only checks the login, everything else
/// runs with the sample package.
pub async fn test(output: &Output) -> Result<String> {
    match &output.kind {
        OutputKind::Pyload(cfg) => {
            let version = pyload::test_connection(cfg).await?;
            Ok(t!(PyloadTestOk, version = version))
        }
        _ => match deliver(output, &Package::sample()).await? {
            Outcome::Done(m) | Outcome::Skipped(m) => Ok(m),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ClipboardOutput;

    #[test]
    fn filters_links() {
        let mut o = Output::new(OutputKind::Clipboard(ClipboardOutput::default()), true);
        o.include = Some(r"part1".into());
        assert_eq!(filtered(&o, &Package::sample()).unwrap().links.len(), 1);
        o.include = None;
        o.exclude = Some(r"\.rar$".into());
        assert!(filtered(&o, &Package::sample()).unwrap().links.is_empty());
    }

    #[test]
    fn vars_contain_host_and_link() {
        let p = Package::sample();
        let v = p.vars_for_link(1);
        let get = |k| match &v[k] {
            Value::Str(s) => s.clone(),
            Value::List(l) => l.join(","),
        };
        assert_eq!(get("host"), "release.example");
        assert_eq!(get("index"), "2");
        assert!(get("link").ends_with("part2.rar"));
    }
}
