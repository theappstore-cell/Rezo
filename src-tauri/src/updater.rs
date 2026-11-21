// Checks GitHub for a newer release and replaces the executable in its own folder after verifying the download
// against the release's SHA256SUMS.txt.
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};

const LATEST: &str = "https://api.github.com/repos/theappstore-cell/Rezo/releases/latest";
const RELEASES_PAGE: &str = "https://github.com/theappstore-cell/Rezo/releases/latest";

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub available: bool,
    pub page: String,
    pub asset: Option<String>,
    pub size: Option<u64>,
    pub can_install: bool,
    pub folder: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Progress {
    received: u64,
    total: u64,
    stage: &'static str,
}

struct Asset {
    name: String,
    url: String,
    size: u64,
}

fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(concat!("Rezo/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .build()?)
}

fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let mut it = v.trim().trim_start_matches('v').split(['.', '-', '+']).map(|p| p.parse::<u64>().ok());
    Some((it.next()??, it.next()??, it.next().flatten().unwrap_or(0)))
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

fn exe_path() -> Result<PathBuf> {
    Ok(std::env::current_exe()?.canonicalize().unwrap_or(std::env::current_exe()?))
}

fn folder_writable(dir: &Path) -> bool {
    let probe = dir.join(".rezo-write-test");
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

// Same kind of file as the one running: the portable executable replaces itself in place.
fn wanted_asset(version: &str) -> Option<String> {
    if cfg!(windows) {
        let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" };
        Some(format!("Rezo-{version}-windows-{arch}-portable.exe"))
    } else {
        None
    }
}

async fn latest_release(c: &reqwest::Client) -> Result<(String, Vec<Asset>)> {
    let body = c.get(LATEST).header("Accept", "application/vnd.github+json").send().await?.error_for_status()?.text().await?;
    let v: serde_json::Value = serde_json::from_str(&body)?;
    let tag = v["tag_name"].as_str().ok_or_else(|| anyhow!("réponse GitHub sans version"))?.trim_start_matches('v').to_string();
    let assets = v["assets"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    Some(Asset {
                        name: x["name"].as_str()?.to_string(),
                        url: x["browser_download_url"].as_str()?.to_string(),
                        size: x["size"].as_u64().unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok((tag, assets))
}

pub async fn check() -> Result<UpdateInfo> {
    let c = client()?;
    let (latest, assets) = latest_release(&c).await?;
    let current = env!("CARGO_PKG_VERSION").to_string();
    let exe = exe_path()?;
    let folder = exe.parent().map(|p| p.display().to_string().trim_start_matches(r"\\?\").to_string()).unwrap_or_default();
    let asset = wanted_asset(&latest).and_then(|n| assets.into_iter().find(|a| a.name == n));
    let can_install = asset.is_some() && exe.parent().map_or(false, folder_writable);
    Ok(UpdateInfo {
        available: is_newer(&latest, &current),
        current,
        latest,
        page: RELEASES_PAGE.into(),
        size: asset.as_ref().map(|a| a.size),
        asset: asset.map(|a| a.name),
        can_install,
        folder,
    })
}

fn expected_hash(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut parts = l.split_whitespace();
        let hash = parts.next()?;
        let file = parts.next()?.trim_start_matches('*');
        (file == name && hash.len() == 64).then(|| hash.to_ascii_lowercase())
    })
}

// Downloads next to the running executable, checks SHA-256, swaps the files and restarts on the new version.
pub async fn install(app: AppHandle) -> Result<()> {
    let c = client()?;
    let (latest, assets) = latest_release(&c).await?;
    if !is_newer(&latest, env!("CARGO_PKG_VERSION")) {
        bail!("Rezo est déjà à jour");
    }
    let name = wanted_asset(&latest).ok_or_else(|| anyhow!("mise à jour automatique indisponible sur ce système"))?;
    let asset = assets.iter().find(|a| a.name == name).ok_or_else(|| anyhow!("{name} absent de la version {latest}"))?;
    let sums = assets.iter().find(|a| a.name == "SHA256SUMS.txt").ok_or_else(|| anyhow!("SHA256SUMS.txt absent de la version {latest}"))?;
    let sums = c.get(&sums.url).send().await?.error_for_status()?.text().await?;
    let want = expected_hash(&sums, &name).ok_or_else(|| anyhow!("{name} absent de SHA256SUMS.txt"))?;

    let exe = exe_path()?;
    let dir = exe.parent().ok_or_else(|| anyhow!("dossier de Rezo introuvable"))?.to_path_buf();
    let partial = dir.join("Rezo.exe.download");

    let total = download_verified(&c, &asset.url, asset.size, &want, &partial, &mut |received, total| {
        let _ = app.emit("update-progress", Progress { received, total, stage: "download" });
    })
    .await?;
    let _ = app.emit("update-progress", Progress { received: total, total, stage: "verify" });
    swap(&exe, &partial)?;
    log::info!("mise à jour {} -> {latest} installée dans {}", env!("CARGO_PKG_VERSION"), dir.display());
    let _ = app.emit("update-progress", Progress { received: total, total, stage: "restart" });

    std::process::Command::new(&exe).env("REZO_WAIT_PID", std::process::id().to_string()).spawn().context("redémarrage")?;
    app.exit(0);
    Ok(())
}

// Streams `url` to `dest`, reporting progress every 100 ms; the file is deleted unless its SHA-256 is `want`.
async fn download_verified(
    c: &reqwest::Client,
    url: &str,
    size_hint: u64,
    want: &str,
    dest: &Path,
    progress: &mut (dyn FnMut(u64, u64) + Send),
) -> Result<u64> {
    use tokio::io::AsyncWriteExt;
    let mut resp = c.get(url).send().await?.error_for_status()?;
    let total = resp.content_length().unwrap_or(size_hint);
    let mut file = tokio::fs::File::create(dest).await.context("écriture dans le dossier de Rezo")?;
    let mut hasher = Sha256::new();
    let mut received = 0u64;
    let mut last = Instant::now();
    let result: Result<()> = async {
        while let Some(chunk) = resp.chunk().await? {
            hasher.update(&chunk);
            file.write_all(&chunk).await?;
            received += chunk.len() as u64;
            if last.elapsed() >= Duration::from_millis(100) {
                last = Instant::now();
                progress(received, total);
            }
        }
        file.flush().await?;
        Ok(())
    }
    .await;
    drop(file);
    let got = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect::<String>();
    if let Err(e) = result {
        let _ = std::fs::remove_file(dest);
        return Err(e);
    }
    progress(received, total);
    if got != want {
        let _ = std::fs::remove_file(dest);
        bail!("le fichier téléchargé ne correspond pas à SHA256SUMS.txt : mise à jour annulée");
    }
    Ok(received)
}

// Windows lets a running executable be renamed, not overwritten: the old one moves aside and is deleted at the
// next start.
fn swap(exe: &Path, new: &Path) -> Result<()> {
    let old = exe.with_extension("exe.old");
    let _ = std::fs::remove_file(&old);
    std::fs::rename(exe, &old).context("impossible de remplacer l'exécutable")?;
    if let Err(e) = std::fs::rename(new, exe) {
        let _ = std::fs::rename(&old, exe);
        return Err(anyhow!("impossible d'installer la nouvelle version ({e})"));
    }
    Ok(())
}

// Called first thing at start-up.
pub fn after_update() {
    if let Ok(pid) = std::env::var("REZO_WAIT_PID").map(|p| p.parse::<u32>().unwrap_or(0)) {
        wait_for_exit(pid);
        std::env::remove_var("REZO_WAIT_PID");
    }
    if let Ok(exe) = exe_path() {
        let _ = std::fs::remove_file(exe.with_extension("exe.old"));
        if let Some(dir) = exe.parent() {
            let _ = std::fs::remove_file(dir.join("Rezo.exe.download"));
        }
    }
}

#[cfg(windows)]
fn wait_for_exit(pid: u32) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};
    if pid == 0 {
        return;
    }
    unsafe {
        if let Ok(h) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            let _ = WaitForSingleObject(h, 15_000);
            let _ = CloseHandle(h);
        }
    }
}

#[cfg(not(windows))]
fn wait_for_exit(_pid: u32) {
    std::thread::sleep(Duration::from_secs(2));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_numerically() {
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(is_newer("v0.3.0", "0.2.0"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.1.9", "0.2.0"));
        assert!(!is_newer("nimportequoi", "0.2.0"));
    }

    #[test]
    fn swap_keeps_the_old_file_aside() {
        let dir = std::env::temp_dir().join(format!("rezo-swap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("Rezo.exe");
        let new = dir.join("Rezo.exe.download");
        std::fs::write(&exe, b"old").unwrap();
        std::fs::write(&new, b"new").unwrap();
        swap(&exe, &new).unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"new");
        assert_eq!(std::fs::read(dir.join("Rezo.exe.old")).unwrap(), b"old");
        assert!(!new.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[ignore]
    fn live_check_and_verified_download() {
        // cargo test -- --ignored live_check: talks to GitHub.
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let info = check().await.unwrap();
            println!("PROBE {info:?}");
            let c = client().unwrap();
            let (latest, assets) = latest_release(&c).await.unwrap();
            let name = wanted_asset(&latest).unwrap();
            let asset = assets.iter().find(|a| a.name == name).unwrap();
            let sums_url = &assets.iter().find(|a| a.name == "SHA256SUMS.txt").unwrap().url;
            let sums = c.get(sums_url).send().await.unwrap().text().await.unwrap();
            let want = expected_hash(&sums, &name).unwrap();
            let dest = std::env::temp_dir().join("rezo-update-probe.exe");
            let mut calls = 0;
            let n = download_verified(&c, &asset.url, asset.size, &want, &dest, &mut |_, _| calls += 1).await.unwrap();
            println!("PROBE téléchargé {n} o, {calls} rappels de progression, somme vérifiée");
            assert!(download_verified(&c, &asset.url, asset.size, &"0".repeat(64), &dest, &mut |_, _| {}).await.is_err());
            assert!(!dest.exists(), "un fichier à la mauvaise somme doit être supprimé");
            println!("PROBE fichier à somme fausse refusé et supprimé");
        });
    }

    #[test]
    fn checksum_line_is_found() {
        let sums = "aa  other.exe\n0123456789abcdef0123456789abcdef0123456789abcdef0123456789ABCDEF  Rezo-0.3.0-windows-x64-portable.exe\n";
        assert_eq!(
            expected_hash(sums, "Rezo-0.3.0-windows-x64-portable.exe").as_deref(),
            Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
        );
        assert_eq!(expected_hash(sums, "missing.exe"), None);
    }
}
