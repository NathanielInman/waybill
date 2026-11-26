//! `waybill serve`: the task list for the phone app, over your own network.
//! It hands out what `waybill_core::wire` builds (the snapshot plus every
//! choice the board makes when it draws) and runs what the phone says
//! through `waybill_core::say`, the same words the board's `/` line reads.
//! Every request carries the token `waybill pair` shows the phone once.

use anyhow::{Context, bail};
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use serde::Deserialize;
use serde_json::json;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use waybill_core::act::NewTask;
use waybill_core::agent::Desk;
use waybill_core::clock::{self, Zone};
use waybill_core::config::Config;
use waybill_core::model::{Card, Snapshot};
use waybill_core::say::{self, Action, Command};
use waybill_core::theme::Palette;
use waybill_core::{paths, wire};

/// How long a phone's "anything newer?" waits before it is told no.
const LONG_POLL: Duration = Duration::from_secs(25);
/// The most a request may send.
const MAX_BODY: usize = 1 << 20;

/// The token the phone sends with every request. Made on first use and
/// kept beside the config, readable only by you.
pub fn token_file() -> PathBuf {
    paths::config_dir().join("phone.token")
}

fn random_hex(bytes: usize) -> anyhow::Result<String> {
    let mut buf = vec![0u8; bytes];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut buf))
        .context("could not read /dev/urandom")?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

/// The phone's token, made if there is none (or `fresh` asks for a new one,
/// which unpairs the phone that had the old one).
pub fn token(fresh: bool) -> anyhow::Result<String> {
    let path = token_file();
    if !fresh
        && let Ok(t) = std::fs::read_to_string(&path)
        && t.trim().len() >= 32
    {
        return Ok(t.trim().to_string());
    }
    let t = random_hex(32)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
    let mut f = opts
        .open(&path)
        .with_context(|| format!("could not write {}", path.display()))?;
    writeln!(f, "{t}")?;
    Ok(t)
}

/// This machine's address on the network it routes through. Connecting a
/// UDP socket sends nothing; it only picks the interface.
fn lan_ip() -> Option<std::net::IpAddr> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("192.0.2.1:9").ok()?;
    Some(s.local_addr().ok()?.ip())
}

/// Where the phone finds the server: `serve.url`, or this machine's LAN
/// address and the port it listens on.
pub fn url(cfg: &Config) -> String {
    if !cfg.serve.url.trim().is_empty() {
        return cfg.serve.url.trim().trim_end_matches('/').to_string();
    }
    let scheme = if cfg.serve.cert.is_empty() {
        "http"
    } else {
        "https"
    };
    let port = cfg.serve.listen.rsplit(':').next().unwrap_or("8446");
    let host = lan_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "localhost".into());
    format!("{scheme}://{host}:{port}")
}

fn pct(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// The link the phone app opens to pair: the server and the token.
pub fn pair_link(url: &str, token: &str) -> String {
    format!("waybill://pair?url={}&token={token}", pct(url))
}

/// One reading of the list, ready to send.
struct Current {
    version: u64,
    snap: Arc<Snapshot>,
    screen: Arc<String>,
    glance: Arc<String>,
    /// The screen as it was last sent, without its version, to tell a real
    /// change from a reread that found nothing new.
    plain: String,
}

struct Server {
    demo: bool,
    token: String,
    current: Mutex<Current>,
    changed: Condvar,
    /// One reread at a time.
    reading: Mutex<()>,
}

impl Server {
    fn config() -> Config {
        let (cfg, problem) = Config::load();
        if let Some(p) = problem {
            eprintln!("waybill serve: config ignored: {p}");
        }
        cfg
    }

    fn note(&self, c: &Card) -> Option<String> {
        if self.demo {
            return Some(waybill_core::demo::note(&c.title));
        }
        let cfg = Server::config();
        let dir = waybill_core::load::notes_dir(&cfg)?;
        waybill_core::act::read_note(&dir, &c.uuid)
    }

    /// Reads the list again and, when anything changed, tells everyone
    /// waiting.
    fn refresh(&self) -> anyhow::Result<()> {
        let _one = self.reading.lock().unwrap_or_else(|e| e.into_inner());
        let cfg = Server::config();
        let now = clock::now();
        let snap = if self.demo {
            waybill_core::demo::snapshot(now, Zone::Local)
        } else {
            let notes = waybill_core::load::notes_dir(&cfg);
            waybill_core::load::snapshot(&cfg, notes.as_deref(), now, Zone::Local)
        };
        let palette = Palette::from(waybill_core::waybar::load_theme().as_ref(), &cfg);
        let options = wire::options(&snap, |c| self.note(c));
        let mut screen = wire::screen(&snap, &palette, options.clone(), self.demo);
        let plain = serde_json::to_string(&screen)?;
        let mut cur = self.current.lock().unwrap_or_else(|e| e.into_inner());
        if plain == cur.plain {
            return Ok(());
        }
        let version = cur.version + 1;
        screen.version = version;
        let mut glance = wire::glance(&snap, &options);
        glance.version = version;
        *cur = Current {
            version,
            screen: Arc::new(serde_json::to_string(&screen)?),
            glance: Arc::new(serde_json::to_string(&glance)?),
            plain,
            snap: Arc::new(snap.clone()),
        };
        drop(cur);
        self.changed.notify_all();
        Ok(())
    }

    /// The screen, once it is newer than `after` or the wait runs out.
    fn screen_after(&self, after: Option<u64>) -> Arc<String> {
        let deadline = Instant::now() + LONG_POLL;
        let mut cur = self.current.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(after) = after {
            while cur.version <= after {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    break;
                }
                cur = self
                    .changed
                    .wait_timeout(cur, left)
                    .unwrap_or_else(|e| e.into_inner())
                    .0;
            }
        }
        cur.screen.clone()
    }

    fn snap(&self) -> (Arc<Snapshot>, u64) {
        let cur = self.current.lock().unwrap_or_else(|e| e.into_inner());
        (cur.snap.clone(), cur.version)
    }

    fn desk(&self) -> anyhow::Result<Desk> {
        if self.demo {
            bail!("The demo list cannot be changed.");
        }
        Ok(Desk::open(Server::config()))
    }
}

