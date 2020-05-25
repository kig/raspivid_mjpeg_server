# raspivid_mjpeg_server

Low-latency video stream from Raspberry Pi to do remote OpenCV processing.

![Screen-to-screen latency](https://github.com/kig/raspivid_mjpeg_server/raw/master/images/latency.jpg)

Screen-to-screen latency with 50 FPS camera, 60Hz screen and Chrome as viewer is about 120ms over WiFi. Camera-to-OpenCV latency might be 30-60 ms lower (how would you measure?)

With 240Hz screen on Win10 Chrome and Raspberry Pi 3 v1 camera set to 120 FPS, the screen-to-screen latency is around 60 ms.

This is decent for e.g. driving an RC car from first-person view and some gestural interfaces.

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

# For higher FPS on v1 camera
raspivid -ISO 0 -t 0 -n -o - -w 640 -h 480 -fps 90 -b 25000000 -cd MJPEG | cargo run --release

# For higher FPS on v2 camera (untested!)
raspivid -md 7 -ex off -ss 4000 -ag 2 -dg 1 -awbg 1.5,1.2 -awb off -t 0 -n -o - -w 640 -h 480 -fps 200 -b 25000000 -cd MJPEG | cargo run --release
```

This is a quick replacement for VLC MJPEG streaming `| cvlc stream:///dev/stdin --sout '#standard{access=http{mime=multipart/x-mixed-replace;boundary=--7b3cc56e5f51db803f790dad720ed50a},mux=mpjpeg,dst=:8554/video.mjpg}` (and/or Python Flask streaming server).

Memory usage is low at 2.3 MB, but climbs with concurrent connection count as tokio can't free allocated slabs. The memory usage was 20.3 MB after `wrk -c 1000 -d 15 -t 300 http://localhost:8554/video.mjpg`

## Install

```sh
cargo build --release
sudo cp target/release/raspivid_mjpeg_server /usr/local/bin
raspivid -ISO 0 -t 0 -n -o - -w 640 -h 480 -fps 90 -b 25000000 -cd MJPEG | raspivid_mjpeg_streamer
```

# It's laggy!? (My network latency is 0.2 ms, the camera shutter is 8 ms, my screen refresh is 4 ms, how does that add up to 60 ms?)

The timestamp-to-screenshot pipeline is roughly:

1. <b>Update application picture (< 1 screen frame, depends on application)</b>
    1. The clock program gets the current time.
    1. The clock program creates a picture of the timestamp.
    1. The timestamp picture is composited onto the application picture.
    1. The application says it's frame time and tells the compositor to use the new application picture.
1. <b>Update screen frame (could be 1-3 screen frames depending on double/triple buffering)</b>
    1. The compositor composites the screen frame from the application pictures it has at hand.
    1. The compositor tells the GPU that it has a new screen frame.
1. <b>Display the screen frame (screen frame + display response time (could be anywhere from 2-50+ ms))</b>
    1. The GPU sends the screen frame to the display.
    1. The display reads the frame into its internal frame buffer.
    1. The display tells the hardware to wiggle the charges on the LCD elements.
    1. The charge shifts the opacity of the LCD element by reorienting the liquid crystals inside the element and changing its polarization.
1. <b>Capture the image (frame delay + exposure + read out time)</b>
    1. Raspivid notices that it's time to take a new image and tells the camera to make it happen.
    1. The camera sensor opens for recording.
    1. Photons hitting sensor cells increase the charge on the cell.
    1. At the end of the exposure time, the charge on the cells is drained out and converted by the camera ADC to a digital signal.
    1. The digital sensor values are read out to the Raspberry Pi memory for use by the GPU.
1. <b>Encode the image (faster than camera FPS in throughput, ~100 Mpixels per second on the RPi)</b>
    1. The RPi GPU converts the raw Bayer matrix sensor values to RGB pixels.
    1. The RPi GPU runs the RGB pixels through the JPEG encoder and attaches the MJPEG headers to each frame.
1. <b>Send the image to HTTP clients (latency of pushing 30 kB of data through a pipe and into a network buffer)</b>
    1. Raspivid receives a pointer to the JPEG buffer and copies it to its stdout.
    1. Raspivid_mjpeg_server reads the JPEG buffers from its stdin.
    1. Raspivid_mjpeg_server parses out the raw JPEG data, and attaches new MJPEG headers to each frame.
    1. Raspivid_mjpeg_server's open HTTP requests copy the latest MJPEG buffer from the read thread and copy it to the socket's send buffer.
1. <b>Network (ping latency + time to transfer 30 kB)</b>
    1. The socket asks the kernel TCP/IP stack to send the buffer, the TCP/IP stack asks the driver to ask the hardware to wiggle the charges on its wires.
    1. The wiggling charges are detected by the receiving end hardware, and passed onto the TCP/IP stack, and copied to the receiver's socket receive buffer.
    1. The receive buffer is copied to the viewer application.
1. <b>Decode the image (sub-ms to a few ms, depending on the decoder and resolution)</b>
    1. The viewer application parses out the JPEG in the receive buffer.
    1. The viewer application decodes the JPEG data into the image picture.
1. <b>Update application picture (< 1 screen frame)</b>
    1. The image picture is composited onto the application picture.
    1. The application says it's frame time and tells the compositor to use the new application picture.
1. <b>Update screen frame (1-3 screen frames)</b>
    1. The compositor composites the screen frame from the application pictures it has at hand.
    1. The compositor tells the GPU that it has a new screen frame.
    1. You grab a screenshot and it has the current timestamp picture and the image picture received from the camera.

If you want lower camera-to-screen latency, the easiest improvement is getting a 240Hz display and setting the camera FPS as high as you can. After that, get a better WiFi antenna / base station or switch to a wired connection. Try to get rid of double/triple buffering. Optimize your JPEG decoder. Hack the camera-to-streamer-to-viewer system to send data chunks as soon as they arrive instead of waiting for end of frame. Make the rolling shutter send 8 scanline chunks. Hack your display to immediately display 8 scanline chunks instead of full framebuffers. Directly drive your display from an FPGA with a wireless receiver. Ditto with the camera. Remove the camera sensor and display hardware, add a few lenses, galvos and a ground glass screen to build an optical image relay system.

Madness aside, you could try streaming raspiraw over GigE and running the Bayer-to-pixels conversion on the receiving end GPU. This could potentially get you 640x64 at 660 FPS with ~4 ms photons-to-GPU latency (1.5 ms exposure, 1.5 ms transfer, 1 ms for fooling around).

## License

MIT

2020 (c) Ilmari Heikkinen <hei@heichen.hk>
