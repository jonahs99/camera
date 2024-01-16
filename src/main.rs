use std::error;
use std::net::UdpSocket;
use tokio::{
    sync::broadcast::{channel, Receiver, Sender},
    net::TcpListener,
};
use axum::{
    serve, Router,
    routing::get,
    response::IntoResponse,
    extract::{WebSocketUpgrade, ws::{WebSocket, Message}},
};
use tower_http::services::ServeDir;

type Result<T> = std::result::Result<T, Box<dyn error::Error>>;

const BASE_URL: &str = "http://192.168.0.10/";
const JPEG_START: &[u8] = &[0xff, 0xd8];
const JPEG_STOP: &[u8] = &[0xff, 0xd9];

#[tokio::main(flavor="current_thread")]
async fn main() {
    let (live_tx, _) = channel(16);

    std::thread::spawn({
        let live_tx = live_tx.clone();
        move || start_live_view(live_tx)
    });

    let liveview_handler = move |ws| liveview_handler(ws, live_tx.subscribe());

    let app = Router::new()
        .route("/liveview", get(liveview_handler))
        .fallback_service(ServeDir::new("www"));

    let listener = TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap();
    serve(listener, app)
        .await
        .unwrap();
}

fn take_photo() {
    send_command("switch_cammode", &[("mode", "shutter")]).unwrap();
    std::thread::sleep_ms(300);
    send_command("exec_shutter", &[("com", "1st2ndpush")]).unwrap();
    std::thread::sleep_ms(300);
    send_command("exec_shutter", &[("com", "2nd1strelease")]).unwrap();
    std::thread::sleep_ms(300);
}

fn get_list() {
    send_command("switch_cammode", &[("mode", "play")]).unwrap();
    let response = send_command("get_imglist", &[("DIR", "/DCIM/100OLYMP")]).unwrap();
    let list: Vec<String> = response
        .as_str().unwrap()
        .split("\n")
        .map(|entry| {
             let parts: Vec<_> = entry.split(",").take(2).collect();
             parts.join("/")
        })
        .filter(|name| name.starts_with("/"))
        .collect();
    dbg!(list);
}

fn start_live_view(tx: Sender<Vec<u8>>) {
    send_command("exec_takemisc", &[
        ("com", "stopliveview"),
    ]).unwrap();
    send_command("switch_cammode", &[
        ("mode", "rec"),
        ("lvqty", "0640x0480"),
    ]).unwrap();
    send_command("exec_takemisc", &[
        ("com", "startliveview"),
        ("port", "40000"),
    ]).unwrap();

    let socket = UdpSocket::bind("0.0.0.0:40000").unwrap();
    loop {
        let mut frame: Vec<u8> = Vec::new(); 
        let mut prev: Option<rtp_rs::Seq> = None;

        // Assemble packets into a frame
        loop {
            let mut buf = [0; 4096];
            let (len, _) = socket.recv_from(&mut buf).unwrap();
            let rtp = rtp_rs::RtpReader::new(&buf[..len]).unwrap();
            
            if let Some(prev) = prev {
                if !prev.precedes(rtp.sequence_number()) {
                    // Packet out of order, restart frame
                    break;
                }
            }
            prev = Some(rtp.sequence_number());
            frame.extend(rtp.payload());

            // If the packet is marked, this is the last packet for a frame
            if rtp.mark() {
                if &frame[..2] == JPEG_START &&
                    &frame[frame.len()-2..] == JPEG_STOP {
                    let _ = tx.send(frame);
                }
                break;
            }
        }
    }
}

fn send_command(cmd: &str, args: &[(&str, &str)]) -> Result<minreq::Response> {
    let mut url = format!("{}{}.cgi", BASE_URL, cmd);
    if args.len() > 0 {
        url.push_str("?");
        for (k, v) in args {
            url.push_str(k);
            url.push_str("=");
            url.push_str(v);
            url.push_str("&");
        }
        url.pop();
    }

    Ok(minreq::get(url).send()?)
}

async fn liveview_handler(
    ws: WebSocketUpgrade,
    rx: Receiver<Vec<u8>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, rx))
}

async fn handle_ws(mut socket: WebSocket, mut rx: Receiver<Vec<u8>>) {
    while let Ok(frame) = rx.recv().await {
        socket.send(Message::Binary(frame)).await.unwrap();
    }
}

