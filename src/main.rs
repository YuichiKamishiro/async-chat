use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::{broadcast, broadcast::Sender, Mutex};
use tokio::{
    net::{TcpListener, TcpStream},
    select,
};
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};

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

async fn handle_client(mut tcp: TcpStream, tx: Sender<String>, names: Arc<Mutex<Vec<String>>>) -> anyhow::Result<()> {
    let (reader, writer) = tcp.split();
    let mut stream = FramedRead::new(reader, LinesCodec::new());
    let mut sink = FramedWrite::new(writer, LinesCodec::new());

    let mut name = String::new();

    sink.send("Pls enter your name").await?;
    if let Some(Ok(message)) = stream.next().await {
        let mut guard = names.lock().await;
        if guard.contains(&message) {
            sink.send("Name is already taken").await?;
            return Ok(())
        } else {
            name = message.clone();
            guard.push(message);
        }
    } else {
        println!("Connection closed while reading name");
        return Ok(())
    }

    let mut rx = tx.subscribe();
    loop {
        select! {
            user_msg = stream.next() => {
                if let Some(Ok(user_msg)) = user_msg {
                    let _ = tx.send(format!("{}: {}", name, user_msg));
                } else {
                    println!("User disconnected\n");
                    break;
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

    let (tx, _) = broadcast::channel::<String>(32);
    loop {
        let (tcp, _) = listener.accept().await?;

        tokio::spawn(handle_client(tcp, tx.clone(), names.names.clone()));
    }
}
