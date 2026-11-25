//! Rights in the module: the wrapper that asks the rules of a module BEFORE every change (Stufe 4, part 2).
//!
//! Pattern: Template Method as a wrapper (Decorator around `M`) + Factory per connection ([`PerConnection`]) - the order
//! check -> change -> report stands ONCE here; a module delivers only reading, changing, taking back and its rules
//! ([`ModuleCore`], [`RightsRules`]) and cannot leave a step out, because only [`Guarded`] serves `FormSource` to the
//! caller. Deviation: not a Template Method in the strict sense (no base class; in Rust the wrapper is generic).
//!
//! The [`Context`] of a connection is built ONCE in `bootstrap` from the connection ([`Caller`], [`ActorOfConnection`],
//! [`PersonOfConnection`]) and from nothing else: no call argument, no global, no "current user". `introduce` replaces
//! only `claimed` (history and undo), never a right. A connection without a known person is refused everything
//! (fail closed). A verdict is read only through `is_allowed()`; a refusal reason or a hidden reason this code does not
//! know is a refusal or `noRight`. Everything runs in the `LocalSet` of the program (capnp-rpc).

use std::cell::RefCell;
use std::rc::Rc;

use capnp::capability::Client;
use capnp_rpc::new_client;
use fs_interfaces::formsource_capnp::form_source;
use fs_interfaces::module::{Caller as WhoCaller, ChangeRef, Context, ModuleCore};
use fs_types::Utc;

use super::conn::Conn;
use super::{Ports, WatchHub};
use crate::transport::{Caller, PerConnection};

/// The command key the rules are asked for a `revert` (the wrapper does not know the command of the change; card).
pub const REVERT_COMMAND: &str = "change.revert";
/// The command key the rules are asked before a change is delivered to a `watch` subscriber (per change, per record).
pub const WATCH_COMMAND: &str = "change.watch";

/// The clock of the wrapper: the "now" of `Ask.at` at a living call.
pub(crate) type Clock = Rc<dyn Fn() -> Utc>;

fn system_clock() -> Utc {
    Utc::from_millis(chrono::Utc::now().timestamp_millis())
}

/// Pattern: Template Method as a wrapper + Factory per connection - serves `FormSource` (level 2) for a module `M`.
pub struct Guarded<M: ModuleCore> {
    core: Rc<M>,
    ports: Ports,
    hub: Rc<WatchHub>,
    clock: Clock,
}

impl<M: ModuleCore> Guarded<M> {
    /// Wrap `core`; registers the one feed sink of the module (a change goes to every `watch` of every connection).
    #[must_use]
    pub fn new(core: M, ports: Ports) -> Self {
        Self::with_clock(core, ports, Rc::new(system_clock))
    }

    /// Like [`new`](Self::new) with an own clock for `Ask.at` (tests; never a value from a caller).
    #[must_use]
    pub fn with_clock(core: M, ports: Ports, clock: Clock) -> Self {
        let core = Rc::new(core);
        let hub = Rc::new(WatchHub::new());
        let (weak, sink_hub, sink_clock) = (Rc::downgrade(&core), hub.clone(), clock.clone());
        // A weak reference: the module holds the sink, the sink must not hold the module (no cycle).
        core.feed(Box::new(move |change: &ChangeRef| {
            if let Some(core) = weak.upgrade() {
                sink_hub.publish(core.rules(), sink_clock(), change);
            }
        }));
        Self {
            core,
            ports,
            hub,
            clock,
        }
    }

    /// The wrapped module (for a test or a program that needs its own counters; never a way around the rules).
    #[must_use]
    pub fn core(&self) -> &Rc<M> {
        &self.core
    }

    /// The hub of the watchers (for a program that wants to count them or end them).
    #[must_use]
    pub fn hub(&self) -> &Rc<WatchHub> {
        &self.hub
    }

    /// The context of a new connection, or `None` if the connection has no known person (then it is refused everything).
    fn context_of(&self, caller: &Caller) -> Option<Context> {
        let person = self.ports.person.person(caller)?;
        let who = WhoCaller::Connection {
            system_id: caller.id().to_owned(),
            system_name: caller.name().map(str::to_owned),
        };
        Some(Context::from_connection(
            who,
            self.ports.actor.actor(caller),
            person,
        ))
    }
}

impl<M: ModuleCore> PerConnection for Guarded<M> {
    fn bootstrap(&mut self, caller: &Caller) -> Client {
        let conn = Conn {
            core: self.core.clone(),
            hub: self.hub.clone(),
            clock: self.clock.clone(),
            cx: RefCell::new(self.context_of(caller)),
        };
        let client: form_source::Client = new_client(conn);
        client.client
    }
}
