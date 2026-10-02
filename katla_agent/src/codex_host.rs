//! Attach to an already loaded conversation in an existing Codex app-server.
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

/// Explicit private local host and existing conversation chosen by the user.
#[derive(Clone, Debug)]
pub struct CodexHostConfig {
    /// Unix control socket of the existing conversation owner.
    pub socket: PathBuf,
    /// Existing conversation ID. This client never creates a conversation.
    pub thread_id: String,
}

/// Events mirrored from the external conversation, without owning its history.
#[derive(Debug)]
pub enum CodexHostEvent {
    /// The existing conversation has been joined.
    Connected { name: Option<String> },
    /// An editor request was accepted for this host turn.
    Accepted(String),
    /// Assistant text scoped to one turn and message item.
    Text {
        turn_id: String,
        item_id: String,
        delta: String,
    },
    /// The external turn completed, failed or was interrupted.
    Finished { turn_id: String, status: String },
    /// A request failed; no internal assistant is started.
    Error(String),
    /// Connection ended; explicit reconnection is required.
    Disconnected(String),
    /// A server request needs the main host client's attention.
    HostAttention,
}

struct EditorQuestion {
    text: String,
    metadata: Value,
    png_base64: String,
}

/// Nonblocking client of the existing host's supported stdio proxy.
pub struct CodexHostBridge {
    requests: mpsc::Sender<EditorQuestion>,
    events: mpsc::Receiver<CodexHostEvent>,
    stop: Arc<AtomicBool>,
}

impl CodexHostBridge {
    /// Connect asynchronously. Only the proxy child is owned by this bridge.
    pub fn connect(config: CodexHostConfig) -> Self {
        let (requests, receiver) = mpsc::channel();
        let (sender, events) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        std::thread::spawn(move || {
            if let Err(error) = run(config, receiver, &sender, &worker_stop) {
                let _ = sender.send(CodexHostEvent::Disconnected(error));
            }
        });
        Self {
            requests,
            events,
            stop,
        }
    }

    /// Send a user question with the committed image and metadata of their view.
    pub fn submit(&self, text: String, metadata: Value, png_base64: String) -> Result<(), String> {
        self.requests
            .send(EditorQuestion {
                text,
                metadata,
                png_base64,
            })
            .map_err(|_| "External host is disconnected".into())
    }

    /// Drain host events on the editor thread.
    pub fn poll(&self) -> Vec<CodexHostEvent> {
        self.events.try_iter().collect()
    }
}

fn run(
    config: CodexHostConfig,
    requests: mpsc::Receiver<EditorQuestion>,
    events: &mpsc::Sender<CodexHostEvent>,
    stop: &AtomicBool,
) -> Result<(), String> {
    validate_config(&config)?;
    let mut command = Command::new("codex");
    command
        .args(["app-server", "proxy", "--sock"])
        .arg(&config.socket);
    let session = Session::spawn(command, config.thread_id, events.clone())?;
    run_session(session, requests, events, stop)
}

fn run_session(
    mut session: Session,
    requests: mpsc::Receiver<EditorQuestion>,
    events: &mpsc::Sender<CodexHostEvent>,
    stop: &AtomicBool,
) -> Result<(), String> {
    if stop.load(Ordering::Acquire) {
        return Ok(());
    }
    let thread_id = session.thread_id.clone();
    session.rpc("initialize", json!({"clientInfo":{"name":"katla_editor","title":"Katla editor","version":env!("CARGO_PKG_VERSION")}}))?;
    session.write(json!({"method":"initialized"}))?;
    let mut cursor = None;
    loop {
        let loaded = session.rpc("thread/loaded/list", json!({"cursor":cursor}))?;
        if require_loaded_thread(&loaded, &thread_id).is_ok() {
            break;
        }
        let next = loaded["nextCursor"].as_str().map(str::to_owned);
        if next.is_none() || next == cursor {
            require_loaded_thread(&loaded, &thread_id)?;
        }
        cursor = next;
    }
    let joined = session.rpc("thread/resume", json!({"threadId":thread_id}))?;
    if joined["thread"]["id"].as_str() != Some(&thread_id) {
        return Err("Host returned a different conversation; refusing to send".into());
    }
    if stop.load(Ordering::Acquire) {
        return Ok(());
    }
    let _ = events.send(CodexHostEvent::Connected {
        name: joined["thread"]["name"].as_str().map(str::to_owned),
    });
    while !stop.load(Ordering::Acquire) {
        match requests.recv_timeout(Duration::from_millis(20)) {
            Ok(question) => {
                let result = (|| {
                    let state = session.rpc(
                        "thread/read",
                        json!({"threadId":thread_id,"includeTurns":true}),
                    )?;
                    if stop.load(Ordering::Acquire) {
                        return Err("Connection replaced before the question was sent".into());
                    }
                    let (method, params) = question_request(&thread_id, &state, question)?;
                    let accepted = session.rpc(method, params)?;
                    accepted["turn"]["id"]
                        .as_str()
                        .or_else(|| accepted["turnId"].as_str())
                        .map(str::to_owned)
                        .ok_or("Host accepted response has no turn ID".into())
                })();
                let event = match result {
                    Ok(turn_id) => CodexHostEvent::Accepted(turn_id),
                    Err(error) => CodexHostEvent::Error(error),
                };
                let _ = events.send(event);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
        }
        loop {
            match session.messages.try_recv() {
                Ok(message) => session.notification(message?),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err("External host disconnected (EOF)".into());
                }
            }
        }
    }
    Ok(())
}

