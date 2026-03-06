//! This example demonstrates an HTTP server that serves files from a directory.
//!
//! Checkout the `README.md` for guidance.

use core::num;
use std::{
    ascii, fs, io,
    net::SocketAddr,
    path::{self, Path, PathBuf},
    str,
    sync::Arc,
};

use anyhow::{Context, Result, anyhow, bail};
use clap::Parser;
use quinn_proto::VarInt;
use quinn_proto::crypto::rustls::QuicServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, pem::PemObject};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tracing::{error, info, info_span};

mod common;
use crate::common::card::{Card, shuffle};
use crate::common::net::{Message, NetworkMessage};
use postcard::from_bytes;

#[derive(Parser, Debug)]
#[clap(name = "server")]
struct Opt {
    /// file to log TLS keys to for debugging
    #[clap(long = "keylog")]
    keylog: bool,
    /// TLS private key in PEM format
    #[clap(short = 'k', long = "key", requires = "cert")]
    key: Option<PathBuf>,
    /// TLS certificate in PEM format
    #[clap(short = 'c', long = "cert", requires = "key")]
    cert: Option<PathBuf>,
    /// Enable stateless retries
    #[clap(long = "stateless-retry")]
    stateless_retry: bool,
    /// Address to listen on
    #[clap(long = "listen", default_value = "[::1]:4433")]
    listen: SocketAddr,
    /// Client address to block
    #[clap(long = "block")]
    block: Option<SocketAddr>,
    /// Maximum number of concurrent connections to allow
    #[clap(long = "connection-limit")]
    connection_limit: Option<usize>,
}
use rustls::crypto::{CryptoProvider, ring};

pub struct UserINFO {
    pub id: String,
    pub host: String,
    pub port: u16,
}

use std::collections::HashMap;

use tokio::sync::Mutex;

struct Client {
    id: String,
    send: mpsc::UnboundedSender<Message>,
    state: u8,
}

impl Client {
    pub fn new(send: mpsc::UnboundedSender<Message>) -> Self {
        Self {
            id: "".to_owned(),
            send,
            state: 0u8,
        }
    }
}

type ClientMap = Arc<Mutex<HashMap<SocketAddr, Client>>>;

#[tokio::main]
async fn main() -> Result<()> {
    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .with_file(true)
            .finish(),
    )
    .unwrap();
    CryptoProvider::install_default(ring::default_provider())
        .expect("Failed to install default CryptoProvider");

    let addr: SocketAddr = "127.0.0.1:4433".parse()?;

    let (certs, key) = {
        let cert_path = Path::new("cert.der");
        let key_path = Path::new("key.der");
        let (cert, key) = match fs::read(&cert_path).and_then(|x| Ok((x, fs::read(&key_path)?))) {
            Ok((cert, key)) => (
                CertificateDer::from(cert),
                PrivateKeyDer::try_from(key).map_err(anyhow::Error::msg)?,
            ),
            Err(ref e) if e.kind() == io::ErrorKind::NotFound => {
                println!("发生错误{:?}", e);
                panic!("generating self-signed certificate");
            }
            Err(e) => {
                bail!("failed to read certificate: {}", e);
            }
        };

        (vec![cert], key)
    };

    let mut server_crypto = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;

    server_crypto.alpn_protocols = common::ALPN_QUIC_HTTP.iter().map(|&x| x.into()).collect();

    let mut server_config =
        quinn::ServerConfig::with_crypto(Arc::new(QuicServerConfig::try_from(server_crypto)?));
    let transport_config = Arc::get_mut(&mut server_config.transport).unwrap();
    transport_config.max_concurrent_uni_streams(0_u8.into());
    // 设置空闲超时时间为 60 秒（可选，单位是毫秒）
    transport_config.max_idle_timeout(Some(VarInt::from_u32(60_000).into()));

    // 启用 keep-alive：每 20 秒发送一次 PING（如果连接空闲）
    transport_config.keep_alive_interval(Some(std::time::Duration::from_secs(20)));

    let endpoint = quinn::Endpoint::server(server_config, addr)?;
    println!("📡 Chat server listening on {}", addr);

    let clients: ClientMap = Arc::new(Mutex::new(HashMap::new()));
    while let Some(conn) = endpoint.accept().await {
        // 为每个连接开一个双向流用于收发消息
        let clients_clone = clients.clone();
        let fut = handle_client(conn, clients_clone);

        tokio::spawn(async move {
            if let Err(e) = fut.await {
                eprintln!("Client error {}", e);
            }
        });
    }

    Ok(())
}

