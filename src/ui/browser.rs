use relm4::gtk::gio;
use std::{path::Path, process::Command};

pub fn open_login_url(url: &str) -> anyhow::Result<()> {
    let mut errors = Vec::new();
    if let Some(browser) = std::env::var_os("BROWSER") {
        let browser = browser.to_string_lossy();
        for command in browser.split(':').filter(|command| !command.is_empty()) {
            if let Err(error) = spawn_browser_shell(command, url) {
                errors.push(format!("{command}: {error}"));
            } else {
                return Ok(());
            }
        }
    }

    for (command, args) in [
        ("xdg-open", vec![url]),
        ("gio", vec!["open", url]),
        ("kioclient", vec!["exec", url]),
        ("kioclient5", vec!["exec", url]),
        ("kde-open5", vec![url]),
        ("kde-open", vec![url]),
        ("exo-open", vec![url]),
        ("gvfs-open", vec![url]),
        ("sensible-browser", vec![url]),
    ] {
        if let Err(error) = spawn_browser_command(command, &args) {
            errors.push(format!("{command}: {error}"));
        } else {
            return Ok(());
        }
    }

    for command in [
        "firefox",
        "firefox-esr",
        "librewolf",
        "chromium",
        "google-chrome",
    ] {
        if let Err(error) = spawn_browser_command(command, &[url]) {
            errors.push(format!("{command}: {error}"));
        } else {
            return Ok(());
        }
    }

    if let Err(error) = gio::AppInfo::launch_default_for_uri(url, None::<&gio::AppLaunchContext>) {
        errors.push(format!("gio: {error}"));
    } else {
        return Ok(());
    }

    anyhow::bail!("could not open a browser ({})", errors.join("; "))
}

pub fn shell_quote_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn spawn_browser_command(command: &str, args: &[&str]) -> std::io::Result<()> {
    Command::new(command).args(args).spawn().map(|_| ())
}

fn spawn_browser_shell(command: &str, url: &str) -> std::io::Result<()> {
    Command::new("sh")
        .args(["-c", &format!("exec {command} \"$1\""), "sh", url])
        .spawn()
        .map(|_| ())
}
