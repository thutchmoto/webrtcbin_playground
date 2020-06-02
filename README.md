# Introduction
This repo is a playground for exploring the behavior of GStreamer's webrtcbin element. There are two primary components: the backend media server, written in rust, and the various front-end html pages to exercise the different scenarios. 

Much of this code is an attempt to replicate or emulate the behaviors in [centricular's gstwebrtc-demos](https://github.com/centricular/gstwebrtc-demos). At first only the sendrecv scenario is replicated.

## Backend
The backend is currently built with rust 1.42 nightly. It uses the rust gstreamer bindings, and actix-web for the web frontend. Html pages are served statically. 

At present, there is no stun/turn. It's assumed everything is localhost, so ice gathering should be nearly instantaneous. 

## Frontend
The frontend is simple html/javascript that uses the basic webrtc api to create offers/answers and to share ice candidates. Different scenarios will be implemented in separate pages.

## Running
It's simple: 
```
$ cargo run
```

And then open your browser and navigate to: http://localhost:8080/send_receive.html

## No-trickle ICE
The backend does not advertise ice candidates to the front end using trickle ice. Instead, ice candidates are gathered and then manually inserted into the SDP presented to the browser. The reason for this decision is that an internal use case does not allow for an out-of-band channel (websocket or otherwise) over which candidates can be advertiesed to the remote party. In fact, that was the catalyst for this playground; i.e., to adapt the original centricular sendrecv example to not use trickle. 

To get around the lack of media server -> browser trickle channel, the code simply iterates over all locally gathered candidates until a hard-coded maximum number is reached, or until 100ms has elapsed between discovery of the next candidate. It's a hack, but is necessary until a fix for [this defect](https://gitlab.freedesktop.org/gstreamer/gst-plugins-bad/-/issues/676) becomes available in an actual release. 

## Scenario: SENDRECV
This is an extremely simple scenario. The user presses `Request Offer` button, the client page requests an offer from the media server.

The media server creates a new pipeline and produces an SDP offer. The pipeline is the same configuration as defined in the centricular example:
```
videotestsrc pattern=ball is-live=true ! vp8enc deadline=1 ! rtpvp8pay pt=96 ! webrtcbin. 
audiotestsrc is-live=true ! opusenc ! rtpopuspay pt=97 ! webrtcbin. 
webrtcbin name=webrtcbin
```
The offer, with ice candidates, is returned to the browser. The client page creates an answer and posts it back to the media server. The client page also posts ice candidates to the media server as they are discovered.

After SDP has been exchanged and ice negotiation is complete, the client page should show the video of the bouncing ball and the white noise. 

To keep things simply, this is a directional flow (i.e., a=sendrecv), and both audio and video are offered.

## Scenario: SENDRECV No Trickle
This scenario is identical to the SENDRECV scenario, except the exchange of ice candidates from the browser to the media server does not occur over a trickle channel. Instead, the ice candidates are included in the sdp answer submitted to the server. 

On the backend, the media server must extract the ice candidates from the payload and manually add them to webrtcbin via the `add-ice-candidate` signal. I.e., webrtcbin does not support non-trickle workflows; it will not automatically inspect the submitted sdp for ice candidates. And without emiting that signal, the ice state machine will never progress, which prevents the pad-added signal from ever firing, which prevents media from ever actually flowing. 

This scenario is exercised from the page: http://localhost:8080/send_receive_no_trickle.html