/// A request, read just far enough to answer it.
struct Request {
    method: String,
    path: String,
    query: String,
    auth: Option<String>,
    body: Vec<u8>,
}

fn read_request(r: &mut impl Read) -> anyhow::Result<Request> {
    let mut r = BufReader::new(r);
    let mut line = String::new();
    r.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("/").to_string();
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target, String::new()),
    };
    let (mut auth, mut length) = (None, 0usize);
    for _ in 0..100 {
        line.clear();
        if r.read_line(&mut line)? == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            match k.trim().to_ascii_lowercase().as_str() {
                "authorization" => auth = Some(v.trim().to_string()),
                "content-length" => length = v.trim().parse().unwrap_or(0),
                _ => {}
            }
        }
    }
    if length > MAX_BODY {
        bail!("the request is too large");
    }
    let mut body = vec![0u8; length];
    r.read_exact(&mut body)?;
    Ok(Request {
        method,
        path,
        query,
        auth,
        body,
    })
}

fn respond(w: &mut impl Write, status: u16, body: &str) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        422 => "Unprocessable Entity",
        _ => "Internal Server Error",
    };
    write!(
        w,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    w.flush()
}

fn error(status: u16, message: &str) -> (u16, String) {
    (status, json!({ "error": message }).to_string())
}

/// Compares in constant time, so the answer's timing gives the token away
/// one byte at a time to nobody.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

fn query_u64(query: &str, key: &str) -> Option<u64> {
    query
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .find(|(k, _)| *k == key)
        .and_then(|(_, v)| v.parse().ok())
}

#[derive(Deserialize)]
struct Say {
    text: String,
    #[serde(default)]
    card: Option<String>,
}

#[derive(Deserialize)]
struct Add {
    title: String,
    #[serde(default)]
    project: String,
    #[serde(default)]
    priority: String,
    #[serde(default)]
    mine: bool,
    #[serde(default)]
    note: String,
}

fn route(server: &Server, req: &Request) -> (u16, String) {
    let bearer = req
        .auth
        .as_deref()
        .and_then(|a| a.strip_prefix("Bearer "))
        .unwrap_or("");
    if !same(bearer.trim(), &server.token) {
        return error(
            401,
            "This phone is not paired. Run waybill pair on the desk.",
        );
    }
    let path = req.path.trim_end_matches('/');
    match (req.method.as_str(), path) {
        ("GET", "/v1/hello") => {
            let (snap, version) = server.snap();
            (
                200,
                json!({
                    "wire": wire::VERSION,
                    "version": version,
                    "you": snap.you,
                    "read_only": server.demo,
                })
                .to_string(),
            )
        }
        ("GET", "/v1/screen") => (
            200,
            server
                .screen_after(query_u64(&req.query, "after"))
                .to_string(),
        ),
        ("GET", "/v1/glance") => {
            let cur = server.current.lock().unwrap_or_else(|e| e.into_inner());
            (200, cur.glance.to_string())
        }
        ("GET", p) if p.starts_with("/v1/card/") => {
            let id = &p["/v1/card/".len()..];
            let (snap, _) = server.snap();
            match snap.cards.iter().find(|c| c.uuid == id) {
                Some(c) => {
                    let note = server.note(c);
                    let page = wire::page(c, note.as_deref());
                    (200, serde_json::to_string(&page).unwrap_or_default())
                }
                None => error(404, "That card is not on the list any more."),
            }
        }
        ("POST", "/v1/say") => match serde_json::from_slice::<Say>(&req.body) {
            Ok(s) => say_it(server, s),
            Err(e) => error(400, &format!("A say is {{\"text\", \"card\"}}: {e}")),
        },
        ("POST", "/v1/add") => match serde_json::from_slice::<Add>(&req.body) {
            Ok(a) => add(server, a),
            Err(e) => error(400, &format!("An add needs a title: {e}")),
        },
        _ => error(404, "Nothing lives there."),
    }
}

