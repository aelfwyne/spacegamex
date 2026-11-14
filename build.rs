//! Stamps the binary with its build time and git commit, so the game can
//! show exactly which build is running (title screen and top bar).
//!
//! For Windows targets it also embeds the .exe icon and version info. That is
//! keyed on the *target* (CARGO_CFG_TARGET_OS), not `cfg(windows)`, because
//! this script runs on the host when cross-compiling from Linux.

use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// "YYYY-MM-DD HH:MM UTC" without external tools (fallback when `date` is
/// unavailable, e.g. a native Windows build).
fn utc_stamp() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02} UTC", rem / 3600, rem % 3600 / 60)
}

fn windows_resources() {
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let ico_path = out_dir.join("icon.ico");

    let png = std::io::BufReader::new(std::fs::File::open("assets/icon.png").expect("open assets/icon.png"));
    let image = ico::IconImage::read_png(png).expect("decode assets/icon.png");
    let mut dir = ico::IconDir::new(ico::ResourceType::Icon);
    dir.add_entry(ico::IconDirEntry::encode(&image).expect("encode icon"));
    dir.write(std::fs::File::create(&ico_path).expect("create icon.ico")).expect("write icon.ico");

    let mut res = winresource::WindowsResource::new();
    res.set_icon(ico_path.to_str().expect("utf-8 OUT_DIR"))
        .set("ProductName", "Colonia Galactica")
        .set("FileDescription", "Colonia Galactica")
        .set("InternalName", "colonia-galactica");
    res.compile().expect("compile Windows resources (is mingw-w64 / windres installed?)");
}

fn main() {
    let stamp = run("date", &["+%Y-%m-%d %H:%M"]).unwrap_or_else(utc_stamp);
    let mut git = run("git", &["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "nogit".into());
    // Uncommitted changes in the build get a "+".
    if run("git", &["status", "--porcelain", "--untracked-files=no"]).is_some_and(|s| !s.is_empty()) {
        git.push('+');
    }
    println!("cargo:rustc-env=BUILD_STAMP={stamp}");
    println!("cargo:rustc-env=BUILD_GIT={git}");
    // Re-stamp whenever the code or the checked-out commit changes.
    for p in ["src", "Cargo.toml", "build.rs", ".git/HEAD", ".git/refs/heads", ".git/index", "assets/icon.png"] {
        println!("cargo:rerun-if-changed={p}");
    }

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        windows_resources();
    }
}
