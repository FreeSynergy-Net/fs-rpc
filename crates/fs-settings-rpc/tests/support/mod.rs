//! A real settings service on a real socket, on a thread of its own.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use fs_rpc::{bind_socket, serve};
pub use fs_settings::coded_stub::Coded;
use fs_settings::{
    Health, Instance, InstanceStore, MemoryStore, Place, ProfileName, ProfileRef, ProfileSet, Raw,
    Scope, SettingCode, SettingKey, SettingsStore, Snapshot, Stamp,
};
use fs_settings_rpc::{ClientTimes, SettingsService};
use tempfile::TempDir;
use tokio::sync::oneshot;

/// The closure type of `SettingsStore::update_index` (the port does not export its alias).
pub type IndexFn<'a> =
    dyn FnMut(&mut ProfileSet<u64>) -> Result<(), Coded<fs_settings::ProfileCode>> + 'a;

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

/// A port whose `update_index` always finds somebody else was first, `fail_first` times (`usize::MAX`: always); counts the rounds.
pub struct Flaky {
    pub inner: MemoryStore,
    pub fail_first: usize,
    pub rounds: std::sync::atomic::AtomicUsize,
}

impl SettingsStore for Flaky {
    fn stamp(&self) -> Stamp {
        self.inner.stamp()
    }
    fn health(&self) -> Health {
        self.inner.health()
    }
    fn instance(&self) -> Instance {
        self.inner.instance()
    }
    fn profile(&self, s: &Scope, p: Place, id: u64) -> Snapshot {
        self.inner.profile(s, p, id)
    }
    fn write(
        &self,
        s: &Scope,
        p: Place,
        id: u64,
        k: &SettingKey,
        r: Option<&Raw>,
    ) -> Result<(), Coded<SettingCode>> {
        self.inner.write(s, p, id, k, r)
    }
    fn clear(&self, s: &Scope, id: u64) -> Result<(), Coded<SettingCode>> {
        self.inner.clear(s, id)
    }
    fn index(&self) -> (ProfileSet<u64>, Vec<Coded<SettingCode>>) {
        self.inner.index()
    }
    fn update_index(&self, f: &mut IndexFn<'_>) -> Result<ProfileSet<u64>, Coded<SettingCode>> {
        let n = self
            .rounds
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if n < self.fail_first {
            // Somebody else changed the index in between: the closure sees another one than the client expected.
            let mut other = ProfileSet::new(999);
            let _ = f(&mut other);
            return Ok(other);
        }
        self.inner.update_index(f)
    }
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

pub fn reference(text: &str) -> fs_types::Reference {
    <fs_types::Reference as fs_types::BasicValue>::parse(text).unwrap()
}

/// Who and when, as the port takes them (the service fills them; the client's is not sent).
pub fn nobody() -> fs_settings::Stampd {
    fs_settings::Stampd {
        by: reference("https://example.net/nobody"),
        at: fs_types::Utc::from_millis(0),
    }
}

/// What a spying instance port saw per write: the OS user of the caller (`current_os_user`), the time, the actor.
pub type Seen = (Option<String>, fs_types::Utc, String);

/// A memory store that notes the caller context of every instance write (the port itself has no place for the OS user).
pub struct Spy {
    pub inner: MemoryStore,
    pub seen: std::sync::Mutex<Vec<Seen>>,
}

impl Spy {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: MemoryStore::new(),
            seen: std::sync::Mutex::new(Vec::new()),
        })
    }
    fn note(&self, who: &fs_settings::Stampd) {
        let canonical = <fs_types::Reference as fs_types::BasicValue>::canonical(&who.by);
        self.seen
            .lock()
            .unwrap()
            .push((fs_settings_rpc::current_os_user(), who.at, canonical));
    }
}

impl SettingsStore for Spy {
    fn stamp(&self) -> Stamp {
        self.inner.stamp()
    }
    fn health(&self) -> Health {
        self.inner.health()
    }
    fn instance(&self) -> Instance {
        self.inner.instance()
    }
    fn profile(&self, s: &Scope, p: Place, id: u64) -> Snapshot {
        self.inner.profile(s, p, id)
    }
    fn write(
        &self,
        s: &Scope,
        p: Place,
        id: u64,
        k: &SettingKey,
        r: Option<&Raw>,
    ) -> Result<(), Coded<SettingCode>> {
        self.inner.write(s, p, id, k, r)
    }
    fn clear(&self, s: &Scope, id: u64) -> Result<(), Coded<SettingCode>> {
        self.inner.clear(s, id)
    }
    fn index(&self) -> (ProfileSet<u64>, Vec<Coded<SettingCode>>) {
        self.inner.index()
    }
    fn update_index(&self, f: &mut IndexFn<'_>) -> Result<ProfileSet<u64>, Coded<SettingCode>> {
        self.inner.update_index(f)
    }
}

impl InstanceStore for Spy {
    fn write_instance(
        &self,
        k: &SettingKey,
        r: Option<&Raw>,
        who: &fs_settings::Stampd,
    ) -> Result<bool, Coded<SettingCode>> {
        self.note(who);
        self.inner.write_instance(k, r, who)
    }
    fn write_lock(
        &self,
        k: &SettingKey,
        c: &fs_settings::LockChange,
        who: &fs_settings::Stampd,
    ) -> Result<bool, Coded<SettingCode>> {
        self.note(who);
        self.inner.write_lock(k, c, who)
    }
    fn instance_log(
        &self,
        after: Option<fs_types::ObjectId>,
        size: u32,
    ) -> Vec<fs_settings::InstanceLogEntry> {
        self.inner.instance_log(after, size)
    }
}
