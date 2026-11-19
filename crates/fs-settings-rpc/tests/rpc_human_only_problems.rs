//! K6-b: the problem lists of the port (health, index, profile) name the hidden key and carry a line number; a bot gets neither, a person
//! gets the lists as they are.

mod support;

use std::sync::Arc;

use fs_settings::{
    Coded, Declaration, GroupId, Health, IndexFn, Instance, Kind, MemoryStore, Place, ProfileSet,
    Raw, Scope, SettingCode, SettingKey, SettingsStore, Snapshot, Stamp, Value,
};
use fs_settings_rpc::{RpcSettingsStore, SettingsService};
use support::{dir, key, serve_on, text, times};

const HIDDEN: &str = "fs.mcp.enabled";
const OPEN: &str = "fs.look.compact";

fn noise() -> Vec<Coded<SettingCode>> {
    [HIDDEN, OPEN]
        .map(|k| {
            Coded::new(SettingCode::WriteFailed)
                .with_text("key", k)
                .with_number("line", 7)
        })
        .to_vec()
}

/// A port whose every problem list names both keys, with a line.
struct Noisy(MemoryStore);

impl SettingsStore for Noisy {
    fn stamp(&self) -> Stamp {
        self.0.stamp()
    }
    fn health(&self) -> Health {
        Health::Defaults(noise())
    }
    fn instance(&self) -> Instance {
        self.0.instance()
    }
    fn profile(&self, s: &Scope, p: Place, id: u64) -> Snapshot {
        Snapshot {
            problems: noise(),
            ..self.0.profile(s, p, id)
        }
    }
    fn write(
        &self,
        s: &Scope,
        p: Place,
        id: u64,
        k: &SettingKey,
        r: Option<&Raw>,
    ) -> Result<(), Coded<SettingCode>> {
        self.0.write(s, p, id, k, r)
    }
    fn clear(&self, s: &Scope, id: u64) -> Result<(), Coded<SettingCode>> {
        self.0.clear(s, id)
    }
    fn index(&self) -> (ProfileSet<u64>, Vec<Coded<SettingCode>>) {
        (self.0.index().0, noise())
    }
    fn update_index(&self, f: &mut IndexFn<'_>) -> Result<ProfileSet<u64>, Coded<SettingCode>> {
        self.0.update_index(f)
    }
}

fn client(person: bool) -> (RpcSettingsStore, support::Server, tempfile::TempDir) {
    let tmp = dir();
    let socket = tmp.path().join("n.sock");
    let server = serve_on(&socket, move || {
        let port: Arc<dyn SettingsStore> = Arc::new(Noisy(MemoryStore::new()));
        let none = None::<Arc<MemoryStore>>;
        let mut svc = if person {
            SettingsService::new_os_user_is_person(port, none)
        } else {
            SettingsService::new(port, none)
        };
        let decl = |k: &str, g: &str| {
            Declaration::new(
                key(k),
                Kind::Flag,
                Value::Flag(false),
                GroupId::new(g).unwrap(),
            )
        };
        svc.catalog =
            fs_settings::Catalog::of(vec![decl(HIDDEN, "ai").human_only(), decl(OPEN, "look")])
                .unwrap();
        svc
    });
    (RpcSettingsStore::manage(&socket, times()), server, tmp)
}

fn keys_and_lines(list: &[Coded<SettingCode>]) -> (Vec<String>, usize) {
    let keys = list
        .iter()
        .filter_map(|p| text(p, "key"))
        .map(str::to_owned)
        .collect();
    let lines = list
        .iter()
        .filter(|p| p.params().any(|(k, _)| k == "line"))
        .count();
    (keys, lines)
}

#[test]
fn a_bot_gets_the_problem_lists_without_the_hidden_key_and_without_lines() {
    let (m, _server, _tmp) = client(false);
    let Health::Defaults(health) = m.health() else {
        panic!("the port serves defaults");
    };
    let lists = [
        health,
        m.index().1,
        m.profile(&Scope::General, Place::Profile, 0).problems,
    ];
    for list in lists {
        assert_eq!(
            keys_and_lines(&list),
            ([OPEN.to_owned()].to_vec(), 0),
            "{list:?}"
        );
    }
}

#[test]
fn a_person_gets_the_problem_lists_whole() {
    let (m, _server, _tmp) = client(true);
    let Health::Defaults(health) = m.health() else {
        panic!("the port serves defaults");
    };
    let lists = [
        health,
        m.index().1,
        m.profile(&Scope::General, Place::Profile, 0).problems,
    ];
    for list in lists {
        assert_eq!(
            keys_and_lines(&list),
            ([HIDDEN, OPEN].map(str::to_owned).to_vec(), 2),
            "{list:?}"
        );
    }
}
