use egui::Vec2;
use quinn_proto::crypto::rustls::QuicClientConfig;
use rustls::crypto::{CryptoProvider, ring};
use rustls::pki_types::CertificateDer;
use std::net::SocketAddr;
use std::sync::Arc;
use std::collections::HashMap;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
mod common;

use crate::common::net::{Message, NetworkMessage};
use eframe::egui;
use eframe::epaint::text::{FontInsert, InsertFontFamily};
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
    button_offsets:Vec<f32>,
    conn: Connection,
    endpoint: Endpoint,
    input_text: String,
    current_cards: Option<Message>,
    is_ready: bool,
    players: Vec<Player>,
    my_turn: usize,
    rounds:usize,
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
            button_offsets:Vec::new(),
            conn: conn,
            endpoint: edpt,
            input_text: "".to_owned(),
            current_cards: None,
            is_ready: false,
            players: Vec::new(),
            my_turn: 0,
            rounds:0,
        }
    }
    fn sendmsg(&self, message: Message) {
        self.send.send(message).unwrap();
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
                    match from_bytes(&data){
                        Ok(msg)=>{
                            // println!("--------- {:?}",msg);
                            match suf_tx.send(msg) {
                                Err(e)=>{
                                    eprintln!("发送ui消息失败：{}",e);
                                },
                                _=>{}
                            };
                        },
                        Err(e)=>{
                            eprintln!("解析消息失败：{}",e);
                        }
                    }
                    
                }
                _ => {
                    println!("读取消息失败");
                    break;
                },
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
                    if msg.id == self.id{
                        let len = msg.carddata.len();
                        self.button_offsets =vec![0.0;len];
                        self.my_cards = Some(msg.clone());
                        
                    }
                    let mut player = Player::new(msg.id.clone());
                    player.card_num = msg.carddata.clone().len();
                    player.is_lord = msg.carddata.len() == 20;
                    player.is_ready = true;
                    self.players.push(player);
                    if self.players.len() ==3 {
                        self.rounds = 0;
                        for (index,player) in self.players.iter_mut().enumerate() {
                            if player.id == self.id{ 
                                self.my_turn = index;
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
            ui.label(format!("当前ID:{}",self.id));
            // ui.add_sized([ui.available_width(), 200.0], |ui| {

            // });
            egui::Frame::group(ui.style()).show(ui, |ui| {

                if let Some(my_card) = self.my_cards.clone() {
                    // 渲染5个按钮
                    for (i,card) in my_card.carddata.iter().enumerate() {
                        // 为每个按钮添加垂直偏移
                        ui.add_space(5.0); // 按钮之间的间距
                        
                        // 创建一个包含按钮的垂直布局，用于应用偏移
                        ui.vertical(|ui| {
                            // 根据偏移量添加顶部空白
                            let offset = self.button_offsets[i];
                            if offset > 0.0 {
                                ui.add_space(offset);
                            }
                            
                            // 渲染按钮
                            let button = egui::Button::new(format!("按钮 {}", i + 1));
                            let response = ui.add_sized([80.0, 30.0], button);
                            
                            // 处理点击事件
                            if response.clicked() {
                                // 点击后向上移动10像素（增加偏移量）
                                self.button_offsets[i] += 10.0;
                                println!("按钮 {} 被点击，当前偏移: {}", i + 1, self.button_offsets[i]);
                                
                                // 请求重绘以显示移动效果
                                ctx.request_repaint();
                            }
                        });
                    }



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
                    if self.rounds%3 == self.my_turn {
                        if ui.button("出牌").clicked() {
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
                }
            });

            // ui.label(format!("Hello '{name}', age {age}"));
        });
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
