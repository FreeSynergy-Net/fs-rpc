//! The OS user of the connection in the change log: `SettingsCore` runs its sync store calls on the blocking pool and sets the caller
//! of the connection (`Context::caller`) for exactly that call, so a store that logs (`fs_settings_rpc::current_os_user`) names who
//! changed - the same value as via the hub. Counter-proof: a plain blocking-pool call without `scoped` sees `None` (red).
//!
//! Pattern: Spy (test double) - the store notes `current_os_user()` at every write.

mod module_support;

use std::sync::{Arc, Mutex};

use fs_rpc::Guarded;
use fs_settings::{
    Coded, Health, IndexFn, Instance, MemoryStore, Place, ProfileSet, Raw, Scope, SettingCode,
    SettingKey, SettingsStore, Snapshot, Stamp,
};
use fs_settings_rpc::{SettingsCore, current_os_user};
use module_support::{WEEK, catalog, connect, invoke, key, ports, run};

/// Pattern: Spy - a store that delegates to `MemoryStore` and notes (key, OS user as the service set it) at every write.
#[derive(Default)]
struct Spy {
    inner: MemoryStore,
    log: Mutex<Vec<(String, Option<String>)>>,
}

impl Spy {
    fn log(&self) -> Vec<(String, Option<String>)> {
        self.log.lock().unwrap().clone()
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
    fn profile(&self, scope: &Scope, place: Place, profile: u64) -> Snapshot {
        self.inner.profile(scope, place, profile)
    }
    fn write(
        &self,
        scope: &Scope,
        place: Place,
        profile: u64,
        key: &SettingKey,
        raw: Option<&Raw>,
    ) -> Result<(), Coded<SettingCode>> {
        let note = (key.as_str().to_owned(), current_os_user());
        self.log.lock().unwrap().push(note);
        self.inner.write(scope, place, profile, key, raw)
    }
    fn clear(&self, scope: &Scope, profile: u64) -> Result<(), Coded<SettingCode>> {
        self.inner.clear(scope, profile)
    }
    fn index(&self) -> (ProfileSet<u64>, Vec<Coded<SettingCode>>) {
        self.inner.index()
    }
    fn update_index(&self, f: &mut IndexFn<'_>) -> Result<ProfileSet<u64>, Coded<SettingCode>> {
        self.inner.update_index(f)
    }
}

fn core_over(spy: &Arc<Spy>) -> SettingsCore<Spy> {
    let program = fs_settings::ProgramName::new("tasks").unwrap();
    SettingsCore::new(program, catalog(), spy.clone())
}

#[test]
fn set_and_reset_over_the_form_source_name_the_os_user_of_the_connection() {
    run(async {
        let spy = Arc::new(Spy::default());
        let mut g = Guarded::new(core_over(&spy), ports());
        let me = connect(&mut g, "alice");
        assert!(
            !invoke(&me, "value.set", WEEK, &[("value", "4")])
                .await
                .failed
        );
        assert!(!invoke(&me, "value.reset", WEEK, &[]).await.failed);
        let log = spy.log();
        assert_eq!(log.len(), 2, "{log:?}");
        for (k, user) in &log {
            assert_eq!(k, WEEK);
            assert_eq!(user.as_deref(), Some("alice"), "the connection's OS user");
        }
        // the value does not leak out of the call: this thread has no caller
        assert_eq!(current_os_user(), None);
    });
}

#[test]
fn counter_proof_without_scoped_the_os_user_is_none() {
    run(async {
        let spy = Arc::new(Spy::default());
        let (s, k) = (spy.clone(), key(WEEK));
        // what `SettingsCore` did before: the store call on the blocking pool WITHOUT the caller set
        tokio::task::spawn_blocking(move || {
            s.write(
                &Scope::Program(fs_settings::ProgramName::new("tasks").unwrap()),
                Place::Profile,
                0,
                &k,
                Some(&Raw::Int(4)),
            )
        })
        .await
        .unwrap()
        .unwrap();
        assert_eq!(spy.log(), vec![(WEEK.to_owned(), None)]);
    });
}
