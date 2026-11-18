//! A real settings service on a real socket, on a thread of its own.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use fs_rpc::{bind_socket, serve};
pub use fs_settings::{Coded, IndexFn};
use fs_settings::{
    InstanceStore, MemoryStore, ProfileName, ProfileRef, ProfileSet, SettingCode, SettingKey,
    SettingsStore,
};
use fs_settings_rpc::{ClientTimes, SettingsService};
use tempfile::TempDir;
use tokio::sync::oneshot;

pub type Service = SettingsService<dyn SettingsStore, MemoryStore>;

/// Quick limits for tests.
pub fn times() -> ClientTimes {
    ClientTimes {
        call: Duration::from_secs(5),
        retry: Duration::from_millis(50),
    }
}

pub fn dir() -> TempDir {
    tempfile::Builder::new()
        .prefix("s")
        .tempdir_in(env!("CARGO_TARGET_TMPDIR"))
        .unwrap()
}

pub fn key(text: &str) -> SettingKey {
    SettingKey::parse(text).unwrap()
}

pub fn program(name: &str) -> fs_settings::ProgramName {
    fs_settings::ProgramName::new(name).unwrap()
}

pub fn code(e: &Coded<SettingCode>) -> &'static str {
    use fs_settings::coded_stub::ErrorCode;
    e.code().as_str()
}

pub fn text<'a>(e: &'a Coded<SettingCode>, k: &str) -> Option<&'a str> {
    e.params().find_map(|(key, v)| match v {
        fs_settings::coded_stub::Param::Text(t) if key == k => Some(t.as_str()),
        _ => None,
    })
}

/// A server on its own thread; stopped (and the socket removed) on drop.
pub struct Server {
    pub socket: PathBuf,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Server {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Serves `make()` on `socket`.
pub fn serve_on<S, I>(
    socket: &Path,
    make: impl FnOnce() -> SettingsService<S, I> + Send + 'static,
) -> Server
where
    S: SettingsStore + ?Sized + 'static,
    I: InstanceStore + ?Sized + 'static,
{
    let (stop, stopped) = oneshot::channel::<()>();
    let (ready, up) = std::sync::mpsc::channel();
    let path = socket.to_path_buf();
    let thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let local = tokio::task::LocalSet::new();
        local.block_on(&rt, async move {
            let bound = bind_socket(&path).await.unwrap();
            let boot = make().bootstrap();
            ready.send(()).unwrap();
            serve(bound, boot, async move {
                let _ = stopped.await;
            })
            .await
            .unwrap();
        });
    });
    up.recv().unwrap();
    Server {
        socket: socket.to_path_buf(),
        stop: Some(stop),
        thread: Some(thread),
    }
}

/// The person's service over a memory store (shared with the caller), the same store as instance when `with_instance`.
pub fn person(socket: &Path, store: &MemoryStore, with_instance: bool) -> Server {
    let s = store.clone();
    serve_on(socket, move || {
        let port: Arc<dyn SettingsStore> = Arc::new(s.clone());
        SettingsService::new(port, with_instance.then(|| Arc::new(s)))
    })
}

/// Creates the profile `name` the way the shell does: the new id is above everything issued so far.
pub fn create(
    store: &dyn SettingsStore,
    name: &str,
) -> Result<ProfileSet<u64>, Coded<SettingCode>> {
    let name = ProfileName::new(name).unwrap();
    store.update_index(&mut |set| {
        let id = *set.get(&ProfileRef::Base).unwrap() + 1;
        set.create(name.clone(), None)?;
        *set.get_mut(&ProfileRef::Named(name.clone())).unwrap() = id;
        *set.get_mut(&ProfileRef::Base).unwrap() = id;
        Ok(())
    })
}

pub fn ids(set: &ProfileSet<u64>) -> Vec<(String, u64)> {
    let mut v: Vec<_> = set
        .names()
        .map(|n| {
            (
                n.as_str().to_owned(),
                *set.get(&ProfileRef::Named(n.clone())).unwrap(),
            )
        })
        .collect();
    v.sort();
    v
}

pub fn instance_port(store: &MemoryStore) -> &dyn InstanceStore {
    store
}

mod more_ports;
mod ports;
#[allow(unused_imports)]
pub use more_ports::*;
#[allow(unused_imports)]
pub use ports::*;
