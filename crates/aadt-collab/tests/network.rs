//! End-to-end: real TCP sockets on loopback prove the transport converges.
//!
//! These exercise [`RelayServer`] + [`Client`] over `127.0.0.1`, with no display
//! and no external service — the once-deferred half of M8, now verifiable in CI.

use std::time::{Duration, Instant};

use aadt_collab::{Client, RelayServer};
use aadt_geometry::Point2;
use aadt_pdf::{Markup, MarkupKind};

fn note(author: &str, text: &str) -> Markup {
    Markup::new(MarkupKind::Cloud, Point2::new(0.0, 0.0), author, text)
}

/// The markup texts on a client, in the replica's total order.
fn texts(c: &Client) -> Vec<String> {
    c.markups().iter().map(|(_, m)| m.note.clone()).collect()
}

/// Poll until `cond` holds or we give up. Network delivery is asynchronous, so a
/// bounded wait is how a test observes "has it propagated yet" without a sleep
/// guess; convergence itself is deterministic once the ops have all arrived.
fn wait_until(mut cond: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    cond()
}

#[test]
fn two_clients_converge_over_tcp() {
    let server = RelayServer::start("127.0.0.1:0").unwrap();
    let mut alice = Client::connect(server.addr(), "alice").unwrap();
    let mut bob = Client::connect(server.addr(), "bob").unwrap();

    // Each adds a markup; the relay fans each op out to the other.
    let a1 = alice
        .add_markup(note("alice", "clarify north arrow"))
        .unwrap();
    bob.add_markup(note("bob", "confirm door swing")).unwrap();

    assert!(
        wait_until(|| texts(&alice).len() == 2 && texts(&bob).len() == 2),
        "both clients should receive both ops"
    );
    // Same set AND same order — the total order is deterministic across sites.
    assert_eq!(texts(&alice), texts(&bob));

    // A remove made on one client propagates and wins on the other.
    bob.remove(a1).unwrap();
    assert!(
        wait_until(|| texts(&alice).len() == 1 && texts(&bob).len() == 1),
        "the remove should propagate to alice"
    );
    assert_eq!(texts(&alice), vec!["confirm door swing"]);
    assert_eq!(texts(&alice), texts(&bob));
}

#[test]
fn a_late_joiner_receives_the_backlog() {
    let server = RelayServer::start("127.0.0.1:0").unwrap();
    let mut alice = Client::connect(server.addr(), "alice").unwrap();
    alice.add_markup(note("alice", "first")).unwrap();
    alice.add_markup(note("alice", "second")).unwrap();

    // Bob connects only after alice has already drawn; he must catch up.
    let bob = Client::connect(server.addr(), "bob").unwrap();
    assert!(
        wait_until(|| texts(&bob).len() == 2),
        "a late joiner replays the existing log"
    );
    assert_eq!(texts(&bob), vec!["first", "second"]);
}

#[test]
fn three_clients_all_converge() {
    let server = RelayServer::start("127.0.0.1:0").unwrap();
    let mut a = Client::connect(server.addr(), "a").unwrap();
    let mut b = Client::connect(server.addr(), "b").unwrap();
    let mut c = Client::connect(server.addr(), "c").unwrap();

    a.add_markup(note("a", "from-a")).unwrap();
    b.add_markup(note("b", "from-b")).unwrap();
    c.add_markup(note("c", "from-c")).unwrap();

    let all_three = |cl: &Client| texts(cl).len() == 3;
    assert!(
        wait_until(|| all_three(&a) && all_three(&b) && all_three(&c)),
        "every client should see all three markups"
    );
    // All three fold to the identical ordered set.
    assert_eq!(texts(&a), texts(&b));
    assert_eq!(texts(&b), texts(&c));
}

#[test]
fn cursor_presence_propagates_over_tcp() {
    let server = RelayServer::start("127.0.0.1:0").unwrap();
    let mut alice = Client::connect(server.addr(), "alice").unwrap();
    let bob = Client::connect(server.addr(), "bob").unwrap();

    alice
        .set_cursor(Point2::new(12.5, -3.0))
        .expect("send cursor");

    assert!(
        wait_until(|| {
            bob.presence()
                .get("alice")
                .is_some_and(|p| (p.at.x - 12.5).abs() < 1e-9 && (p.at.y + 3.0).abs() < 1e-9)
        }),
        "bob should see alice's cursor"
    );

    alice.leave().unwrap();
    assert!(
        wait_until(|| bob.presence().get("alice").is_none()),
        "leave should clear alice from bob's presence map"
    );
}