fn run_action(server: &Server, action: Action, said: String) -> (u16, String) {
    let done = server
        .desk()
        .and_then(|desk| say::perform(&desk, action, clock::now()));
    match done {
        Ok(result) => {
            if let Err(e) = server.refresh() {
                eprintln!("waybill serve: could not reread the list: {e:#}");
            }
            let (_, version) = server.snap();
            (
                200,
                json!({ "said": said, "result": result, "version": version }).to_string(),
            )
        }
        Err(e) if server.demo => error(403, &format!("{e:#}")),
        Err(e) => error(422, &format!("{e:#}")),
    }
}

fn say_it(server: &Server, s: Say) -> (u16, String) {
    let (snap, _) = server.snap();
    let card = match s.card.as_deref().filter(|c| !c.is_empty()) {
        Some(id) => match snap.cards.iter().find(|c| c.uuid == id) {
            Some(c) => Some(c.uuid.clone()),
            None => return error(404, "That card is not on the list any more."),
        },
        None => None,
    };
    match say::parse(&s.text, card.as_deref()) {
        Some((Command::Do(action), said)) => run_action(server, action, said),
        Some((Command::View(view), said)) => {
            (200, json!({ "said": said, "view": view.id() }).to_string())
        }
        Some((Command::New(title), said)) => {
            (200, json!({ "said": said, "new": title }).to_string())
        }
        None => error(
            422,
            "Waybill does not know those words. Try start, done, high, mine, park or visit ...",
        ),
    }
}

fn add(server: &Server, a: Add) -> (u16, String) {
    let title = a.title.trim();
    if title.is_empty() {
        return error(400, "A new task needs a title.");
    }
    let priority = match a.priority.trim() {
        "" => None,
        p @ ("H" | "M" | "L") => p.chars().next(),
        p => return error(400, &format!("A priority is H, M or L, not {p:?}.")),
    };
    let project = Some(a.project.trim().to_string()).filter(|p| !p.is_empty());
    let new = NewTask {
        title: title.to_string(),
        project,
        priority,
        ..NewTask::default()
    };
    run_action(
        server,
        Action::Add {
            new,
            note: a.note,
            mine: a.mine,
        },
        format!("New task: {title}"),
    )
}

trait Stream: Read + Write {}
impl<T: Read + Write> Stream for T {}

fn tls_config(cfg: &Config) -> anyhow::Result<Option<Arc<rustls::ServerConfig>>> {
    let (cert, key) = (cfg.serve.cert.trim(), cfg.serve.key.trim());
    if cert.is_empty() && key.is_empty() {
        return Ok(None);
    }
    if cert.is_empty() || key.is_empty() {
        bail!("serve.cert and serve.key go together");
    }
    let (cert, key) = (paths::expand(cert), paths::expand(key));
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(&cert)
        .with_context(|| format!("could not read {}", cert.display()))?
        .collect::<Result<_, _>>()
        .with_context(|| format!("{} is not a PEM certificate", cert.display()))?;
    let key = PrivateKeyDer::from_pem_file(&key)
        .with_context(|| format!("could not read the key in {}", key.display()))?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let tls = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .context("the certificate and key do not match")?;
    Ok(Some(Arc::new(tls)))
}

fn handle(server: &Server, tcp: TcpStream, tls: Option<Arc<rustls::ServerConfig>>) {
    let peer = tcp
        .peer_addr()
        .map(|a| a.ip().to_string())
        .unwrap_or_default();
    let _ = tcp.set_read_timeout(Some(Duration::from_secs(15)));
    let _ = tcp.set_write_timeout(Some(Duration::from_secs(15)));
    let mut stream: Box<dyn Stream> = match tls {
        Some(cfg) => match rustls::ServerConnection::new(cfg) {
            Ok(conn) => Box::new(rustls::StreamOwned::new(conn, tcp)),
            Err(_) => return,
        },
        None => Box::new(tcp),
    };
    let req = match read_request(&mut stream) {
        Ok(r) if !r.method.is_empty() => r,
        _ => return,
    };
    let (status, body) = route(server, &req);
    let _ = respond(&mut stream, status, &body);
    eprintln!("{peer} {} {} {status}", req.method, req.path);
}

