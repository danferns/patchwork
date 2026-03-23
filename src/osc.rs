use crate::graph::NodeId;
use rosc::{OscMessage, OscPacket, OscType};
use std::collections::HashMap;
use std::net::UdpSocket;
use std::sync::mpsc;

pub enum OscAction {
    Send { node_id: NodeId, host: String, port: u16, address: String, args: Vec<f32> },
    StartListening { node_id: NodeId, port: u16 },
    StopListening { node_id: NodeId },
}

struct Listener {
    _thread: std::thread::JoinHandle<()>,
    rx: mpsc::Receiver<(String, Vec<f32>)>,
}

pub struct OscManager {
    listeners: HashMap<NodeId, Listener>,
    send_socket: Option<UdpSocket>,
    last_sent: HashMap<NodeId, Vec<u8>>,
}

impl OscManager {
    pub fn new() -> Self {
        Self {
            listeners: HashMap::new(),
            send_socket: UdpSocket::bind("0.0.0.0:0").ok(),
            last_sent: HashMap::new(),
        }
    }

    pub fn process(&mut self, actions: Vec<OscAction>) {
        for action in actions {
            match action {
                OscAction::Send { node_id, host, port, address, args } => {
                    let osc_args: Vec<OscType> = args.iter().map(|&v| OscType::Float(v)).collect();
                    let msg = OscMessage { addr: address, args: osc_args };
                    let packet = OscPacket::Message(msg);
                    if let Ok(buf) = rosc::encoder::encode(&packet) {
                        // Change detection
                        if self.last_sent.get(&node_id) == Some(&buf) {
                            continue;
                        }
                        if let Some(sock) = &self.send_socket {
                            let addr = format!("{}:{}", host, port);
                            let _ = sock.send_to(&buf, &addr);
                        }
                        self.last_sent.insert(node_id, buf);
                    }
                }
                OscAction::StartListening { node_id, port } => {
                    if self.listeners.contains_key(&node_id) {
                        continue;
                    }
                    let bind_addr = format!("0.0.0.0:{}", port);
                    let socket = match UdpSocket::bind(&bind_addr) {
                        Ok(s) => s,
                        Err(e) => {
                            eprintln!("OSC bind error on port {}: {}", port, e);
                            continue;
                        }
                    };
                    socket.set_nonblocking(false).ok();
                    socket.set_read_timeout(Some(std::time::Duration::from_millis(10))).ok();

                    let (tx, rx) = mpsc::channel();
                    let handle = std::thread::spawn(move || {
                        let mut buf = [0u8; 4096];
                        loop {
                            match socket.recv_from(&mut buf) {
                                Ok((size, _)) => {
                                    if let Ok((_, packet)) = rosc::decoder::decode_udp(&buf[..size]) {
                                        Self::extract_messages(&packet, &tx);
                                    }
                                }
                                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                    std::thread::sleep(std::time::Duration::from_millis(1));
                                }
                                Err(_) => break,
                            }
                        }
                    });
                    self.listeners.insert(node_id, Listener { _thread: handle, rx });
                }
                OscAction::StopListening { node_id } => {
                    self.listeners.remove(&node_id);
                }
            }
        }
    }

    fn extract_messages(packet: &OscPacket, tx: &mpsc::Sender<(String, Vec<f32>)>) {
        match packet {
            OscPacket::Message(msg) => {
                let args: Vec<f32> = msg.args.iter().map(|a| match a {
                    OscType::Float(f) => *f,
                    OscType::Int(i) => *i as f32,
                    OscType::Double(d) => *d as f32,
                    OscType::Long(l) => *l as f32,
                    _ => 0.0,
                }).collect();
                let _ = tx.send((msg.addr.clone(), args));
            }
            OscPacket::Bundle(bundle) => {
                for p in &bundle.content {
                    Self::extract_messages(p, tx);
                }
            }
        }
    }

    pub fn poll(&mut self, node_id: NodeId) -> Vec<(String, Vec<f32>)> {
        let mut messages = Vec::new();
        if let Some(listener) = self.listeners.get(&node_id) {
            while let Ok(msg) = listener.rx.try_recv() {
                messages.push(msg);
            }
        }
        messages
    }

    pub fn is_listening(&self, node_id: NodeId) -> bool {
        self.listeners.contains_key(&node_id)
    }

    pub fn cleanup_node(&mut self, node_id: NodeId) {
        self.listeners.remove(&node_id);
        self.last_sent.remove(&node_id);
    }
}
