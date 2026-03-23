use crate::graph::NodeId;
use std::collections::HashMap;
use std::sync::mpsc;

#[derive(Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

pub enum HttpAction {
    SendRequest {
        node_id: NodeId,
        url: String,
        method: String,
        headers: Vec<(String, String)>,
        body: String,
    },
}

struct PendingRequest {
    rx: mpsc::Receiver<HttpResponse>,
}

pub struct HttpManager {
    pending: HashMap<NodeId, PendingRequest>,
}

impl HttpManager {
    pub fn new() -> Self {
        Self { pending: HashMap::new() }
    }

    pub fn process(&mut self, actions: Vec<HttpAction>) {
        for action in actions {
            match action {
                HttpAction::SendRequest { node_id, url, method, headers, body } => {
                    let (tx, rx) = mpsc::channel();
                    self.pending.insert(node_id, PendingRequest { rx });
                    std::thread::spawn(move || {
                        let client = reqwest::blocking::Client::builder()
                            .timeout(std::time::Duration::from_secs(60))
                            .build()
                            .unwrap_or_else(|_| reqwest::blocking::Client::new());

                        let mut req = if method.to_uppercase() == "GET" {
                            client.get(&url)
                        } else {
                            client.post(&url)
                        };

                        for (key, val) in &headers {
                            req = req.header(key.as_str(), val.as_str());
                        }

                        if method.to_uppercase() != "GET" && !body.is_empty() {
                            req = req.body(body);
                        }

                        let resp = match req.send() {
                            Ok(r) => {
                                let status = r.status().as_u16();
                                let body = r.text().unwrap_or_default();
                                HttpResponse { status, body }
                            }
                            Err(e) => HttpResponse {
                                status: 0,
                                body: format!("Error: {}", e),
                            },
                        };
                        let _ = tx.send(resp);
                    });
                }
            }
        }
    }

    pub fn poll(&mut self, node_id: NodeId) -> Option<HttpResponse> {
        if let Some(pending) = self.pending.get(&node_id) {
            match pending.rx.try_recv() {
                Ok(resp) => {
                    self.pending.remove(&node_id);
                    Some(resp)
                }
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending.remove(&node_id);
                    None
                }
            }
        } else {
            None
        }
    }

    pub fn is_pending(&self, node_id: NodeId) -> bool {
        self.pending.contains_key(&node_id)
    }
}
