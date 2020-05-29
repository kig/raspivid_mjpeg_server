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
    std::time::{Duration, Instant},
    std::fs,
    structopt::StructOpt,
    tokio::stream::{StreamExt},
    tokio::sync::watch,
};

#[derive(StructOpt)]
struct Opt {
    /// Delay in microseconds between frames read from files.
    #[structopt(short, long, default_value = "16000")]
    delay: u64,

    /// Read frame filenames from the given file and loop over them. Use `-` to read from STDIN. Set the frame rate with --delay.
    #[structopt(short, long = "file")]
    filename: Option<String>,

    /// Listen for HTTP connections on this port.
    #[structopt(short, long, default_value = "8554")]
    port: u16,
}

static HEAD: &[u8] = "\r\n--7b3cc56e5f51db803f790dad720ed50a\r\nContent-Type: image/jpeg\r\nContent-Length: ".as_bytes();
static RNRN: &[u8] = "\r\n\r\n".as_bytes();

async fn serve_req(_req: Request<Body>, rx: watch::Receiver<Vec<u8>>) -> Result<Response<Body>> {
    // Convert the watch stream of Vec<u8>s into a Result stream for Body::wrap_stream.
    let result_stream = rx.map(|buffer| Result::Ok(buffer) );
    let body = Body::wrap_stream(result_stream);
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "multipart/x-mixed-replace; boundary=--7b3cc56e5f51db803f790dad720ed50a") // MJPEG stream.
        .body(body) // Send out the body stream.
        .unwrap())
}

async fn run_server(addr: SocketAddr, rx: watch::Receiver<Vec<u8>>) {
    println!("Listening on http://{}", addr);
    // Bind the Hyper HTTP server to addr and start serving requests.
    let serve_future = Server::bind(&addr)
        .serve(make_service_fn(|_| {
            // This function is invoked on every request.
            // We need to clone rx to avoid moving it to this request.
            let my_rx = rx.clone();
            async {
                // We need to clone my_rx because of the async block.
                Ok::<_, hyper::Error>(service_fn(move |_req| serve_req(_req, my_rx.clone() )))
            }
        }));

    if let Err(e) = serve_future.await {
        eprintln!("Server error: {}", e);
    }
}

fn send_jpeg(tx: &watch::Sender<Vec<u8>>, output_buffer: &mut Vec<u8>, jpeg: &Vec<u8>) {
    output_buffer.clear();
    // Write the MJPEG header to the output_buffer, followed by the JPEG data.
    output_buffer.extend_from_slice(&HEAD);
    output_buffer.extend_from_slice(&jpeg.len().to_string().as_bytes());
    output_buffer.extend_from_slice(&RNRN);
    output_buffer.extend_from_slice(&jpeg.as_slice());
    
    // Send the output_buffer to all the open client responses.
    match tx.broadcast(output_buffer.clone()) {
        _ => ()
    }
}

fn file_send_loop(filenames: Vec<String>, tx: watch::Sender<Vec<u8>>, delay: Duration) {
    let mut output_buffer = Vec::with_capacity(65500); // Output buffer, contains MJPEG headers and JPEG data.
    let mut frame = 0;
    let now = Instant::now();
    loop {
        // Loop over the files, send them out and sleep.
        for filename in &filenames {
            let jpeg = fs::read(filename).unwrap();

            // println!("{:?}", now.elapsed());
            send_jpeg(&tx, &mut output_buffer, &jpeg);

            frame += 1;

            // Try to maintain a stable frame rate. E.g. if the delay is 1 ms, the first frame should trigger at 0 ms, next at 1 ms, 2 ms, 3 ms, ..., 997 ms, 998 ms, 999 ms, etc.
            let elapsed = now.elapsed();
            std::thread::sleep((delay * frame).checked_sub(elapsed).unwrap_or(Duration::new(0, 0)));
        }
    }
}

