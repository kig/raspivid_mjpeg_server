# raspivid_mjpeg_server

Low-latency video stream from Raspberry Pi to do remote OpenCV processing.

![Screen-to-screen latency](https://github.com/kig/raspivid_mjpeg_server/raw/master/images/latency.jpg)

Screen-to-screen latency with 50 FPS camera, 60Hz screen and Chrome as viewer is about 120ms over WiFi. Camera-to-OpenCV latency might be 30-60 ms lower. Measure by using an OpenCV app to display a timestamp, point the camera to it, and use the OpenCV app to save the image from the camera along with timestamp.

With 240Hz screen on Win10 Chrome and Raspberry Pi 3 v1 camera set to 120 FPS, the screen-to-screen latency is around 60 ms.

This is decent for e.g. driving an RC car from first-person view and some gestural interfaces.

## Usage

```sh
# Install Rust if needed
curl https://sh.rustup.rs -sSf | sh
. ~/.cargo/env 

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

## Install

```sh
cargo build --release
sudo cp target/release/raspivid_mjpeg_server /usr/local/bin
raspivid -ISO 0 -t 0 -n -o - -w 640 -h 480 -fps 90 -b 25000000 -cd MJPEG | raspivid_mjpeg_streamer
```

## Advanced usage

```
raspivid_mjpeg_server 0.2.0

USAGE:
    raspivid_mjpeg_server [OPTIONS]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -d, --delay <delay>      Delay in microseconds between frames read from files [default: 16000]
    -f, --file <filename>    Read frame filenames from the given file and loop over them. 
                             Use `-` to read from STDIN.
                             Set the frame rate with --delay
    -p, --port <port>        Listen for HTTP connections on this port [default: 8554]
```


## Notes

This is a quick replacement for VLC MJPEG streaming `| cvlc stream:///dev/stdin --sout '#standard{access=http{mime=multipart/x-mixed-replace;boundary=--7b3cc56e5f51db803f790dad720ed50a},mux=mpjpeg,dst=:8554/video.mjpg}` (and/or Python Flask streaming server).

Memory usage is low at 2.3 MB, but may climb with concurrent connection count. The memory usage was 20.3 MB after `wrk -c 1000 -d 15 -t 300 http://localhost:8554/video.mjpg`

CPU usage is ~2% on a RPi 3B+, which is slightly higher than `mjpg-streamer`.

# How to get lower latency?

If you want lower camera-to-screen latency, the easiest improvement is getting a 240Hz display and setting the camera FPS as high as you can. After that, get a better WiFi antenna / base station or switch to a wired connection. Try to get rid of double buffering. 

# Pipeline latency handwaving

Q: My network latency is 0.2 ms, the camera shutter is 8 ms, my screen refresh is 4 ms, how does that add up to 60 ms?

A: The timestamp-to-screenshot pipeline is roughly:

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

If you go with double-buffered latencies: timestamp-to-application takes 1 frame, displaying application takes 1 frames, jpeg-to-application takes 1 frame, displaying image application takes 1 frames, for a total of 4 frames of latency. For the display to change the frame takes 30 ms on a 60 Hz IPS screen, 12 ms on a 240 Hz screen. Then camera latency of 1 camera frame. With a 60Hz display and a 50Hz camera, that's 87 ms + 30 ms display response time. With 240 Hz display and 90 Hz camera, the timestamp-to-screenshot latency would be 28 ms + 12 ms DRT. Add in 10 ms WiFi latency and 16 ms frame transfer time, and the 60 Hz system is at 113 ms + 30 ms DRT and the 240 Hz system is at 54 ms + 12 ms DRT.

## Network latency

WiFi one-way latency with not-so-great signal can be 5-10 ms with spikes of tens of ms. With a better signal you can get the latency down to 1-2 ms. On Ethernet, latencies are from tens to low hundreds of microseconds and down. With 200 Gb HDR InfiniBand you could potentially reach ~12 microsecond frame transfer times for 30 kB frames.

## Camera-to-display latency

Camera-to-photons latencies would be one camera frame, two screen frames and display response time. For the above setups, 60/50 with 30 ms display response time: 83 ms, 240/90 with 12 ms DRT: 32 ms, 160 Hz CRT with a 200 Hz camera and zero DRT: 12 ms.

OLED display response is in the 1-2 ms range, so if you can find an OLED with a high refresh rate and low image processing delay, that can be good too. Samsung's QLED displays are in the low-single-digit milliseconds as well.

## Processing latency

The more processing you do per frame, the more UI lag you have. If you're running a 6 FPS CPU pose detector on a 30 Hz camera stream, it will have a minimum of 200 ms latency (and require a lot of smoothing/prediction to make it feel fluid). Switch to a 60 FPS detector and you'll get it down to 50 ms latency. 200 FPS YOLO detector and 200 Hz camera stream would have you at 10 ms latency. 

## Camera-to-processing latency

Camera to processing latencies are primarily driven by camera frame rate and network latency. Crank up the camera FPS, optimize your network and process the latest camera frame. Here's a table of theoretical latency figures for 30 kB frames.

<table>
<tr>
    <th>Camera FPS</th>
    <th>Network latency</th>
    <th>Network bandwidth</th>
    <th>Minimum response time</th>
</tr>
<tr>
    <td>30</td>
    <td>100 ms</td>
    <td>5 Mbps</td>
    <td>182 ms</td>
</tr>
<tr>
    <td>30</td>
    <td>10 ms</td>
    <td>20 Mbps</td>
    <td>55 ms</td>
</tr>
<tr>
    <td>60</td>
    <td>10 ms</td>
    <td>20 Mbps</td>
    <td>39 ms</td>
</tr>
<tr>
    <td>90</td>
    <td>10 ms</td>
    <td>20 Mbps</td>
    <td>34 ms</td>
</tr>
<tr>
    <td>200</td>
    <td>10 ms</td>
    <td>20 Mbps</td>
    <td>28 ms</td>
</tr>
<tr>
    <td>30</td>
    <td>1 ms</td>
    <td>100 Mbps</td>
    <td>36 ms</td>
</tr>
<tr>
    <td>60</td>
    <td>1 ms</td>
    <td>100 Mbps</td>
    <td>20 ms</td>
</tr>
<tr>
    <td>90</td>
    <td>1 ms</td>
    <td>100 Mbps</td>
    <td>16 ms</td>
</tr>
<tr>
    <td>200</td>
    <td>1 ms</td>
    <td>100 Mbps</td>
    <td>9 ms</td>
</tr>
<tr>
    <td>1 000</td>
    <td>0.05 ms</td>
    <td>1 Gbps</td>
    <td>1.5 ms</td>
</tr>
<tr>
    <td>12 600</td>
    <td>600 ns</td>
    <td>200 Gbps</td>
    <td>0.085 ms</td>
</tr>
</table>

## Codec latency

H.264 is not necessarily any worse than MJPEG in terms of latency, as long as you don't have B-frames (decoding a B-frame requires frames before and _after_ the B-frame). Hardware H.264 encoders shouldn't add much latency in Basic Profile (no B-frames). The main issue with H.264 is that decoder software likes to buffer a second of video stream before starting to decode it. H.264 requires a solid stream of data because decoding P-frames depends on the preceding frames (so if you've got a missing bits, your video will be corrupted until you hit the next I-frame). You can try watching an RTSP stream from a Basic Profile IP camera with `mplayer -benchmark` and it'll be quite low latency. Watch out for video corruption though!

To limit the corruption blast radius, you can reduce the I-frame interval. I-frames are full frames that don't need other frames to decode. This will increase the required bandwidth.

Latency of H.264 comes in two varieties: I-frame latency and P-frame latency. I-frames are full frames, so the transfer latency is higher. P-frames are small partial data frames, so the transfer latency is lower as well.

With MJPEG, every frame is an I-frame. This requires more bandwidth, but whatever missing data video corruption you encounter has a blast radius limited to the frames with missing data.

## Even lower latency? 

Optimize your JPEG encoder and decoder. Hack the camera-to-streamer-to-viewer system to send data chunks as soon as they arrive instead of waiting for end of frame. If using H.264, allow the receiver request a new I-frame when hitting missing packets. Make the rolling shutter send 8 scanline chunks. Hack your display to immediately display 8 scanline chunks instead of full framebuffers. Directly drive your display from an FPGA with a wireless receiver. Ditto with the camera. Make the camera expose and stream out randomly placed 8x8 blocks and display them immediately. Remove the camera sensor and display hardware, add a few lenses, galvos and a ground glass screen to build an optical image relay system.

Madness aside, you could try [streaming `raspiraw` from the v2 camera](https://www.raspberrypi.org/forums/viewtopic.php?f=43&t=212518&p=1310445) over Raspberry Pi 4's gigabit Ethernet and running the [Bayer-to-pixels conversion](https://github.com/6by9/dcraw) on the receiving end GPU. This could potentially get you 640x75 at 1007 FPS with ~2 ms photons-to-GPU latency (1 ms exposure, 0.4 ms transfer, 0.6 ms for fooling around).

## License

MIT

2020 (c) Ilmari Heikkinen <hei@heichen.hk>
