// for sink and stream
use futures::{SinkExt, StreamExt};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{broadcast, broadcast::Sender, Mutex};
use tokio::{
    net::{TcpListener, TcpStream},
    select,
};
// better reading/wrting
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};

#[derive(Clone)]
struct Groups {
    groups: Arc<Mutex<HashMap<String, Sender<String>>>>
}

impl Groups{
    fn new() -> Self {
        let (tx, _) = broadcast::channel(32);
        let (tx2, _) = broadcast::channel(32);
        let mut hm: HashMap<String, Sender<String>> = HashMap::new();
        hm.insert("main".to_string(), tx);
        hm.insert("main2".to_string(), tx2);
        Self {
            groups: Arc::new(Mutex::new(hm)),
        }
    }
    async fn join(&self, group_name: &str) -> Option<Sender<String>>{
        // check if group exists
        let guard = self.groups.lock().await;
        if guard.contains_key(group_name) {
            return Some(guard[group_name].clone())
        }
        else {
            println!("Client joining room failed");
        }
        None
    }
}

#[derive(Clone)]
struct Names {
    names: Arc<Mutex<Vec<String>>>,
}

impl Names {
    fn new() -> Self {
        Self {
            names: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

async fn handle_client<'a>(mut tcp: TcpStream, groups: Groups, names: Names) -> anyhow::Result<()> {
    let (reader, writer) = tcp.split();
    let mut stream = FramedRead::new(reader, LinesCodec::new());
    let mut sink = FramedWrite::new(writer, LinesCodec::new());

    let mut name = String::new();
    let mut tx = groups.join("main").await.unwrap();

    sink.send("Pls enter your name").await?;
    // if Some(Ok) => client connected and sent message
    if let Some(Ok(message)) = stream.next().await {
        // lock so only 1 client can access
        let mut guard = names.names.lock().await;
        if guard.contains(&message) {
            sink.send("Name is already taken").await?;
            return Ok(())
        } else {
            name = message.clone();
            guard.push(message);
        }
    // User disconnected while server was waiting for name
    } else {
        println!("Connection closed while reading name");
        return Ok(())
    }

    let mut rx = tx.subscribe();
    loop {
        select! {
            user_msg = stream.next() => {
                if let Some(Ok(user_msg)) = user_msg {
                    if user_msg.starts_with("/name") {
                        let new_name = user_msg.split_whitespace().nth(1).unwrap().to_string();
                    
                        let mut guard = names.names.lock().await;

                        if !guard.contains(&new_name) {
                            if let Some(index) = guard.iter().position(|x| {*x == name}) {
                                guard[index] = new_name.clone();
                                name = new_name;
                            }
                        } else {
                            sink.send("Name is already taken").await?;
                        }
                    }
                    else if user_msg.starts_with("/join"){
                        let room_name = user_msg.split_whitespace().nth(1).unwrap().to_string();
                        
                        if let Some(new_tx) = groups.join(&room_name).await {
                            tx = new_tx;
                            rx = tx.subscribe();
                        } else {
                            sink.send(format!("Failed joining room")).await?;
                        }
                    } else {
                        let _ = tx.send(format!("{}: {}", name, user_msg));
                    }
                } else {
                    let mut guard = names.names.lock().await;
                    if let Some(index) = guard.iter().position(|x| {*x==name}) {
                        guard.remove(index);
                    }
                    return Ok(());
                }
            }
            peer_msg = rx.recv() => {
                match peer_msg {
                    Ok(peer_msg) => {
                        sink.send(peer_msg).await?;
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8000").await.unwrap();

    let names = Names::new();
    let groups = Groups::new();



    loop {
        let (tcp, _) = listener.accept().await?;

        tokio::spawn(handle_client(tcp, groups.clone(), names.clone()));
    }
}
