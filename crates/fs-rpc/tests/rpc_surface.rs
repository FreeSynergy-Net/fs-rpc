//! Is there a surface, and does the default `ensure_single` therefore ask? Tested in a CHILD PROCESS with a made environment, so the
//! answer does not depend on the machine (a developer's `DISPLAY`, a CI without one): `env_clear` + standard input `/dev/null` is
//! headless (false); `DISPLAY=:0` is a surface; only `WAYLAND_DISPLAY` is a surface; both empty is none. The child asks `has_surface()`,
//! `SingleOptions::default()` and the plain `ensure_single` (against a real silent server without heartbeat: `no-control`), and says
//! what it got; the parent compares the table. Mutants: the terminal test of standard input inverted, a default that always assumes a
//! surface, an empty name that counts, a default call that ignores the options.
//!
//! Pattern: Table of environments over a child (the test binary again), a watchdog around the children ([TEST-TIMEOUT]).
#![cfg(target_os = "linux")]

use std::process::{Command, Stdio};

use fs_interfaces::common_capnp::blob;
use fs_rpc::{AskPolicy, SingleError, SingleOptions, ensure_single, has_surface};

mod handshake_support;
mod single_support;
use handshake_support::*;
use single_support::*;

#[test]
fn helper_hanging_server() {
    hanging_server_main();
}

/// The child: only when the parent asks for it (`FS_SURFACE_SOCKET`); says `has_surface`, the default options and the plain call.
#[test]
fn helper_surface_child() {
    let Some(socket) = std::env::var_os("FS_SURFACE_SOCKET") else {
        return;
    };
    let socket = std::path::PathBuf::from(socket);
    let starter = starter(&socket);
    let rt = runtime();
    let result = tokio::task::LocalSet::new().block_on(&rt, async {
        ensure_single::<blob::Client>(&socket, &expected(), &starter, &times(), None)
            .await
            .map(|_| ())
    });
    let Err(SingleError::Unresponsive(stuck)) = result else {
        println!("RESULT unexpected");
        return;
    };
    let default = SingleOptions::default();
    println!(
        "RESULT surface={} default_asks={} default_ask_policy_is_ask={} can_ask={} choices={}",
        has_surface(),
        default.asks(),
        default.ask == AskPolicy::Ask,
        stuck.can_ask,
        stuck.choices.tokens().join(",")
    );
}

/// A name, the environment of the child, whether it has a surface.
type Row<'a> = (&'a str, &'a [(&'a str, &'a str)], bool);

fn ask_child(socket: &std::path::Path, env: &[(&str, &str)]) -> String {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "helper_surface_child", "--nocapture"])
        .env_clear()
        .envs(env.iter().copied())
        .env("FS_SURFACE_SOCKET", socket)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let child = command.spawn().expect("child starts");
    let id = child.id();
    CHILDREN.lock().unwrap().push(id);
    let out = child.wait_with_output().expect("child ends");
    CHILDREN.lock().unwrap().retain(|p| *p != id);
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find(|l| l.starts_with("RESULT "))
        .unwrap_or("RESULT none")
        .to_owned()
}

#[test]
fn the_surface_and_the_default_follow_the_environment_not_the_machine() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _limit = watchdog(120);
    let (_dir, socket, mut child, _) = setup("silent", false);
    let table: [Row; 6] = [
        ("nothing set, stdin /dev/null: headless", &[], false),
        ("DISPLAY=:0", &[("DISPLAY", ":0")], true),
        (
            "only WAYLAND_DISPLAY",
            &[("WAYLAND_DISPLAY", "wayland-0")],
            true,
        ),
        (
            "both empty",
            &[("DISPLAY", ""), ("WAYLAND_DISPLAY", "")],
            false,
        ),
        (
            "empty DISPLAY, Wayland named",
            &[("DISPLAY", ""), ("WAYLAND_DISPLAY", "w")],
            true,
        ),
        (
            "DISPLAY named, empty Wayland",
            &[("DISPLAY", ":1"), ("WAYLAND_DISPLAY", "")],
            true,
        ),
    ];
    for (name, env, surface) in table {
        let line = ask_child(&socket, env);
        let choices = if surface { "wait,replace,abort" } else { "" };
        let want = format!(
            "RESULT surface={surface} default_asks={surface} default_ask_policy_is_ask=true can_ask={surface} choices={choices}"
        );
        assert_eq!(line, want, "{name}");
    }
    assert!(alive(&mut child), "no signal for no-control, whoever asks");
}
