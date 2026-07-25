//! The network transport — a TCP relay that pumps operations between replicas.
//!
//! This is the once-deferred half of M8, and it is deliberately thin: the relay
//! owns no document and makes no decisions about the markup set. It is a star hub
//! that remembers every [`Op`] it has seen and rebroadcasts each new one to every
//! connected [`Client`]; each client folds arriving ops into its own [`Replica`]
//! with [`Replica::integrate`]. Convergence is entirely the replicas' property
//! (see [`crate::replica`]) — the network only decides *when* ops arrive, never
//! *what* the result is, which is why out-of-order or duplicated delivery is
//! harmless.
//!
//! Ephemeral **presence** lines (`J` / `L` / `P`) are rebroadcast but never stored
//! in the markup backlog — see [`crate::presence`].
//!
//! Dependency-free: [`std::net`] and [`std::thread`] only, one line per op (see
//! [`crate::wire`]). It is verifiable over a loopback socket without a display or
//! any service — see the crate's `tests/network.rs`.

use std::collections::BTreeMap;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use aadt_geometry::Point2;
use aadt_pdf::Markup;

use crate::op::{Op, OpId};
use crate::presence::PresenceMap;
use crate::replica::Replica;
use crate::wire::{self, PresenceMsg};

type OrderKey = (u64, String, u64);

/// The relay's shared state: the union of operations seen, and a write handle to
/// each connected client for broadcasting.
#[derive(Default)]
struct Hub {
    log: BTreeMap<OrderKey, Op>,
    clients: Vec<TcpStream>,
}

impl Hub {
    /// Record `op` if new, returning whether it was. New ops are broadcast to
    /// every client (dead sockets are dropped); duplicates are ignored, which is
    /// what stops a rebroadcast storm.
    fn relay(&mut self, op: Op) -> bool {
        let key = (op.clock, op.id.site.clone(), op.id.seq);
        if self.log.contains_key(&key) {
            return false;
        }
        self.log.insert(key, op.clone());
        let line = wire::encode_op(&op);
        self.broadcast_line(&line);
        true
    }

    /// Rebroadcast a presence line without recording it in the markup log.
    fn relay_presence(&mut self, line: &str) {
        self.broadcast_line(line);
    }

    fn broadcast_line(&mut self, line: &str) {
        self.clients
            .retain_mut(|c| writeln!(c, "{line}").and_then(|()| c.flush()).is_ok());
    }
}

/// A relay server. Bind it, hand [`addr`](RelayServer::addr) to clients, and it
/// pumps operations between them on a background thread until dropped.
pub struct RelayServer {
    addr: SocketAddr,
    _accept: JoinHandle<()>,
}

impl RelayServer {
    /// Bind and start accepting. Pass `"127.0.0.1:0"` to let the OS choose a free
    /// port, then read it back from [`addr`](RelayServer::addr).
    pub fn start(addr: impl ToSocketAddrs) -> io::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        let addr = listener.local_addr()?;
        let hub: Arc<Mutex<Hub>> = Arc::new(Mutex::new(Hub::default()));

        let accept = thread::spawn(move || {
            for incoming in listener.incoming() {
                let Ok(stream) = incoming else { continue };
                let Ok(writer) = stream.try_clone() else {
                    continue;
                };
                {
                    // Register the newcomer and replay the backlog under one lock,
                    // so no op slips in between the replay and registration.
                    let mut hub = hub.lock().unwrap();
                    let mut w = &writer;
                    let backlog: Vec<String> = hub.log.values().map(wire::encode_op).collect();
                    let ok = backlog
                        .iter()
                        .all(|line| writeln!(w, "{line}").and_then(|()| w.flush()).is_ok());
                    if ok {
                        hub.clients.push(writer);
                    } else {
                        continue; // client vanished mid-handshake
                    }
                }
                let hub = hub.clone();
                thread::spawn(move || serve_client(stream, hub));
            }
        });

        Ok(Self {
            addr,
            _accept: accept,
        })
    }

    /// The address clients connect to (the OS-assigned port if `:0` was used).
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }
}

/// Read one client's operations / presence, relaying each to peers.
fn serve_client(stream: TcpStream, hub: Arc<Mutex<Hub>>) {
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        let Ok(line) = line else { break };
        if line.is_empty() {
            continue;
        }
        if let Some(op) = wire::decode_op(&line) {
            hub.lock().unwrap().relay(op);
        } else if wire::decode_presence(&line).is_some() {
            hub.lock().unwrap().relay_presence(&line);
        }
    }
}

