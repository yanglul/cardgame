use egui::Vec2;
use quinn_proto::crypto::rustls::QuicClientConfig;
use rustls::crypto::{CryptoProvider, ring};
use rustls::pki_types::CertificateDer;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
mod common;

use crate::common::card::{Card, check, compare};
use crate::common::net::{Message, NetworkMessage};
use eframe::egui;
use eframe::epaint::text::{FontInsert, InsertFontFamily};
use egui_toast::{Toast, ToastKind, ToastOptions, Toasts};
use postcard::from_bytes;
use quinn::{Connection, Endpoint};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
struct Player {
    pub id: String,
    pub is_ready: bool,
    pub card_num: usize,
    pub is_lord: bool,
}
impl Player {
    pub fn new(id: String) -> Self {
        Player {
            id: id,
            is_ready: false,
            card_num: 64,
            is_lord: false,
        }
    }
    pub fn set_cnum(&mut self, num: usize) {
        self.card_num = self.card_num - num;
    }
}

struct MyApp {
    id: String,
    send: UnboundedSender<Message>,
    receive: UnboundedReceiver<Message>,
    messages: Vec<Message>,
    my_cards: Option<Message>,
    button_offsets: Vec<bool>,
    conn: Connection,
    endpoint: Endpoint,
    input_text: String,
    current_cards: Option<Message>,
    is_ready: bool,
    players: Vec<Player>,
    my_turn: usize,
    rounds: usize,
    toasts: Toasts,
}

impl MyApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        id: String,
        usm: UnboundedSender<Message>,
        urm: UnboundedReceiver<Message>,
        conn: Connection,
        edpt: Endpoint,
    ) -> Self {
        add_font(&cc.egui_ctx);
        Self {
            id: id,
            send: usm,
            receive: urm,
            messages: vec![],
            my_cards: None,
            button_offsets: Vec::new(),
            conn: conn,
            endpoint: edpt,
            input_text: "".to_owned(),
            current_cards: None,
            is_ready: false,
            players: vec![],
            my_turn: 0,
            rounds: 0,
            toasts: Toasts::new()
                .anchor(egui::Align2::RIGHT_TOP, (10.0, 10.0)) // 位置
                .direction(egui::Direction::TopDown),
        }
    }
    fn sendmsg(&self, message: Message) {
        self.send.send(message).unwrap();
    }

    fn get_play_card(&self) -> Vec<Card> {
        let mut cards = vec![];
        for (index, flag) in self.button_offsets.iter().enumerate() {
            if *flag {
                let card = self.my_cards.as_ref().unwrap().carddata[index];
                cards.push(card.clone());
            }
        }
        cards
    }
    fn clear_play(&mut self) {
        // 方法2a：从后向前遍历删除（避免索引变化问题）
        let mut i = self.button_offsets.len();
        while i > 0 {
            i -= 1;
            if self.button_offsets[i] {
                self.button_offsets.remove(i);
                self.my_cards.as_mut().unwrap().carddata.remove(i);
            }
        }
    }
    fn reset_play(&mut self) {
        for flag in self.button_offsets.iter_mut() {
            *flag = false;
        }
    }

    fn close(&self) {
        self.send
            .send(Message {
                id: self.id.clone(),
                command: NetworkMessage::Disconnect,
                data: "".to_owned(),
                carddata: vec![],
            })
            .unwrap();
        self.conn.close(0u32.into(), b"done");
        // Give the server a fair chance to receive the close packet
        // block_on(self.endpoint.wait_idle())?;
        // Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    CryptoProvider::install_default(ring::default_provider())
        .expect("Failed to install default CryptoProvider");
    println!("Connecting to server...");
    let args: Vec<String> = std::env::args().collect();
    let client_id = args[1].clone();
    println!("client id: {}", client_id);

    let server_addr: SocketAddr = "127.0.0.1:4433".parse()?;

    let mut roots = rustls::RootCertStore::empty();
    let cert_path = std::path::Path::new("cert.der");
    match std::fs::read(cert_path) {
        Ok(cert) => {
            roots.add(CertificateDer::from(cert)).unwrap();
        }
        Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("local server certificate not found");
        }
        Err(e) => {
            eprintln!("failed to open local server certificate: {}", e);
        }
    }
    let mut client_crypto = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();

    client_crypto.alpn_protocols = common::ALPN_QUIC_HTTP.iter().map(|&x| x.into()).collect();

    let client_config =
        quinn::ClientConfig::new(Arc::new(QuicClientConfig::try_from(client_crypto).unwrap()));
    let mut endpoint = quinn::Endpoint::client("0.0.0.0:0".parse()?).unwrap();
    endpoint.set_default_client_config(client_config);

    println!("Connecting to chat server...");
    let connection = endpoint.connect(server_addr, "localhost")?.await?;
    println!("✅ Connected! Type your messages below:");

    // 打开一个双向流用于聊天
    let (mut send_stream, mut recv_stream) = connection.open_bi().await?;

    // 该通道是从本地到服务器
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    //该通道是服务器-> 本地
    let (suf_tx, suf_rx) = mpsc::unbounded_channel::<Message>();

    let id = client_id.clone();
    // 任务1：读取 stdin 并发送
    tokio::spawn(async move {
        // let stdin = tokio::io::stdin();
        // let mut reader = BufReader::new(stdin);
        // loop {
        //     let mut line = String::new();
        //     if reader.read_line(&mut line).await.unwrap() == 0 {
        //         break;
        //     }
        //     let msg = line.trim().to_string();
        //     if !msg.is_empty() {
        //         let _ = tx.send(msg);
        //     }
        // }
        // 任务3：从通道取消息并通过 QUIC 发送
        let start = Message {
            id: id,
            command: NetworkMessage::Connect,
            data: "已连接".to_owned(),
            carddata: vec![],
        };
        let _ = send_stream.write_all(&start.to_bytes().unwrap()).await;
        while let Some(msg) = rx.recv().await {
            if let Err(e) = send_stream.write_all(&msg.to_bytes().unwrap()).await {
                eprintln!("Send error: {}", e);
                break;
            }
            let _ = send_stream.flush().await;
        }
    });

    // 任务2：接收服务端消息并打印
    tokio::spawn(async move {
        let mut len_buf = [0u8; 4];
        loop {
            match recv_stream.read_exact(&mut len_buf).await {
                Ok(()) => {
                    let len = u32::from_be_bytes(len_buf) as usize;
                    let mut data = vec![0u8; len];
                    recv_stream.read_exact(&mut data).await;
                    match from_bytes(&data) {
                        Ok(msg) => {
                            // println!("--------- {:?}",msg);
                            match suf_tx.send(msg) {
                                Err(e) => {
                                    eprintln!("发送ui消息失败：{}", e);
                                }
                                _ => {}
                            };
                        }
                        Err(e) => {
                            eprintln!("解析消息失败：{}", e);
                        }
                    }
                }
                _ => {
                    println!("读取消息失败");
                    break;
                }
            }
        }
        println!("⚠️ Server closed connection.");
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([750.0, 410.0]),
        ..Default::default()
    };

    let id = client_id.clone();
    let _ = eframe::run_native(
        "egui example: custom font",
        options,
        Box::new(|cc| {
            Ok(Box::new(MyApp::new(
                cc, id, tx, suf_rx, connection, endpoint,
            )))
        }),
    );

    Ok(())
}

