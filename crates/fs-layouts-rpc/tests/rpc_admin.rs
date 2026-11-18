//! The actor is the connection (K78): a forged `by` is recorded and never effective; a watcher hears every write path, a lost
//! connection is told to the host.
#![allow(clippy::many_single_char_names, clippy::too_many_lines)]
mod support;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use fs_forms_layout::*;
use support::*;

async fn until(mut done: impl FnMut() -> bool) -> bool {
    for _ in 0..100 {
        if done() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    done()
}

/// The server of an instance service that hands every claimed `by` to a channel.
fn claiming(socket: &std::path::Path) -> (Server, std::sync::mpsc::Receiver<String>) {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = serve_on(socket, move || {
        let mut svc = service(true);
        svc.claimed_by = Some(Rc::new(move |_: &fs_rpc::Caller, by: &str| {
            let _ = tx.send(by.to_owned());
        }));
        svc
    });
    (s, rx)
}

#[test]
fn the_actor_of_an_admin_call_is_the_connection_and_a_forged_by_is_only_recorded() {
    let d = dir();
    let (s, claims) = claiming(&d.path().join("l.sock"));
    run(async {
        let c = client(&s.socket);
        let id = c
            .install(doc("board"), &pkg("1.0"), Mode::Apply, &who())
            .await
            .unwrap()
            .layout
            .unwrap();
        // raw: a client that claims to be someone else, and one that claims nothing
        let conn = raw(&s.socket).await;
        for claim in ["https://example.org/people/forged", ""] {
            let mut a = conn.client.admin_request();
            a.get().set_by(claim);
            let admin = a
                .send()
                .promise
                .await
                .unwrap()
                .get()
                .unwrap()
                .get_admin()
                .unwrap();
            let mut req = admin.lock_request();
            put_uuid(req.get().init_layout(), &fs_layouts_rpc_id(&id));
            req.get().set_reason(if claim.is_empty() {
                "empty claim"
            } else {
                "forged claim"
            });
            assert!(
                req.send()
                    .promise
                    .await
                    .unwrap()
                    .get()
                    .unwrap()
                    .get_changed()
            );
            // release it again, so the next claim locks anew
            let mut off = admin.unlock_request();
            put_uuid(off.get().init_layout(), &fs_layouts_rpc_id(&id));
            off.get().set_reason("released");
            off.send().promise.await.unwrap();
        }
        assert_eq!(
            claims.try_iter().collect::<Vec<_>>(),
            ["https://example.org/people/forged", ""]
        );
        let log = c.log(None, 50).await.unwrap();
        let locks: Vec<_> = log.iter().filter(|e| e.action == LogAction::Lock).collect();
        assert_eq!(locks.len(), 2);
        for e in locks {
            assert_eq!(
                (e.who.actor.0.as_str(), e.who.author.0.as_str()),
                (PERSON, PERSON),
                "forged by became the actor"
            );
        }
        // the client of the port sends `who.actor` as the claim, nothing else changes
        let claimed = Who {
            actor: Reference("https://example.org/people/claimed".into()),
            author: who().author,
        };
        c.admin()
            .lock(&id, None, "claimed", &claimed)
            .await
            .unwrap();
        assert_eq!(
            claims.try_iter().collect::<Vec<_>>(),
            ["https://example.org/people/claimed"]
        );
        let log = c.log(None, 50).await.unwrap();
        assert_eq!(
            log.iter()
                .rev()
                .find(|e| e.action == LogAction::Lock)
                .unwrap()
                .who
                .actor
                .0,
            PERSON
        );
    });
}

/// The UUID text of a layout id as the wire carries it.
fn fs_layouts_rpc_id(id: &LayoutId) -> String {
    id.0.0.clone()
}

#[test]
fn a_watcher_hears_every_write_path() {
    let d = dir();
    let s = instance(&d.path().join("l.sock"));
    run(async {
        let c = client(&s.socket);
        let heard = Rc::new(Cell::new(0_u32));
        let h = heard.clone();
        let _w = c
            .watch(Rc::new(move |_| h.set(h.get() + 1)), Rc::new(|| {}))
            .await
            .unwrap();
        let mut expect = 0;
        macro_rules! heard_one {
            ($what:literal) => {{
                expect += 1;
                assert!(
                    until(|| heard.get() == expect).await,
                    concat!("no change heard after ", $what)
                );
            }};
        }
        let own = c.create(doc("own"), &who()).await.unwrap().layout.unwrap();
        heard_one!("create");
        let pk = c
            .install(doc("board"), &pkg("1.0"), Mode::Apply, &who())
            .await
            .unwrap()
            .layout
            .unwrap();
        heard_one!("install");
        let o = c.create_overlay(&pk, None, None, &who()).await.unwrap();
        heard_one!("create_overlay");
        c.delete_overlay(&o, Rev(1), &who()).await.unwrap();
        heard_one!("delete_overlay");
        let o2 = c.create_overlay(&pk, None, None, &who()).await.unwrap();
        heard_one!("create_overlay (2)");
        c.admin().lock(&pk, None, "audit", &who()).await.unwrap();
        heard_one!("admin lock");
        c.admin()
            .unlock(&pk, None, "audit done", &who())
            .await
            .unwrap();
        heard_one!("admin unlock");
        c.admin()
            .create_instance_overlay(&pk, None, None, &who())
            .await
            .unwrap();
        heard_one!("admin create_instance_overlay");
        c.set_state(&o2, Rev(1), false, &who()).await.unwrap();
        heard_one!("set_state");
        // an overlay with an edit, then a changed original: a conflict, and its resolution
        let o3 = c.create_overlay(&pk, None, None, &who()).await.unwrap();
        heard_one!("create_overlay (3)");
        c.replace_ops(&o3, Rev(1), vec![set_text(Field::TextKey, "mine")], &who())
            .await
            .unwrap();
        heard_one!("replace_ops");
        let mut v2 = doc("board");
        v2.pages[0].v.blocks[0].v.text_key = Some("theirs".into());
        let r = c
            .install(v2, &pkg("2.0"), Mode::Apply, &who())
            .await
            .unwrap();
        heard_one!("install (2)");
        assert_eq!(r.conflicts.len(), 1, "{r:?}");
        c.resolve_conflict(&o3, 0, "2.0", Resolution::Keep, &who())
            .await
            .unwrap();
        heard_one!("resolve_conflict");
        let od = OverlayDoc {
            format: 1,
            package: "net.example.pkg".into(),
            extends: "net.example.Tasks".into(),
            frontend: "board".into(),
            base_version: "2.0".into(),
            lang: None,
            title_key: None,
            ops: vec![Item {
                elem: Some(elem("block:board/main")),
                line: 0,
                v: set_text(Field::TextKey, "imported"),
            }],
        };
        let dry = c
            .import_overlay(od.clone(), Mode::DryRun, &who())
            .await
            .unwrap();
        assert!(dry.problems.is_empty(), "{dry:?}");
        // a dry run writes nothing: after a pause that lets a wrong notice arrive, the count is still the writes so far
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(heard.get(), expect, "a dry run was heard as a change");
        let done = c.import_overlay(od, Mode::Apply, &who()).await.unwrap();
        assert!(done.problems.is_empty(), "{done:?}");
        heard_one!("import_overlay");
        c.save(&own, Rev(1), doc("own"), &who()).await.unwrap();
        heard_one!("save");
        c.import(doc("imp"), Mode::Apply, &who()).await.unwrap();
        heard_one!("import");
        c.delete(&own, Rev(2), &who()).await.unwrap();
        heard_one!("delete");
        c.uninstall("net.example.pkg", &who()).await.unwrap();
        heard_one!("uninstall");
        assert_eq!(heard.get(), expect, "more changes heard than writes");
    });
}

#[test]
fn a_lost_connection_is_told_and_a_normal_drop_is_not() {
    let d = dir();
    let s = person(&d.path().join("l.sock"));
    run(async {
        let c = client(&s.socket);
        let ended = Rc::new(RefCell::new(Vec::<&str>::new()));
        let (e1, e2) = (ended.clone(), ended.clone());
        let kept = c
            .watch(
                Rc::new(|_| {}),
                Rc::new(move || e1.borrow_mut().push("lost")),
            )
            .await
            .unwrap();
        let dropped = c
            .watch(
                Rc::new(|_| {}),
                Rc::new(move || e2.borrow_mut().push("dropped")),
            )
            .await
            .unwrap();
        drop(dropped);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(
            ended.borrow().is_empty(),
            "a normal drop was told as a loss"
        );
        drop(s);
        assert!(
            until(|| ended.borrow().as_slice() == ["lost"]).await,
            "the lost connection was not told"
        );
        drop(kept);
    });
}
