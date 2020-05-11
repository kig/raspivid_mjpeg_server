# raspivid_mjpeg_server

Low-latency video stream from Raspberry Pi to do remote OpenCV processing.

![Screen-to-screen latency](https://github.com/kig/raspivid_mjpeg_server/raw/master/images/latency.jpg)

Screen-to-screen latency with 50 FPS camera, 60Hz screen and Chrome as viewer is about 120ms over WiFi. Camera-to-OpenCV latency might be 30-60 ms lower (how would you measure?)

This is decent for e.g. driving an RC car from first-person view and some gestural interfaces.

If you want lower camera-to-screen latency, the easiest improvement is getting a 240Hz display and setting the camera FPS as high as it goes. After that, get a better WiFi antenna / base station. Try to get rid of double/triple buffering. Hack the camera-to-streamer-to-viewer system to send data chunks as soon as they arrive instead of waiting for end of frame. Make the rolling shutter send 8 scanline chunks. Hack your display to immediately display 8 scanline chunks instead of full framebuffers. Directly drive your display from an FPGA with a wireless receiver. Ditto with the camera. Remove the camera sensor and display hardware, add a few lenses, galvos and a ground glass screen to build an optical image relay system.

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

# Open http://raspberrypi:8554/video.mjpg
```

This is a quick replacement for VLC MJPEG streaming `| cvlc stream:///dev/stdin --sout '#standard{access=http{mime=multipart/x-mixed-replace;boundary=--7b3cc56e5f51db803f790dad720ed50a},mux=mpjpeg,dst=:8554/video.mjpg}` (and/or Python Flask streaming server).

CPU usage is a high at 10%. If you know why, let me know. I'd love to fix it.

Memory usage is low at 2.3 MB.

## License

MIT

2020 (c) Ilmari Heikkinen <hei@heichen.hk>