fn add_font(ctx: &egui::Context) {
    ctx.add_font(FontInsert::new(
        "my_font",
        egui::FontData::from_static(include_bytes!("C:\\Windows\\Fonts\\STSONG.TTF")),
        vec![
            InsertFontFamily {
                family: egui::FontFamily::Proportional,
                priority: egui::epaint::text::FontPriority::Highest,
            },
            InsertFontFamily {
                family: egui::FontFamily::Monospace,
                priority: egui::epaint::text::FontPriority::Lowest,
            },
        ],
    ));
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &eframe::egui::Context, frame: &mut eframe::Frame) {
        if let Ok(msg) = self.receive.try_recv() {
            ctx.request_repaint();
            // println!("消息: {:?}",msg);

            match msg.command {
                NetworkMessage::Deal => {
                    if msg.id == self.id {
                        let len = msg.carddata.len();
                        self.button_offsets = vec![false; len];
                        self.my_cards = Some(msg.clone());
                    }
                    let mut player = Player::new(msg.id.clone());
                    player.card_num = msg.carddata.clone().len();
                    player.is_lord = msg.carddata.len() == 20;
                    player.is_ready = true;
                    self.players.push(player);
                    if self.players.len() == 3 {
                        self.rounds = 0;
                        for (index, player) in self.players.iter_mut().enumerate() {
                            if player.id == self.id {
                                self.my_turn = index;
                            }
                        }
                    }
                }
                NetworkMessage::Play => {
                    self.rounds = self.rounds + 1;
                    self.current_cards = Some(msg.clone());
                    let id = msg.id.clone();
                    for player in self.players.iter_mut() {
                        if player.id == id {
                            player.card_num = player.card_num - msg.carddata.len();
                            if player.card_num == 0 {
                                self.toasts.add(Toast {
                                    text: format!("恭喜{}获得胜利", id).as_str().into(),
                                    kind: ToastKind::Success,
                                    options: ToastOptions::default(),
                                    ..Default::default()
                                });
                            }
                        }
                    }
                }
                NetworkMessage::Skip => {
                    self.rounds = self.rounds + 1;
                    if self.rounds % 3 == self.my_turn {
                        if self.current_cards.is_some() {
                            let cc = self.current_cards.clone().unwrap();
                            if cc.id == self.id {
                                self.current_cards = None;
                            }
                        }
                    }
                }

                _ => {
                    self.messages.push(msg.clone());
                }
            }
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label(format!("当前ID:{}", self.id));
            // ui.add_sized([ui.available_width(), 200.0], |ui| {

            // });
            egui::Frame::group(ui.style()).show(ui, |ui| {
                if let Some(my_card) = self.my_cards.clone() {
                    ui.horizontal(|ui| {
                        if self.players.len() >= 3 {
                            ui.label(format!(
                                "玩家:{} 剩余牌数量:{}",
                                self.players[(self.my_turn + 2) % 3].id,
                                self.players[(self.my_turn + 2) % 3].card_num
                            ));
                            ui.separator();
                            egui::Frame::group(ui.style()).show(ui, |ui| {
                                ui.label("当前出牌:");
                                if let Some(ccards) = self.current_cards.clone() {
                                    for card in ccards.carddata.iter() {
                                        ui.label(format!("{}", card.display_name()));
                                    }
                                }
                            });
                            ui.separator();
                            ui.label(format!(
                                "玩家:{} 剩余牌数量:{}",
                                self.players[(self.my_turn + 1) % 3].id,
                                self.players[(self.my_turn + 1) % 3].card_num
                            ));
                        }
                    });

                    ui.horizontal(|ui| {
                        for (i, card) in my_card.carddata.iter().enumerate() {
                            // 根据偏移量添加顶部空白
                            let button_color = if self.button_offsets[i] {
                                egui::Color32::from_rgb(100, 200, 100) // 点击后的绿色
                            } else {
                                egui::Color32::from_rgb(200, 100, 100) // 默认红色
                            };

                            // 渲染按钮
                            let button = egui::Button::new(format!("{}", card.display_name()))
                                .fill(button_color);
                            let response = ui.add_sized([30.0, 80.0], button);

                            // 处理点击事件
                            if response.clicked() {
                                self.button_offsets[i] = !self.button_offsets[i];
                            }
                        }
                    });
                }
            });

            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.input_text));
                    if ui.button("Send").clicked() {
                        let msg = Message {
                            id: self.id.clone(),
                            command: NetworkMessage::Data,
                            data: self.input_text.clone(),
                            carddata: vec![],
                        };
                        self.sendmsg(msg);
                    }
                });

                if !self.is_ready {
                    if ui.button("准备").clicked() {
                        let msg = Message {
                            id: self.id.clone(),
                            command: NetworkMessage::Ready,
                            data: "".to_owned(),
                            carddata: vec![],
                        };
                        self.sendmsg(msg);
                        self.is_ready = true;
                    }
                }
                if self.is_ready {
                    if (self.rounds == 0
                        && !self.players.is_empty()
                        && self.players[self.my_turn].is_lord)
                        || self.rounds % 3 == self.my_turn
                    {
                        if ui.button("出牌").clicked() {
                            let play_card = self.get_play_card();
                            let poke = check(play_card.clone());
                            if poke.is_none() {
                                self.toasts.add(Toast {
                                    text: "你出你妈呢".into(),
                                    kind: ToastKind::Error,
                                    options: ToastOptions::default(),
                                    ..Default::default()
                                });
                                self.reset_play();
                            } else {
                                let playground = self.current_cards.clone();
                                if playground.is_some() {
                                    let pg = playground.unwrap();
                                    if compare(play_card.clone(), pg.carddata) {
                                        let msg = Message {
                                            id: self.id.clone(),
                                            command: NetworkMessage::Play,
                                            data: "".to_owned(),
                                            carddata: play_card.clone(),
                                        };
                                        self.sendmsg(msg);
                                        self.clear_play();
                                    }else{
                                        self.reset_play();
                                        self.toasts.add(Toast {
                                            text: "牌小了".into(),
                                            kind: ToastKind::Error,
                                            options: ToastOptions::default(),
                                            ..Default::default()
                                        });
                                    }
                                } else {
                                    let msg = Message {
                                            id: self.id.clone(),
                                            command: NetworkMessage::Play,
                                            data: "".to_owned(),
                                            carddata: play_card.clone(),
                                        };
                                        self.sendmsg(msg);
                                        self.clear_play();



                                }
                            }
                        }
                        if ui.button("过").clicked() {
                            let msg = Message {
                                id: self.id.clone(),
                                command: NetworkMessage::Skip,
                                data: "".to_owned(),
                                carddata: vec![],
                            };
                            self.sendmsg(msg);
                        }
                    }
                }
            });

            // ui.label(format!("Hello '{name}', age {age}"));
        });
        self.toasts.show(ctx);
    }

    // 👇 关键：窗口关闭时调用
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // 在这里执行清理工作
        println!("🧹 应用正在关闭，执行清理...");
        // let msg = Message {
        //                         id: self.id,
        //                         command: NetworkMessage::Disconnect,
        //                         data: "已退出".to_owned(),
        //                         carddata: vec![],
        //                         timestamp: 0,
        //                     };
        // self.sendmsg(msg);
        self.close();
    }
}
