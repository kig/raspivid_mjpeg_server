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
    std::io::{Read, BufRead},
    tokio::stream::{StreamExt},
    tokio::sync::watch,
};

static HEAD: &[u8] = "\r\n--7b3cc56e5f51db803f790dad720ed50a\r\nContent-Type: image/jpeg\r\nContent-Length: ".as_bytes();
static RNRN: &[u8] = "\r\n\r\n".as_bytes();

async fn serve_req(_req: Request<Body>, rx: watch::Receiver<Vec<u8>>) -> Result<Response<Body>> {
    let result_stream = rx.map(|buffer| { 
        let res: Result<_> = Ok(buffer);
        res
    });
    let body = Body::wrap_stream(result_stream);
    Ok(Response::builder()
        .header("Content-Type", "multipart/x-mixed-replace; boundary=--7b3cc56e5f51db803f790dad720ed50a")
        .status(StatusCode::OK)
        .body(body)
        .unwrap())
}

async fn run_server(addr: SocketAddr, rx: watch::Receiver<Vec<u8>>) { // current_frame: Arc<Frame>) {
    println!("Listening on http://{}", addr);
    let serve_future = Server::bind(&addr)
        .serve(make_service_fn(|_| {
            let my_rx = rx.clone();
            async {
                {
                    Ok::<_, hyper::Error>(service_fn(move |_req| { 
                        serve_req(_req, my_rx.clone())
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

    // let current_frame = Arc::new(Frame{frame: RwLock::new(FrameBuffer{ buffer: Vec::with_capacity(65500), number: 0 }) });
    let (tx, rx) = watch::channel(Vec::new());
    // let server_frame = Arc::clone(&current_frame);

    let server = run_server(addr, rx);
    tokio::spawn(async move {
        server.await;
    });

    // JPEG starts with 0xff 0xd8 0xff
    // and ends with    0xff 0xd9

    let mut reader = std::io::BufReader::with_capacity(4096, std::io::stdin());
    let mut buffer = Vec::with_capacity(65500);
    let mut jpeg = Vec::with_capacity(65500);
    let mut len_buf = vec![0; 2];
    let mut data_buf = vec![0; 0];
    let mut byt = vec![0; 1];

    loop {
        jpeg.clear();
        let mut in_jpeg = false;

        while !in_jpeg {
            in_jpeg = match reader.read_until(0xff, &mut jpeg) {
                Ok(0) => { panic!("EOF") },
                Ok(_n) => jpeg.len() > 2 && jpeg[jpeg.len()-3] == 0xff && jpeg[jpeg.len()-2] == 0xd8,
                Err(error) => { panic!("error: {}", error) },
            };
            // println!("init {}", jpeg.len());
        }
        jpeg = jpeg[jpeg.len()-3..].to_vec();
        loop {
            reader.read_exact(&mut byt).unwrap();
            let b = byt[0];
            jpeg.push(b);

            if b == 0xd9 { // end of image
                break;
            } else if b == 0x00 || (b >= 0xd0 && b <= 0xd7) { // marker without length or byte stuffing
            } else { // marker with length
                reader.read_exact(&mut len_buf).unwrap();
                let len:usize = (len_buf[0] as usize * 256) + (len_buf[1] as usize) - 2;
                jpeg.extend_from_slice(&len_buf.as_slice());
                data_buf.resize(len, 0);
                reader.read_exact(&mut data_buf).unwrap();
                jpeg.extend_from_slice(&data_buf.as_slice());
            }
            
            reader.read_until(0xff, &mut jpeg).unwrap();
            // println!("jpeg {}", jpeg.len());
        }
        buffer.clear();
        buffer.extend_from_slice(&HEAD);
        buffer.extend_from_slice(&jpeg.len().to_string().as_bytes());
        buffer.extend_from_slice(&RNRN);
        buffer.extend_from_slice(&jpeg.as_slice());
        match tx.broadcast(buffer.clone()) {
            _ => ()
        }
        // println!("frame {}: {} bytes", tx_guard.number, tx_guard.buffer.len());
    }
}
