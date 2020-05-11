# raspivid_mjpeg_server

## Usage

```sh
 # Install Rust if needed
 curl https://sh.rustup.rs -sSf | sh
. ~/cargo/env 

# Clone the repo and start the server
git clone https://github.com/kig/raspivid_mjpeg_server
cd raspivid_mjpeg_server
raspivid -ISO 0 -t 0 -n -o - -w 1280 -h 720 -fps 25 -b 25000000 -cd MJPEG | cargo run --release

# Wait forever (cargo build --release takes 12 minutes on RPi 3B)

# Open http://pi:8554/video.mjpg
```

This is a quick replacement for VLC MJPEG streaming `| cvlc stream:///dev/stdin --sout '#standard{access=http{mime=multipart/x-mixed-replace;boundary=--7b3cc56e5f51db803f790dad720ed50a},mux=mpjpeg,dst=:8554/video.mjpg}` (and/or Python Flask streaming server).

CPU usage is a high at 10%. If you know why, let me know. I'd love to fix it.

Memory usage is low at 2.3 MB.

## License

MIT

2020 (c) Ilmari Heikkinen <hei@heichen.hk>