/// What the task database, the notes and the claims look like on disk.
fn watched(cfg: &Config) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = waybill_core::load::data_dir(cfg).into_iter().collect();
    dirs.extend(waybill_core::load::notes_dir(cfg));
    dirs.push(waybill_core::claim::dir());
    dirs
}

pub fn run(demo: bool, listen: Option<&str>) -> anyhow::Result<()> {
    let cfg = Server::config();
    let tls = tls_config(&cfg)?;
    let listen = listen.unwrap_or(&cfg.serve.listen).to_string();
    let server = Arc::new(Server {
        demo,
        token: token(false)?,
        current: Mutex::new(Current {
            version: 0,
            snap: Arc::new(Snapshot::default()),
            screen: Arc::new(String::new()),
            glance: Arc::new(String::new()),
            plain: String::new(),
        }),
        changed: Condvar::new(),
        reading: Mutex::new(()),
    });
    server.refresh()?;
    let socket =
        TcpListener::bind(&listen).with_context(|| format!("could not listen on {listen}"))?;
    eprintln!(
        "waybill serve: {} on {listen}{}; pair a phone with waybill pair",
        if tls.is_some() { "HTTPS" } else { "HTTP" },
        if demo { ", the demo list" } else { "" },
    );

    // Reread when the database, a note or a claim changes, and every minute
    // so that times keep counting.
    let watcher = server.clone();
    std::thread::spawn(move || {
        loop {
            let dirs = if watcher.demo {
                Vec::new()
            } else {
                watched(&Server::config())
            };
            let seen = crate::bar::signature(&dirs);
            for _ in 0..30 {
                std::thread::sleep(Duration::from_secs(2));
                if crate::bar::signature(&dirs) != seen {
                    std::thread::sleep(Duration::from_millis(400));
                    break;
                }
            }
            if let Err(e) = watcher.refresh() {
                eprintln!("waybill serve: could not reread the list: {e:#}");
            }
        }
    });

    for tcp in socket.incoming().flatten() {
        let (server, tls) = (server.clone(), tls.clone());
        std::thread::spawn(move || handle(&server, tcp, tls));
    }
    Ok(())
}

/// What `waybill doctor` says about the phone: whether one is paired, and
/// whether HTTPS can start.
pub fn check(cfg: &Config) -> waybill_core::doctor::Check {
    use waybill_core::doctor::Check;
    if let Err(e) = tls_config(cfg) {
        return Check::fail("phone", format!("{e:#}"));
    }
    if !token_file().exists() {
        return Check::info("phone", "not paired; waybill pair shows the link");
    }
    let scheme = if cfg.serve.cert.is_empty() {
        "HTTP"
    } else {
        "HTTPS"
    };
    Check::ok(
        "phone",
        format!("paired; waybill serve speaks {scheme} at {}", url(cfg)),
    )
}

/// Prints the pairing link, and a QR code of it when `qrencode` is there.
pub fn pair(fresh: bool, url_override: Option<&str>) -> anyhow::Result<()> {
    let cfg = Server::config();
    let url = url_override
        .map(|u| u.trim_end_matches('/').to_string())
        .unwrap_or_else(|| url(&cfg));
    let link = pair_link(&url, &token(fresh)?);
    let qr = std::process::Command::new("qrencode")
        .args(["-t", "ANSIUTF8", "-m", "2", &link])
        .output();
    if let Ok(out) = qr
        && out.status.success()
    {
        std::io::stdout().write_all(&out.stdout)?;
    }
    println!("Scan it with the phone's camera, or open this link on the phone:\n\n{link}\n");
    println!("The phone reaches the list at {url}. Keep `waybill serve` running there.");
    if url.starts_with("http:") {
        println!(
            "That is plain HTTP: anyone on the network can read what passes. \
             Set serve.cert and serve.key to serve HTTPS."
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pair_link_carries_the_server_and_the_token() {
        assert_eq!(
            pair_link("https://box.lan:8446", "ab12"),
            "waybill://pair?url=https%3A%2F%2Fbox.lan%3A8446&token=ab12"
        );
    }

    #[test]
    fn reads_a_request() {
        let raw = b"POST /v1/say?x=1 HTTP/1.1\r\nHost: a\r\nAuthorization: Bearer t0k\r\n\
                    Content-Length: 15\r\n\r\n{\"text\":\"high\"}";
        let r = read_request(&mut &raw[..]).unwrap();
        assert_eq!((r.method.as_str(), r.path.as_str()), ("POST", "/v1/say"));
        assert_eq!(r.query, "x=1");
        assert_eq!(r.auth.as_deref(), Some("Bearer t0k"));
        assert_eq!(r.body, b"{\"text\":\"high\"}");
        assert_eq!(query_u64("after=7&x=1", "after"), Some(7));
    }

    #[test]
    fn the_token_must_match() {
        assert!(same("abc", "abc"));
        assert!(!same("abc", "abd"));
        assert!(!same("abc", "abcd"));
    }
}