impl Drop for CodexHostBridge {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

fn validate_config(config: &CodexHostConfig) -> Result<(), String> {
    if config.thread_id.trim().is_empty() || !config.socket.is_absolute() {
        return Err("Choose an existing conversation ID and an absolute host socket path".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{FileTypeExt, PermissionsExt};
        let metadata = std::fs::symlink_metadata(&config.socket)
            .map_err(|e| format!("Host socket unavailable: {e}"))?;
        if !metadata.file_type().is_socket() || metadata.permissions().mode() & 0o077 != 0 {
            return Err("Host connection requires a private Unix socket (0600)".into());
        }
        Ok(())
    }
    #[cfg(not(unix))]
    Err("Local Codex host attachment currently requires Unix".into())
}

fn require_loaded_thread(loaded: &Value, thread_id: &str) -> Result<(), String> {
    if loaded["data"]
        .as_array()
        .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(thread_id)))
    {
        Ok(())
    } else {
        Err("Conversation is not loaded in this host. Connect to its existing owner; Katla will not start another session".into())
    }
}

fn question_request(
    thread_id: &str,
    state: &Value,
    question: EditorQuestion,
) -> Result<(&'static str, Value), String> {
    if state["thread"]["id"].as_str() != Some(thread_id) {
        return Err("Host returned a different conversation".into());
    }
    let status = state["thread"]["status"]["type"].as_str();
    if !matches!(status, Some("idle" | "active")) {
        return Err("Existing host conversation is not ready; no turn was started".into());
    }
    let input = json!([
        {"type":"text","text":format!("{}\n\nKatla editor view at submission {}. Geometry candidates are not proof of occlusion visibility or room membership. Use scene/spatial queries beyond the frustum when needed.\n{}",question.text,question.metadata["submission"],question.metadata)},
        {"type":"image","url":format!("data:image/png;base64,{}",question.png_base64)}
    ]);
    let active = state["thread"]["turns"].as_array().and_then(|turns| {
        turns
            .iter()
            .rev()
            .find(|turn| turn["status"] == "inProgress")
    });
    if let Some(turn) = active {
        let id = turn["id"].as_str().ok_or("Active host turn has no ID")?;
        Ok((
            "turn/steer",
            json!({"threadId":thread_id,"expectedTurnId":id,"input":input}),
        ))
    } else if status == Some("active") {
        Err("Host is busy but has not exposed its active turn ID; retry after it finishes".into())
    } else {
        Ok(("turn/start", json!({"threadId":thread_id,"input":input})))
    }
}

struct Session {
    child: Child,
    input: ChildStdin,
    messages: mpsc::Receiver<Result<Value, String>>,
    sequence: u64,
    thread_id: String,
    events: mpsc::Sender<CodexHostEvent>,
}