async fn handle_client(conn: quinn::Incoming, clients: ClientMap) -> Result<()> {
    let connection = conn.await?;
    let remote_addr = connection.remote_address();
    println!("✅ New connection from {}", remote_addr);
    // 每个客户端使用 **一个双向流** 进行聊天（简单模型）
    let (mut send_stream, mut recv_stream) = connection.accept_bi().await?;

    // 2. 创建一个通道：用于接收要广播给此客户端的消息
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    // 将该客户端注册到聊天室
    {
        let mut map = clients.lock().await;
        map.insert(remote_addr, Client::new(tx));
        println!("User {} joined", remote_addr);
    }

    // 4. 启动“写任务”：从 rx 读取消息并写入 QUIC
    tokio::spawn(async move {
        let mut buf = Vec::with_capacity(1024);
        let addr = remote_addr;
        loop {
            match rx.recv().await {
                Some(msg) => {
                    println!("向{}发送消息 {:?}", addr, msg);
                    buf.clear();
                    buf.extend_from_slice(&msg.to_bytes().unwrap());
                    let len = buf.len() as u32;
                    let _ = send_stream.write_all(&len.to_be_bytes()).await;
                    if let Err(e) = send_stream.write_all(&buf).await {
                        eprintln!("Write error to {}: {}", remote_addr, e);
                        break;
                    }
                    match send_stream.flush().await {
                        Err(e) => {
                            println!("发送失败:{}", e);
                        }
                        _ => {}
                    }
                }
                None => break, // channel closed
            }
        }
    });

    // 5. 读取客户端发来的消息，并广播给所有人
    let mut read_buf = vec![0u8; 8192];
    let mut expire_addr = Vec::new();
    loop {
        match recv_stream.read(&mut read_buf).await? {
            Some(n) => {
                let mut msg: Message = from_bytes(&read_buf[..n]).unwrap();
                // 广播给所有客户端（包括自己）
                if msg.command == NetworkMessage::Ready {
                    let mut map = clients.lock().await;
                    let mut ready_num = 0;
                    match map.get_mut(&remote_addr) {
                        Some(client) => {
                            client.id = msg.id.clone();
                            client.state = 1;
                        }
                        None => {}
                    };
                    for (_, client) in map.iter() {
                        if client.state == 1 {
                            ready_num += 1;
                        }
                    }
                    if ready_num == 3 {
                        let mut play1_msg = Message {
                            id: "".to_owned(),
                            command: NetworkMessage::Deal,
                            data: "".to_owned(),
                            carddata: vec![],
                        };
                        let mut play2_msg = Message {
                            id: "".to_owned(),
                            command: NetworkMessage::Deal,
                            data: "".to_owned(),
                            carddata: vec![],
                        };

                        let mut play3_msg = Message {
                            id: "".to_owned(),
                            command: NetworkMessage::Deal,
                            data: "".to_owned(),
                            carddata: vec![],
                        };
                        let (player1_cards, player2_cards, player3_cards, underhand) = shuffle();
                        for (index, (_addr1, sender)) in map.iter().enumerate() {
                            if index == 0 {
                                play1_msg.id = sender.id.clone();
                                play1_msg.carddata = player1_cards
                                    .iter()
                                    .chain(underhand.iter())
                                    .copied()
                                    .collect();
                            } else if index == 1 {
                                play2_msg.id = sender.id.clone();
                                play2_msg.carddata = player2_cards.clone();
                            } else if index == 2 {
                                play3_msg.id = sender.id.clone();
                                play3_msg.carddata = player3_cards.clone();
                            }
                        }
                        for (_index, (_addr1, sender)) in map.iter().enumerate() {
                            let _ = sender.send.send(play1_msg.clone());
                            let _ = sender.send.send(play2_msg.clone());
                            let _ = sender.send.send(play3_msg.clone());
                        }

                        continue;
                    } else {
                        msg.command = NetworkMessage::Data;
                        msg.data = "已准备".to_owned();
                    }
                }
                let map = clients.lock().await;
                for (addr, sender) in map.iter() {
                    let res = sender.send.send(msg.clone()); // fire-and-forget
                    match res {
                        Err(e) => {
                            expire_addr.push(addr.clone());
                        }
                        _ => {}
                    }
                }
            }
            None => break,
        }

        if expire_addr.len() > 0 {
            let mut map = clients.lock().await;
            for item in expire_addr.drain(..) {
                map.remove(&item);
            }
        }
    }

    // 连接关闭，自动触发 leave（在 handle_incoming_messages 中处理）

    Ok(())
}
