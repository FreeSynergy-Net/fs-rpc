//! Shared by the handshake tests: an old server in a child process (this test binary, started again), the new server in a thread.
#![allow(dead_code)] // each test file uses a part

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use capnp_rpc::new_client;
use fs_interfaces::common_capnp::blob;
use fs_rpc::{RestartTimes, ServerIdentity, bind_socket, run_until_signal_as, serve};

/// The server threads that `start_new` made: `wait_gone` joins the finished ones, so the process-wide stop flag of a server that ended
/// is reset (its `Run` guard dropped) before the next test starts a server. A sleep would only hope for that.
pub static SERVERS: Mutex<Vec<std::thread::JoinHandle<()>>> = Mutex::new(Vec::new());

/// One server run at a time in this process (`stop_self` is a flag of the process).
pub static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

pub struct Seven;
impl blob::Server for Seven {
    #[allow(clippy::unused_async_trait_impl)] // the generated server trait is async; nothing to await here
    async fn size(
        self: capnp::capability::Rc<Self>,
        _: blob::SizeParams,
        mut results: blob::SizeResults,
    ) -> Result<(), capnp::Error> {
        results.get().set_bytes(7);
        Ok(())
    }
}

pub fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

/// The old server, in a child process: serves `Seven` without `Control` until SIGTERM.
pub fn old_server_main() {
    let Some(socket) = std::env::var_os("FS_OLD_SERVER") else {
        return;
    };
    let runtime = runtime();
    tokio::task::LocalSet::new().block_on(&runtime, async {
        let mut term =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
        let bound = bind_socket(Path::new(&socket)).await.unwrap();
        let root: blob::Client = new_client(Seven);
        serve(bound, root.client, async move {
            term.recv().await;
        })
        .await
        .unwrap();
    });
}

pub fn spawn_old_server(socket: &Path) -> Child {
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "helper_old_server", "--nocapture"])
        .env("FS_OLD_SERVER", socket)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let began = Instant::now();
    while connect_once(socket) {
        assert!(
            began.elapsed() < Duration::from_secs(20),
            "old server did not start"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    child
}

/// True while nothing answers on `socket`.
pub fn connect_once(socket: &Path) -> bool {
    std::os::unix::net::UnixStream::connect(socket).is_err()
}

/// Starts the new server (with `Control`) in this process on its own thread.
pub fn start_new(socket: &Path, version: &'static str) {
    let socket: PathBuf = socket.to_owned();
    let handle = std::thread::spawn(move || {
        let _ = run_until_signal_as(&socket, ServerIdentity::new("test", version), || async {
            let root: blob::Client = new_client(Seven);
            Ok(root.client)
        });
    });
    SERVERS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(handle);
}

/// Starts the first server and waits until it listens (else "no server" would be what the client finds).
pub fn start_first(socket: &Path, version: &'static str) {
    start_new(socket, version);
    let began = Instant::now();
    while connect_once(socket) {
        assert!(
            began.elapsed() < Duration::from_secs(10),
            "server did not start"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub async fn seven(c: &blob::Client) -> u64 {
    c.size_request()
        .send()
        .promise
        .await
        .unwrap()
        .get()
        .unwrap()
        .get_bytes()
}

pub fn fast() -> RestartTimes {
    RestartTimes {
        call: Duration::from_secs(5),
        stop: Duration::from_secs(10),
        start: Duration::from_secs(10),
    }
}

pub async fn wait_gone(socket: &Path) {
    let began = Instant::now();
    while !connect_once(socket) {
        assert!(
            began.elapsed() < Duration::from_secs(10),
            "server did not end"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // The socket is gone, the server thread may still be ending: join it (bounded), so its stop request is forgotten.
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let mut all = SERVERS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (done, running): (Vec<_>, Vec<_>) = all.drain(..).partition(|h| h.is_finished());
        *all = running;
        let left = all.len();
        drop(all);
        for handle in done {
            let _ = handle.join();
        }
        if left == 0 || Instant::now() >= until {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Child processes of a test, by PID: the watchdog ends them if the test does not finish.
pub static CHILDREN: Mutex<Vec<u32>> = Mutex::new(Vec::new());

/// Factor for every wait of a time-dependent test: `FS_TEST_SCALE` (default 1).
pub fn scale() -> u32 {
    std::env::var("FS_TEST_SCALE")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n| *n >= 1)
        .unwrap_or(1)
}

/// A hard limit for the whole test: when it runs out, the children of the test are killed by PID and the process ends red (never a
/// hang). Dropping it at the end of the test cancels it.
pub struct Watchdog(std::sync::Arc<std::sync::atomic::AtomicBool>);

pub fn watchdog(secs: u64) -> Watchdog {
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = done.clone();
    let limit = Duration::from_secs(secs) * scale();
    std::thread::spawn(move || {
        let until = Instant::now() + limit;
        while Instant::now() < until {
            if flag.load(std::sync::atomic::Ordering::SeqCst) {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        for pid in CHILDREN.lock().map(|c| c.clone()).unwrap_or_default() {
            if let Some(pid) = i32::try_from(pid)
                .ok()
                .and_then(rustix::process::Pid::from_raw)
            {
                let _ = rustix::process::kill_process(pid, rustix::process::Signal::KILL);
            }
        }
        eprintln!("WATCHDOG: the test did not finish within {limit:?}; its children were killed");
        std::process::exit(124);
    });
    Watchdog(done)
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}
