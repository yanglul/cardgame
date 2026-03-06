 

use anyhow::Result;
use crate::common::card::Card;
use postcard::{from_bytes, to_allocvec};
use serde::{Deserialize, Serialize};
#[derive(Deserialize, Serialize, Debug, PartialEq, Clone)]
pub struct Message {
    pub id: String,
    pub command: NetworkMessage,
    pub data: String,
    pub carddata: Vec<Card>,
}

impl Message {
    // 序列化消息
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok(to_allocvec(&self)?)
    }
}

// 反序列化消息
pub fn from_bytes_to_Msg(bytes: &[u8]) -> Result<Message> {
    Ok(from_bytes(bytes)?)
}
// 消息类型定义
#[derive(Debug, Clone,Deserialize, Serialize,PartialEq)]
pub enum NetworkMessage {
    Connect,   // 连接服务器
    Ready,// 准备
    Deal,//发牌
    Play,//出牌
    Skip,//过
    Data,//数据
    Disconnect,        // 断开连接
    Error(String),     // 错误信息
}


use std::net::SocketAddr;
use std::sync::Arc;
use bytes::Bytes;
use quinn::{Endpoint,  VarInt};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

use rustls::pki_types::CertificateDer;
use quinn_proto::crypto::rustls::QuicClientConfig;
// 同步客户端包装
pub struct QuinnSyncClient {
    pub connection: Option<quinn::Connection>,
}
use crate::common::ALPN_QUIC_HTTP;
impl QuinnSyncClient {
    
    
    pub async  fn async_connect(
        &self,
        server_addr: &str,
    ) -> Result<quinn::Connection> {
        let addr: SocketAddr = server_addr.parse()?;
        let server_name = "localhost".try_into()?;
        
        // 创建 TLS 配置（测试环境跳过验证）
       
    let mut roots = rustls::RootCertStore::empty();
    let cert_path = std::path::Path::new("cert.der");
        match std::fs::read(cert_path) {
            Ok(cert) => {
                roots.add(CertificateDer::from(cert))?;
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

    client_crypto.alpn_protocols = ALPN_QUIC_HTTP.iter().map(|&x| x.into()).collect();
    

    let client_config =
        quinn::ClientConfig::new(Arc::new(QuicClientConfig::try_from(client_crypto)?));
  
        
        // 创建端点
        let mut endpoint = Endpoint::client("[::]:0".parse()?)?;
        endpoint.set_default_client_config(client_config);
        
        // 连接服务器
        let connecting = endpoint.connect(addr, server_name)?.await?;
        Ok(connecting)
    }
 
    /// 关闭连接
    pub fn close(&mut self) {
        if let Some(conn) = self.connection.take() {
            conn.close(VarInt::from_u32(0), b"goodbye");
        }
    }
}




 #[cfg(test)]
mod net{
    use crate::common::net::*;
    #[test]
    fn test_card(){
        let start = Message {
            id: "2".to_owned(),
            command: NetworkMessage::Deal,
            data: "".to_owned(),
            carddata: vec![],
        };
        let sd = start.to_bytes().unwrap();
        println!("sd {:?}",sd);
        let json_str = serde_json::to_string(&sd).unwrap(); 
        println!("json_str {:?}",json_str);
        let end :Message= from_bytes(&sd).unwrap();
        println!("end {:?}",end);

    }

}