impl Session {
    fn spawn(
        mut command: Command,
        thread_id: String,
        events: mpsc::Sender<CodexHostEvent>,
    ) -> Result<Self, String> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("Cannot launch Codex host proxy: {e}"))?;
        let input = child.stdin.take().ok_or("Proxy stdin missing")?;
        let output = child.stdout.take().ok_or("Proxy stdout missing")?;
        let (sender, messages) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let value = line.map_err(|e| e.to_string()).and_then(|s| {
                    serde_json::from_str(&s).map_err(|e| format!("Invalid host response: {e}"))
                });
                if sender.send(value).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            input,
            messages,
            sequence: 0,
            thread_id,
            events,
        })
    }
    fn write(&mut self, value: Value) -> Result<(), String> {
        writeln!(self.input, "{value}")
            .and_then(|()| self.input.flush())
            .map_err(|e| format!("Host connection failed: {e}"))
    }
    fn rpc(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.sequence += 1;
        let id = self.sequence;
        self.write(json!({"id":id,"method":method,"params":params}))?;
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let message = self
                .messages
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|e| format!("Host did not respond: {e}"))??;
            if message["id"].as_u64() == Some(id) && message.get("method").is_none() {
                return match message.get("error") {
                    Some(error) => Err(format!("Host rejected {method}: {error}")),
                    None => message
                        .get("result")
                        .cloned()
                        .ok_or("Missing host result".into()),
                };
            }
            self.notification(message);
        }
    }
    fn notification(&self, value: Value) {
        if value.get("id").is_some() {
            if value.get("method").is_some() {
                // A secondary view never answers server requests. The main
                // host remains responsible for approvals, tools and identity.
                let _ = self.events.send(CodexHostEvent::HostAttention);
            }
            return;
        }
        if value["params"]["threadId"].as_str() != Some(&self.thread_id) {
            return;
        }
        let event = match value["method"].as_str() {
            Some("item/agentMessage/delta") => {
                let p = &value["params"];
                match (
                    p["turnId"].as_str(),
                    p["itemId"].as_str(),
                    p["delta"].as_str(),
                ) {
                    (Some(turn), Some(item), Some(delta)) => Some(CodexHostEvent::Text {
                        turn_id: turn.into(),
                        item_id: item.into(),
                        delta: delta.into(),
                    }),
                    _ => None,
                }
            }
            Some("turn/completed") => {
                value["params"]["turn"]["id"]
                    .as_str()
                    .map(|turn| CodexHostEvent::Finished {
                        turn_id: turn.into(),
                        status: value["params"]["turn"]["status"]
                            .as_str()
                            .unwrap_or("unknown")
                            .into(),
                    })
            }
            Some("error") => Some(CodexHostEvent::Error(value["params"]["error"].to_string())),
            _ => None,
        };
        if let Some(event) = event {
            let _ = self.events.send(event);
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn question() -> EditorQuestion {
        EditorQuestion {
            text: "Furnish this room".into(),
            metadata: json!({"submission":42,"selected_entity_id":null}),
            png_base64: "PNG".into(),
        }
    }
    #[test]
    fn test_requires_existing_loaded_owner() {
        assert!(require_loaded_thread(&json!({"data":["owner"]}), "owner").is_ok());
        assert!(require_loaded_thread(&json!({"data":[]}), "owner").is_err());
    }
    #[test]
    fn test_idle_request_preserves_host_policy_and_carries_view() {
        let (method, p) = question_request(
            "owner",
            &json!({"thread":{"id":"owner","turns":[],"status":{"type":"idle"}}}),
            question(),
        )
        .unwrap();
        assert_eq!(method, "turn/start");
        assert_eq!(p["threadId"], "owner");
        assert!(p.get("sandboxPolicy").is_none());
        assert!(p.get("approvalPolicy").is_none());
        assert_eq!(p["input"][1]["url"], "data:image/png;base64,PNG");
        assert!(
            p["input"][0]["text"]
                .as_str()
                .unwrap()
                .contains("submission 42")
        );
    }
    #[test]
    fn test_busy_request_uses_matching_turn_without_fork() {
        let (m,p)=question_request("owner",&json!({"thread":{"id":"owner","turns":[{"id":"turn-a","status":"inProgress"}],"status":{"type":"active"}}}),question()).unwrap();
        assert_eq!(m, "turn/steer");
        assert_eq!(p["expectedTurnId"], "turn-a");
        assert!(
            question_request("owner", &json!({"thread":{"id":"another"}}), question()).is_err()
        );
    }
    #[test]
    fn test_unloaded_or_failed_host_cannot_start_another_turn() {
        for status in ["notLoaded", "systemError", "unknown"] {
            assert!(
                question_request(
                    "owner",
                    &json!({"thread":{"id":"owner","status":{"type":status},"turns":[]}}),
                    question()
                )
                .is_err()
            );
        }
    }

    #[test]
    fn test_pipe_session_keeps_approval_unanswered_and_detects_eof() {
        let script = r#"import sys,json
for line in sys.stdin:
 r=json.loads(line)
 if r.get('method') == 'probe':
  print(json.dumps({'id':r['id'],'method':'item/commandExecution/requestApproval','params':{'threadId':'owner'}}),flush=True)
  print(json.dumps({'id':r['id'],'result':{'ok':True}}),flush=True)
 elif r.get('method') == 'finish':
  print(json.dumps({'id':r['id'],'result':{}}),flush=True)
  break
 else: raise RuntimeError('Unexpected approval response')
"#;
        let mut command = Command::new("python3");
        command.args(["-u", "-c", script]);
        let (events, receiver) = mpsc::channel();
        let mut session = Session::spawn(command, "owner".into(), events).unwrap();
        assert_eq!(session.rpc("probe", json!({})).unwrap()["ok"], true);
        assert!(matches!(
            receiver.recv().unwrap(),
            CodexHostEvent::HostAttention
        ));
        session.rpc("finish", json!({})).unwrap();
        assert!(matches!(
            session.messages.recv_timeout(Duration::from_secs(2)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
    }
    #[test]
    fn test_pipe_host_handshake_pagination_same_thread_view_and_stream() {
        let script = r#"import sys,json
for line in sys.stdin:
 r=json.loads(line); m=r['method']; p=r.get('params',{})
 if m == 'initialized': continue
 if m == 'initialize': result={}
 elif m == 'thread/loaded/list':
  result={'data':['other'],'nextCursor':'page2'} if p.get('cursor') is None else {'data':['owner'],'nextCursor':None}
 elif m == 'thread/resume':
  assert p == {'threadId':'owner'}
  result={'thread':{'id':'owner','name':'My room'}}
 elif m == 'thread/read':
  assert p == {'threadId':'owner','includeTurns':True}
  result={'thread':{'id':'owner','status':{'type':'idle'},'turns':[]}}
 elif m == 'turn/start':
  assert set(p) == {'threadId','input'} and p['threadId'] == 'owner'
  assert p['input'][1]['url'] == 'data:image/png;base64,PNG'
  assert 'submission 42' in p['input'][0]['text']
  print(json.dumps({'method':'item/agentMessage/delta','params':{'threadId':'other','turnId':'foreign','itemId':'x','delta':'wrong'}}),flush=True)
  print(json.dumps({'id':r['id'],'method':'item/commandExecution/requestApproval','params':{'threadId':'owner'}}),flush=True)
  print(json.dumps({'method':'item/agentMessage/delta','params':{'threadId':'owner','turnId':'turn-a','itemId':'answer','delta':'Hello'}}),flush=True)
  result={'turn':{'id':'turn-a'}}
 else: raise RuntimeError('Unexpected request or approval response: '+str(r))
 print(json.dumps({'id':r['id'],'result':result}),flush=True)
 if m == 'turn/start':
  print(json.dumps({'method':'turn/completed','params':{'threadId':'owner','turn':{'id':'turn-a','status':'completed'}}}),flush=True)
  break
"#;
        let mut command = Command::new("python3");
        command.args(["-u", "-c", script]);
        let (events, receiver) = mpsc::channel();
        let session = Session::spawn(command, "owner".into(), events.clone()).unwrap();
        let (questions, requests) = mpsc::channel();
        questions.send(question()).unwrap();
        let error = run_session(session, requests, &events, &AtomicBool::new(false)).unwrap_err();
        assert!(error.contains("EOF"));
        let received: Vec<_> = receiver.try_iter().collect();
        assert!(
            matches!(&received[0], CodexHostEvent::Connected { name: Some(n) } if n == "My room")
        );
        assert!(matches!(&received[1], CodexHostEvent::HostAttention));
        assert!(
            matches!(&received[2], CodexHostEvent::Text {turn_id, item_id, delta} if turn_id == "turn-a" && item_id == "answer" && delta == "Hello")
        );
        assert!(matches!(&received[3], CodexHostEvent::Accepted(id) if id == "turn-a"));
        assert!(
            matches!(&received[4], CodexHostEvent::Finished {status,..} if status == "completed")
        );
        assert_eq!(received.len(), 5);
    }
}
