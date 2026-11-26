//! `waybill serve` against a real `task` binary and a throwaway list: a
//! decision filed by an agent is settled from the phone's side of the wire.
//! Skipped when Taskwarrior is not installed.

use serde_json::Value;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn waybill(dir: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_waybill"));
    c.env("WAYBILL_CONFIG_DIR", dir.join("config"))
        .env("WAYBILL_RUNTIME_DIR", dir.join("run"));
    c
}

fn call(port: u16, token: &str, method: &str, path: &str, body: &str) -> (u16, Value) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(
        s,
        "{method} {path} HTTP/1.1\r\nHost: test\r\nAuthorization: Bearer {token}\r\n\
         Content-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    let status = out[9..12].parse().unwrap();
    let json = out.split_once("\r\n\r\n").map(|x| x.1).unwrap_or("");
    (status, serde_json::from_str(json).unwrap_or(Value::Null))
}

fn export(dir: &Path) -> Vec<Value> {
    let out = Command::new("task")
        .arg(format!("rc:{}", dir.join("taskrc").display()))
        .args(["rc.verbose=nothing", "rc.json.array=on", "export"])
        .output()
        .unwrap();
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn a_phone_settles_a_decision() {
    if Command::new("task").arg("--version").output().is_err() {
        eprintln!("skipped: no task binary");
        return;
    }
    let dir: PathBuf = std::env::temp_dir().join(format!("waybill-serve-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("data/notes")).unwrap();
    std::fs::create_dir_all(dir.join("config")).unwrap();
    std::fs::write(
        dir.join("taskrc"),
        format!(
            "data.location={}\nuda.ref.type=string\nuda.ref.label=Ref\nnews.version=3.5.0\n",
            dir.join("data").display()
        ),
    )
    .unwrap();
    std::fs::write(
        dir.join("config/config.toml"),
        format!(
            "[task]\ncommand = \"task rc:{}\"\n[tags]\nhands = [\"nate\"]\n[theme]\nfrom = \"none\"\n",
            dir.join("taskrc").display()
        ),
    )
    .unwrap();

    let asked = waybill(&dir)
        .args(["ask", "--json", "--project", "storefront"])
        .args(["--option", "Keep 4 s", "--option", "Lower it to 2 s"])
        .args(["--", "Settle the global cooldown"])
        .output()
        .unwrap();
    assert!(asked.status.success(), "{asked:?}");
    let uuid = serde_json::from_slice::<Value>(&asked.stdout).unwrap()["uuid"]
        .as_str()
        .unwrap()
        .to_string();

    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let _server = Server(
        waybill(&dir)
            .args(["serve", "--listen", &format!("127.0.0.1:{port}")])
            .spawn()
            .unwrap(),
    );
    let token = {
        let file = dir.join("config/phone.token");
        let start = Instant::now();
        loop {
            if let Ok(t) = std::fs::read_to_string(&file)
                && TcpStream::connect(("127.0.0.1", port)).is_ok()
            {
                break t.trim().to_string();
            }
            assert!(start.elapsed() < Duration::from_secs(10), "no server");
            std::thread::sleep(Duration::from_millis(100));
        }
    };

    assert_eq!(call(port, "wrong", "GET", "/v1/hello", "").0, 401);
    let (status, glance) = call(port, &token, "GET", "/v1/glance", "");
    assert_eq!(status, 200);
    assert_eq!(glance["lamp"]["state"], "needs-you");
    assert_eq!(glance["decisions"][0]["uuid"], uuid.as_str());
    assert_eq!(
        glance["options"][&uuid],
        serde_json::json!(["Keep 4 s", "Lower it to 2 s"])
    );

    // The button the phone shows is the words it sends.
    let (_, page) = call(port, &token, "GET", &format!("/v1/card/{uuid}"), "");
    let offer = page["offers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["label"] == "Keep 4 s")
        .unwrap();
    let words = offer["words"].as_str().unwrap();
    let (status, said) = call(
        port,
        &token,
        "POST",
        "/v1/say",
        &serde_json::json!({ "text": words, "card": uuid }).to_string(),
    );
    assert_eq!(status, 200, "{said}");
    assert_eq!(said["result"], "Ruled: Keep 4 s");

    let task = export(&dir)
        .into_iter()
        .find(|t| t["uuid"] == uuid.as_str())
        .unwrap();
    let tags = task["tags"].as_array().cloned().unwrap_or_default();
    assert!(!tags.contains(&"decision".into()), "back to the agents");
    assert_eq!(task["annotations"][0]["description"], "Nate: Keep 4 s");

    // The screen is already the new one: nothing waits on the user.
    let (_, screen) = call(port, &token, "GET", "/v1/screen", "");
    assert_eq!(screen["lamp"]["state"], "calm");
    assert_eq!(screen["columns"][0]["cards"][0], uuid.as_str());

    let (status, added) = call(
        port,
        &token,
        "POST",
        "/v1/add",
        r#"{"title": "Photograph the box", "project": "storefront", "mine": true}"#,
    );
    assert_eq!(status, 200, "{added}");
    assert!(export(&dir).iter().any(|t| {
        t["description"] == "Photograph the box"
            && t["tags"]
                .as_array()
                .is_some_and(|g| g.contains(&"nate".into()))
    }));
    let _ = std::fs::remove_dir_all(&dir);
}
