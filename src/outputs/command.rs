use super::Package;
use crate::config::CommandOutput;
use crate::template::expand_path;
use anyhow::{Context, Result, bail};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

/// Environment passed to the command. Package data never becomes part of the command line.
fn environment(package: &Package, index: Option<usize>) -> Vec<(String, String)> {
    let mut env = vec![
        ("KLIKNLOAD_PACKAGE", package.name.clone()),
        ("KLIKNLOAD_LINKS", package.links.join("\n")),
        ("KLIKNLOAD_COUNT", package.links.len().to_string()),
        (
            "KLIKNLOAD_PASSWORD",
            package.password.clone().unwrap_or_default(),
        ),
        (
            "KLIKNLOAD_SOURCE",
            package.source.clone().unwrap_or_default(),
        ),
        ("KLIKNLOAD_HOST", package.host()),
        (
            "KLIKNLOAD_DATE",
            package.received.format("%Y-%m-%d").to_string(),
        ),
        (
            "KLIKNLOAD_TIME",
            package.received.format("%H-%M-%S").to_string(),
        ),
    ];
    if let Some(i) = index {
        env.push(("KLIKNLOAD_LINK", package.links[i].clone()));
        env.push(("KLIKNLOAD_INDEX", (i + 1).to_string()));
    }
    let json = serde_json::json!({
        "package": package.name, "links": package.links, "password": package.password, "source": package.source,
    });
    env.push(("KLIKNLOAD_JSON", json.to_string()));
    env.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

/// Apps started from Finder get a minimal PATH; add the usual Homebrew locations.
#[cfg(not(windows))]
fn path_env() -> String {
    let current = std::env::var("PATH").unwrap_or_default();
    let mut parts: Vec<&str> = current.split(':').filter(|p| !p.is_empty()).collect();
    for extra in [
        "/opt/homebrew/bin",
        "/usr/local/bin",
        "/usr/bin",
        "/bin",
        "/usr/sbin",
        "/sbin",
    ] {
        if !parts.contains(&extra) {
            parts.push(extra);
        }
    }
    parts.join(":")
}

async fn run_once(cfg: &CommandOutput, package: &Package, index: Option<usize>) -> Result<String> {
    #[cfg(windows)]
    let mut cmd = {
        let mut c = tokio::process::Command::new("cmd");
        c.arg("/C").arg(&cfg.command);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = tokio::process::Command::new("/bin/sh");
        c.arg("-c").arg(&cfg.command).env("PATH", path_env());
        c
    };
    cmd.envs(environment(package, index))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if !cfg.working_dir.trim().is_empty() {
        cmd.current_dir(expand_path(cfg.working_dir.trim()));
    }

    let mut child = cmd.spawn().with_context(|| t!(CmdStartFailed))?;
    if let Some(mut stdin) = child.stdin.take() {
        let input = match index {
            Some(i) => package.links[i].clone() + "\n",
            None => package.links.join("\n") + "\n",
        };
        // The command may not read stdin at all; that is fine.
        let _ = stdin.write_all(input.as_bytes()).await;
    }
    let out = tokio::time::timeout(
        Duration::from_secs(cfg.timeout_secs.max(1)),
        child.wait_with_output(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("{}", t!(CmdTimeout, secs = cfg.timeout_secs)))??;

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    if !out.status.success() {
        let detail = stderr
            .trim()
            .lines()
            .last()
            .or_else(|| stdout.trim().lines().last())
            .unwrap_or("");
        bail!("{}", t!(CmdExit, status = out.status, detail = detail));
    }
    Ok(stdout.trim().lines().last().unwrap_or("").to_string())
}

pub async fn deliver(cfg: &CommandOutput, package: &Package) -> Result<String> {
    if cfg.per_link {
        let mut last = String::new();
        for i in 0..package.links.len() {
            last = run_once(cfg, package, Some(i)).await?;
        }
        Ok(format!(
            "{}{}",
            t!(CmdOkMany, count = package.links.len()),
            if last.is_empty() {
                String::new()
            } else {
                format!(": {last}")
            }
        ))
    } else {
        let out = run_once(cfg, package, None).await?;
        Ok(if out.is_empty() {
            t!(CmdOk)
        } else {
            t!(CmdOkOutput, output = out)
        })
    }
}

pub fn preview(cfg: &CommandOutput, package: &Package) -> Result<String> {
    let runs = if cfg.per_link {
        t!(CmdRunsPerLink, count = package.links.len())
    } else {
        t!(CmdRunsOne)
    };
    let mut out = format!(
        "$ {}\n{}\n",
        cfg.command.trim(),
        t!(CmdPreview, runs = runs)
    );
    let index = cfg.per_link.then_some(0);
    for (k, v) in environment(package, index) {
        if k == "KLIKNLOAD_JSON" {
            continue;
        }
        out.push_str(&format!("{k}={}\n", v.replace('\n', "\\n")));
    }
    out.push_str("KLIKNLOAD_JSON={…}");
    Ok(out)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn passes_data_via_env_and_stdin() {
        let cfg = CommandOutput {
            command: "read first; echo \"$KLIKNLOAD_COUNT|$KLIKNLOAD_PACKAGE|$first\"".into(),
            working_dir: String::new(),
            per_link: false,
            timeout_secs: 5,
        };
        let mut p = Package::sample();
        p.name = "$(touch /tmp/kliknload-pwned); `id`".into();
        let msg = deliver(&cfg, &p).await.unwrap();
        assert!(msg.ends_with("2|$(touch /tmp/kliknload-pwned); `id`|https://hoster.example/file/abc123/Example.part1.rar"), "{msg}");
    }

    #[tokio::test]
    async fn reports_failures_and_timeouts() {
        let mut cfg = CommandOutput {
            command: "echo boom >&2; exit 3".into(),
            working_dir: String::new(),
            per_link: true,
            timeout_secs: 5,
        };
        let err = deliver(&cfg, &Package::sample())
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("boom"), "{err}");
        cfg.command = "sleep 5".into();
        cfg.timeout_secs = 1;
        assert!(
            deliver(&cfg, &Package::sample())
                .await
                .unwrap_err()
                .to_string()
                .contains("timed out")
        );
    }
}
