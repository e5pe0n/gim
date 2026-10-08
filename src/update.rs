//! `gim self-update`: replace the running binary with the latest GitHub release.

use self_update::backends::github::Update;

pub fn run() -> Result<(), String> {
    let status = Update::configure()
        .repo_owner("e5pe0n")
        .repo_name("gim")
        .bin_name("gim")
        .current_version(self_update::cargo_crate_version!())
        .show_download_progress(true)
        .build()
        .and_then(|u| u.update())
        .map_err(|e| e.to_string())?;
    if status.is_updated() {
        println!("updated to {}", status.version());
    } else {
        println!("already up to date ({})", status.version());
    }
    Ok(())
}
