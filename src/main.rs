use {
    hyper::{
        service::{make_service_fn, service_fn},
        Body,
        StatusCode,
        Request,
        Response,
        Result,
        Server,
    },
    std::net::SocketAddr,
    tokio::io::{BufReader, AsyncReadExt, AsyncBufReadExt},
    std::sync::Arc,
    tokio::sync::{RwLock},
    tokio::stream::{Stream},
};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::future::Future;
use std::time::Duration;

struct FrameBuffer {
    buffer: Vec<u8>,
    number: i32,
}

struct Frame {
    frame: RwLock<FrameBuffer>,
}

pub struct FrameStream {
    current_frame: Arc<Frame>,
    last_frame_number: i32,
}

impl Stream for FrameStream {
    type Item = Result<Vec<u8>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Vec<u8>>>> {
        let mut last_frame_number = self.last_frame_number;
        let result = {
            let mut pin_box = std::boxed::Box::pin(self.current_frame.frame.read());
            match pin_box.as_mut().poll(cx) {
                Poll::Ready(frame) => {
                    if frame.number < 0 {
                        Poll::Ready(None)
                    } else if last_frame_number != frame.number {
                        // println!("last_frame_number: {} {}, {:?}", last_frame_number, frame.number, frame.buffer.len());
                        last_frame_number = frame.number;
                        let mut res = Vec::new();
                        let header = format!("\r\n--7b3cc56e5f51db803f790dad720ed50a\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n", frame.buffer.len());
                        let mut header_vec = header.as_bytes().to_vec();
                        res.append(&mut header_vec);
                        res.append(&mut frame.buffer.clone());
                        Poll::Ready(Some(Ok(res)))
                    } else {
                        Poll::Pending
                    }
                }
                Poll::Pending => Poll::Pending
            }
        };
        self.last_frame_number = last_frame_number;
        match result {
            Poll::Pending => {
                std::thread::sleep(Duration::from_millis(10));
                cx.waker().wake_by_ref()
            },
            _ => ()
        };
        result
    }
}

async fn serve_req(_req: Request<Body>, current_frame: Arc<Frame>) -> Result<Response<Body>> {
    let frame_stream = FrameStream { current_frame: current_frame, last_frame_number: 0 };
    let body = Body::wrap_stream(frame_stream);
    Ok(Response::builder()
        .header("Content-Type", "multipart/x-mixed-replace; boundary=--7b3cc56e5f51db803f790dad720ed50a")
        .status(StatusCode::OK)
        .body(body)
        .unwrap())
}

async fn run_server(addr: SocketAddr, current_frame: Arc<Frame>) {
    println!("Listening on http://{}", addr);
    let serve_future = Server::bind(&addr)
        .serve(make_service_fn(|_| {
            let buf = Arc::clone(&current_frame);
            async {
                {
                    Ok::<_, hyper::Error>(service_fn(move |_req| { 
                        serve_req(_req, Arc::clone(&buf)) 
                    }))
                }
            }
        }));

    if let Err(e) = serve_future.await {
        eprintln!("server error: {}", e);
    }
}

#[tokio::main]
async fn main() {
    let addr = SocketAddr::from(([0, 0, 0, 0], 8554));

    let mut reader = BufReader::new(tokio::io::stdin());
    let current_frame = Arc::new(Frame{frame: RwLock::new(FrameBuffer{ buffer: Vec::new(), number: 0 }) });

    let server = run_server(addr, Arc::clone(&current_frame));
    tokio::spawn(async move {
        server.await;
    });

    // JPEG starts with 0xff 0xd8 0xff
    // and ends with    0xff 0xd9

    loop {
        let mut jpeg = Vec::new();
        let mut in_jpeg = false;

        while !in_jpeg {
            in_jpeg = match reader.read_until(0xff, &mut jpeg).await {
                Ok(0) => { panic!("EOF") },
                Ok(_n) => jpeg.len() > 2 && jpeg[jpeg.len()-3] == 0xff && jpeg[jpeg.len()-2] == 0xd8,
                Err(error) => { panic!("error: {}", error) },
            };
            // println!("init {}", jpeg.len());
        }
        jpeg = jpeg[jpeg.len()-3..].to_vec();
        loop {
            let mut byt = vec![0; 1];
            reader.read_exact(&mut byt).await.unwrap();
            let b = byt[0];
            jpeg.append(&mut byt);

            if b == 0xd9 { // end of image
                break;
            } else if b == 0x00 || (b >= 0xd0 && b <= 0xd7) { // marker without length or byte stuffing
            } else { // marker with length
                let mut len_buf = vec![0; 2];
                reader.read_exact(&mut len_buf).await.unwrap();
                let len:usize = (len_buf[0] as usize * 256) + (len_buf[1] as usize) - 2;
                jpeg.append(&mut len_buf);
                let mut data_buf = vec![0; len];
                reader.read_exact(&mut data_buf).await.unwrap();
                jpeg.append(&mut data_buf);
            }
            
            reader.read_until(0xff, &mut jpeg).await.unwrap();
            // println!("jpeg {}", jpeg.len());
        }
        let mut tx_guard = current_frame.frame.write().await;
        tx_guard.buffer.clear();
        tx_guard.buffer.append(&mut jpeg);
        tx_guard.number += 1;
        // println!("frame {}: {} bytes", tx_guard.number, tx_guard.buffer.len());
    }
}