fn stdin_send_loop(tx: watch::Sender<Vec<u8>>) {
    let mut reader = std::io::BufReader::with_capacity(4096, std::io::stdin()); // Buffered reader for stdin.
    let mut output_buffer = Vec::with_capacity(65500); // Output buffer, contains MJPEG headers and JPEG data.
    let mut jpeg = Vec::with_capacity(65500); // Read buffer, contains JPEG data read from stdin.

    // Utility buffers for reading JPEG data.
    let mut len_buf = vec![0; 2];
    let mut data_buf = vec![0; 0];
    let mut byt = vec![0; 1];

    // Read JPEGs from stdin and broadcast them to connected clients.
    loop {
        jpeg.clear();
        let mut in_jpeg = false;

        // Does this block the tokio event loop in a bad way? That is, does this prevent clients from receiving data?
        // The async IO version is more CPU-heavy, which is why this is using sync IO.

        while !in_jpeg {
            // Read until the next potential image start marker. This strips out the MJPEG headers in raspivid output.
            in_jpeg = match reader.read_until(0xFF, &mut jpeg) {
                Ok(0) => { panic!("EOF") },
                // JPEG starts with 0xFF 0xD8 0xFF.
                Ok(_n) => jpeg.len() > 2 && jpeg[jpeg.len()-3] == 0xFF && jpeg[jpeg.len()-2] == 0xD8,
                Err(error) => { panic!("IO error: {}", error) },
            };
        }
        // Keep the last three bytes of jpeg, making jpeg == 0xFF 0xD8 0xFF.
        jpeg = jpeg[jpeg.len()-3..].to_vec();

        // Read the rest of the JPEG image data, block by block.
        let mut valid_jpeg = true;
        let mut inside_scan = false;
        loop {
            // Get the marker byte.
            reader.read_exact(&mut byt).unwrap();
            let b = byt[0];
            jpeg.push(b);

            if b == 0xD9 { // End of image marker.
                break;
            } else if b == 0x00 || (b >= 0xD0 && b <= 0xD7) { // Escaped 0xFF or scan reset marker.
                if !inside_scan {
                    println!("0xFF escape or scan reset outside scan data {}", b);
                    valid_jpeg = false;
                    break;
                }
                // Find the next marker.
                reader.read_until(0xFF, &mut jpeg).unwrap();
            } else if b >= 0xC0 && b <= 0xFE { // Marker with length. Read the length and the content.
                inside_scan = b == 0xDA; // Start of Scan.
                reader.read_exact(&mut len_buf).unwrap();
                let len:usize = (len_buf[0] as usize * 256) + (len_buf[1] as usize) - 2;
                jpeg.extend_from_slice(&len_buf.as_slice());
                data_buf.resize(len+1, 0);
                reader.read_exact(&mut data_buf).unwrap();
                jpeg.extend_from_slice(&data_buf.as_slice());
                let end = data_buf[len];
                if end != 0xFF { // Markers must be followed by markers.
                    if inside_scan { // Unless we are inside compressed image data.
                        reader.read_until(0xFF, &mut jpeg).unwrap();
                    } else {
                        println!("Marker not followed by marker {}", end);
                        valid_jpeg = false;
                        break;
                    }
                }
            } else { // Invalid marker.
                println!("Invalid marker {}", b);
                valid_jpeg = false;
                break;
            }
        }

        // Send valid JPEGs to clients.
        if valid_jpeg {
            send_jpeg(&tx, &mut output_buffer, &jpeg);
        }
    }
}

#[tokio::main]
async fn main() {
    let opt = Opt::from_args();

    // Listening IP address and port.
    let addr = SocketAddr::from(([0, 0, 0, 0], opt.port));

    // Single-sender, multiple-receiver tokio::watch channel for sending JPEGs read from stdin to HTTP response streams.
    let (tx, rx) = watch::channel(Vec::new());

    // Create the Hyper HTTP server and give it the receiving end of the watch channel.
    let server = run_server(addr, rx);
    tokio::spawn(async move {
        server.await; // Start the server.
    });

    match opt.filename {
        Some(filename) => {
            let filenames: Vec<_> = if filename == "-" { // If the filename is `-`, read the filenames from STDIN.
                std::io::BufReader::with_capacity(4096, std::io::stdin()).lines().map(|l| l.unwrap()).collect()
            } else { // Otherwise, read the filenames from the file.
                fs::read(filename).unwrap().lines().map(|l| l.unwrap()).collect()
            };
            file_send_loop(filenames, tx, Duration::from_micros(opt.delay))
        },
        None => stdin_send_loop(tx),
    }

}
