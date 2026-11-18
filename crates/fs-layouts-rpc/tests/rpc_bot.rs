//! An AI (bot) connection: the actor is the connection's, never a client field; it cannot administrate (K78 c) and the store
//! refuses its writes until a commission can be checked.
#![allow(clippy::many_single_char_names, clippy::too_many_lines)]
mod support;

use fs_forms_layout::*;
use support::*;

const BOT: &str = "https://example.org/bots/ai";

#[test]
fn an_ai_connection_is_refused_admin_whatever_by_says() {
    let d = dir();
    let s = serve_actors(vec![
        (d.path().join("h.sock"), None),
        (d.path().join("b.sock"), Some(BOT.to_owned())),
    ]);
    let bot_socket = d.path().join("b.sock");
    run(async {
        let human = client(&s.socket);
        let bot = client(&bot_socket);
        let id = human
            .install(doc("board"), &pkg("1.0"), Mode::Apply, &who())
            .await
            .unwrap()
            .layout
            .unwrap();
        // the human connection may lock, even when it claims to be the bot
        let forged = Who {
            actor: Reference(BOT.to_owned()),
            author: who().author,
        };
        assert!(
            human
                .admin()
                .lock(&id, None, "audit", &forged)
                .await
                .unwrap()
        );
        // the bot connection may not, even when it claims to be the person
        let e = bot
            .admin()
            .unlock(&id, None, "audit", &who())
            .await
            .unwrap_err();
        assert_eq!(
            (code(&e), param(&e, "what")),
            ("unsupported", Some("on-behalf"))
        );
    });
}

#[test]
fn a_bot_connection_cannot_write_whatever_actor_it_claims() {
    let d = dir();
    let bot_socket = d.path().join("b.sock");
    let s = serve_actors(vec![
        (d.path().join("h.sock"), None),
        (bot_socket.clone(), Some(BOT.to_owned())),
    ]);
    run(async {
        let (human, bot) = (client(&s.socket), client(&bot_socket));
        human.create(doc("mine"), &who()).await.unwrap();
        // the bot claims to be the person (actor and author): the connection's actor wins, the store refuses (K78, no commission yet)
        let e = bot.create(doc("bots"), &who()).await.unwrap_err();
        assert_eq!(
            (code(&e), param(&e, "what")),
            ("unsupported", Some("on-behalf"))
        );
        // the human connection claims to be the bot: nothing changes for it
        let claim = Who {
            actor: Reference(BOT.to_owned()),
            author: who().author,
        };
        human.create(doc("claims"), &claim).await.unwrap();
        let log = human.log(None, 10).await.unwrap();
        let actors: Vec<_> = log
            .iter()
            .map(|e| (e.who.actor.0.as_str(), e.who.author.0.as_str()))
            .collect();
        assert_eq!(actors, [(PERSON, PERSON), (PERSON, PERSON)]);
        assert_eq!(
            human
                .list(&LayoutFilter::default(), None, 10)
                .await
                .unwrap()
                .items
                .len(),
            2
        );
    });
}
