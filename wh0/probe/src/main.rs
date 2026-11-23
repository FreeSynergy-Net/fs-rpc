//! WH0 probe: facts about the Windows runner, printed as `key=value` lines. Not shipped, never merged.
//! `wh0-probe p2 <dir>` which file starts for a path without extension; `p4` known-folders and same-file;
//! `p5 <dir>` reparse points; `p6` elevation; `child` (started by p2) names its own file.

#[cfg(not(windows))]
fn main() {
    println!("wh0-probe: Windows only");
}

#[cfg(windows)]
fn main() {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("child") => println!("child-exe={}", std::env::current_exe().unwrap().display()),
        Some("p2") => p2(std::path::Path::new(&a[2])),
        Some("p4") => p4(),
        Some("p5") => p5(std::path::Path::new(&a[2])),
        Some("p6") => p6(),
        _ => eprintln!("usage: wh0-probe p2 <dir> | p4 | p5 <dir> | p6"),
    }
}

#[cfg(windows)]
fn p2(dir: &std::path::Path) {
    use std::process::Command;
    let me = std::env::current_exe().unwrap();
    for case in ["both", "only-plain", "only-exe"] {
        let d = dir.join(case);
        std::fs::create_dir_all(&d).unwrap();
        let (plain, exe) = (d.join("service"), d.join("service.exe"));
        if case != "only-exe" {
            std::fs::copy(&me, &plain).unwrap();
        }
        if case != "only-plain" {
            std::fs::copy(&me, &exe).unwrap();
        }
        for (label, cmd) in [
            ("absolute", plain.clone()),
            ("relative", std::path::PathBuf::from("service")),
        ] {
            let mut c = Command::new(&cmd);
            c.arg("child");
            if label == "relative" {
                c.current_dir(&d);
            }
            match c.output() {
                Ok(o) => println!(
                    "p2 case={case} how={label} status={:?} {}",
                    o.status.code(),
                    String::from_utf8_lossy(&o.stdout).trim()
                ),
                Err(e) => println!("p2 case={case} how={label} spawn-error={e}"),
            }
        }
    }
}

#[cfg(windows)]
fn p4() {
    use known_folders::{KnownFolder as K, get_known_folder_path as get};
    for (n, k) in [
        ("ProgramFiles", K::ProgramFiles),
        ("ProgramFilesX64", K::ProgramFilesX64),
        ("ProgramFilesX86", K::ProgramFilesX86),
        ("ProgramData", K::ProgramData),
        ("LocalAppData", K::LocalAppData),
    ] {
        println!("p4 known-folders {n}={:?}", get(k));
    }
    println!("p4 env ProgramFiles={:?}", std::env::var_os("ProgramFiles"));
    let d = std::env::temp_dir().join("wh0-same");
    std::fs::create_dir_all(&d).unwrap();
    let (a, b, c) = (d.join("a.txt"), d.join("b-hardlink.txt"), d.join("c.txt"));
    std::fs::write(&a, "x").unwrap();
    let _ = std::fs::remove_file(&b);
    std::fs::hard_link(&a, &b).unwrap();
    std::fs::write(&c, "x").unwrap();
    let upper = std::path::PathBuf::from(a.to_string_lossy().to_uppercase());
    let canon = std::fs::canonicalize(&a).unwrap();
    println!("p4 same-file a~a={:?}", same_file::is_same_file(&a, &a));
    println!("p4 same-file a~hardlink={:?}", same_file::is_same_file(&a, &b));
    println!("p4 same-file a~UPPERCASE-spelling={:?}", same_file::is_same_file(&a, &upper));
    println!("p4 same-file a~canonical(\\\\?\\)={:?} canon={}", same_file::is_same_file(&a, &canon), canon.display());
    println!("p4 same-file a~other-file-same-content={:?}", same_file::is_same_file(&a, &c));
    println!("p4 same-file a~missing={:?}", same_file::is_same_file(&a, d.join("missing")).map_err(|e| e.to_string()));
    println!("p4 path-starts_with case-sensitive: {}", a.starts_with(std::path::PathBuf::from(d.to_string_lossy().to_uppercase())));
}

#[cfg(windows)]
fn p5(dir: &std::path::Path) {
    use std::os::windows::fs::MetadataExt;
    use std::process::Command;
    std::fs::create_dir_all(dir).unwrap();
    let (target, junction, link, plain) = (dir.join("target"), dir.join("junction"), dir.join("symlink"), dir.join("plain"));
    for p in [&target, &plain] {
        std::fs::create_dir_all(p).unwrap();
    }
    let j = Command::new("cmd").args(["/c", "mklink", "/J"]).arg(&junction).arg(&target).output().unwrap();
    println!("p5 mklink-J status={:?} {}", j.status.code(), String::from_utf8_lossy(&j.stdout).trim());
    let r = std::os::windows::fs::symlink_dir(&target, &link);
    println!("p5 symlink_dir={:?}", r.map_err(|e| e.to_string()));
    for (n, p) in [("plain", &plain), ("junction", &junction), ("symlink", &link)] {
        match std::fs::symlink_metadata(p) {
            Ok(m) => println!(
                "p5 {n}: is_symlink={} is_dir={} attrs=0x{:x} reparse_attr={} (metadata.is_symlink via fs::metadata={:?})",
                m.file_type().is_symlink(),
                m.file_type().is_dir(),
                m.file_attributes(),
                m.file_attributes() & 0x400 != 0,
                std::fs::metadata(p).map(|m| m.file_type().is_symlink()).map_err(|e| e.to_string()),
            ),
            Err(e) => println!("p5 {n}: error {e}"),
        }
        println!("p5 {n}: read_link={:?}", std::fs::read_link(p).map_err(|e| e.to_string()));
    }
}

#[cfg(windows)]
fn p6() {
    use windows_sys::Win32::UI::Shell::IsUserAnAdmin;
    // SAFETY: IsUserAnAdmin takes no arguments and has no preconditions.
    println!("p6 IsUserAnAdmin={}", unsafe { IsUserAnAdmin() } != 0);
}