/// One collaborator's connection to a [`RelayServer`], wrapping a [`Replica`].
///
/// Local edits go through [`add_markup`](Client::add_markup) /
/// [`remove`](Client::remove): they update the local replica *and* send the
/// operation to the relay, which fans it out to the others. A background thread
/// folds arriving ops in, so [`snapshot`](Client::snapshot) always reflects
/// everything received so far.
pub struct Client {
    stream: TcpStream,
    site: String,
    replica: Arc<Mutex<Replica>>,
    presence: Arc<Mutex<PresenceMap>>,
    _reader: JoinHandle<()>,
}

impl Client {
    /// Connect to a relay and join as `site`. Site ids must be unique per
    /// participant (they disambiguate the total order). Sends a join presence
    /// line so peers can see the newcomer.
    pub fn connect(addr: impl ToSocketAddrs, site: impl Into<String>) -> io::Result<Self> {
        let site = site.into();
        let mut stream = TcpStream::connect(addr)?;
        let read_side = stream.try_clone()?;
        let replica = Arc::new(Mutex::new(Replica::new(site.clone())));
        let presence = Arc::new(Mutex::new(PresenceMap::new()));

        let sink = replica.clone();
        let presence_sink = presence.clone();
        let reader = thread::spawn(move || {
            let reader = BufReader::new(read_side);
            for line in reader.lines() {
                let Ok(line) = line else { break };
                if line.is_empty() {
                    continue;
                }
                if let Some(op) = wire::decode_op(&line) {
                    sink.lock().unwrap().integrate(op);
                } else if let Some(msg) = wire::decode_presence(&line) {
                    let mut map = presence_sink.lock().unwrap();
                    match msg {
                        PresenceMsg::Join { site } => {
                            map.upsert(site, Point2::new(0.0, 0.0));
                        }
                        PresenceMsg::Leave { site } => {
                            map.remove(&site);
                        }
                        PresenceMsg::Cursor { site, at } => {
                            map.upsert(site, at);
                        }
                    }
                }
            }
        });

        // Announce join (ephemeral — not in the op backlog).
        writeln!(
            stream,
            "{}",
            wire::encode_presence(&PresenceMsg::Join {
                site: site.clone()
            })
        )?;
        stream.flush()?;

        Ok(Self {
            stream,
            site,
            replica,
            presence,
            _reader: reader,
        })
    }

    /// Add a markup: record it locally and broadcast it. Returns its id (the
    /// handle a later [`remove`](Client::remove) names).
    pub fn add_markup(&mut self, markup: Markup) -> io::Result<OpId> {
        let op = self.replica.lock().unwrap().add_markup_op(markup);
        let id = op.id.clone();
        self.send_op(&op)?;
        Ok(id)
    }

    /// Remove the markup identified by `target`, and broadcast the removal.
    pub fn remove(&mut self, target: OpId) -> io::Result<()> {
        let op = self.replica.lock().unwrap().remove_op(target);
        self.send_op(&op)
    }

    /// Broadcast a cursor pulse for this site.
    pub fn set_cursor(&mut self, at: Point2) -> io::Result<()> {
        self.presence.lock().unwrap().upsert(self.site.clone(), at);
        let line = wire::encode_presence(&PresenceMsg::Cursor {
            site: self.site.clone(),
            at,
        });
        writeln!(self.stream, "{line}")?;
        self.stream.flush()
    }

    /// Announce leave (peers drop this site from presence).
    pub fn leave(&mut self) -> io::Result<()> {
        let line = wire::encode_presence(&PresenceMsg::Leave {
            site: self.site.clone(),
        });
        writeln!(self.stream, "{line}")?;
        self.stream.flush()
    }

    /// Snapshot of peer presence (includes self after a local `set_cursor`).
    pub fn presence(&self) -> PresenceMap {
        self.presence.lock().unwrap().clone()
    }

    /// A clone of the local replica — the converged view as of now.
    pub fn snapshot(&self) -> Replica {
        self.replica.lock().unwrap().clone()
    }

    /// The current markup set (owned), in total order.
    pub fn markups(&self) -> Vec<(OpId, Markup)> {
        self.replica
            .lock()
            .unwrap()
            .markups()
            .into_iter()
            .map(|(id, m)| (id, m.clone()))
            .collect()
    }

    fn send_op(&mut self, op: &Op) -> io::Result<()> {
        writeln!(self.stream, "{}", wire::encode_op(op))?;
        self.stream.flush()
    }
}
