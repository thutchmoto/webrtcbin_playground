// Copyright (C) 2019-2020 Motorola Solutions, Inc. All rights reserved.

use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};

use gst::prelude::*;
use gstreamer as gst;

use gstreamer_webrtc as gst_webrtc;

use super::domain::*;

type StdResult<L, R> = std::result::Result<L, R>;

pub fn create_send_receive_pipeline(
) -> StdResult<(gst::Pipeline, gst::Element, Receiver<IceCandidate>), String> {
    let pipe_source =
        "videotestsrc pattern=ball is-live=true ! vp8enc deadline=1 ! rtpvp8pay pt=96 ! webrtcbin. \
        audiotestsrc is-live=true ! opusenc ! rtpopuspay pt=97 ! webrtcbin. \
        webrtcbin name=webrtcbin";

    let pipeline = match create_pipeline(pipe_source) {
        Ok(r) => Ok(r),
        Error => Err("Could not create pipeline"),
    }?;

    let webrtcbin = pipeline
        .get_by_name("webrtcbin")
        .expect("Could not find webrtcbin element");

    webrtcbin.set_property_from_str("bundle-policy", "max-bundle");

    pipeline.call_async(|p| {
        p.set_state(gst::State::Playing)
            .expect("Couldn't set pipeline to Playing");
        info!("Started webrtc pipeline.");
    });

    // setup the ice candidate channels
    let (ice_tx, ice_rx): (Sender<IceCandidate>, Receiver<IceCandidate>) = mpsc::channel();

    // bind and listen for candidates; gathered candidates will be sent on this channel
    listen_for_local_candidates(&webrtcbin, ice_tx);

    webrtcbin
        .connect("on-negotiation-needed", false, move |values| {
            let _webrtc = values[0]
                .get::<gst::Element>()
                .expect("Invalid argument")
                .expect("Should never be null.");

            let local_description = _webrtc
                .get_property("local-description")
                .expect("Expected local description.")
                .get::<gst_webrtc::WebRTCSessionDescription>()
                .expect("Invalid argument");

            let remote_description = _webrtc
                .get_property("remote-description")
                .expect("Expected remote description.")
                .get::<gst_webrtc::WebRTCSessionDescription>()
                .expect("Invalid argument");

            info!(
                "Negotiation needed. Has local {}, has remote {}",
                local_description.is_some(),
                remote_description.is_some()
            );

            auto_create_offer(&_webrtc)
                .expect("Could not automatically create offer.");
            None
        })
        .unwrap();

    let pad_added_pipeline = pipeline.clone();
    webrtcbin.connect_pad_added(move |_webrtc, pad| {
        on_incoming_stream(&pad_added_pipeline, pad)
            .expect("Could not decode incoming stream.");
        info!("Connected to new pad");
    });

    webrtcbin
        .connect("on-new-transceiver", false, move |values| {
            let _webrtc = values[0]
                .get::<gst::Element>()
                .expect("Invalid argument")
                .expect("Should never be null.");
            let transceiver = values[1]
                .get::<gst_webrtc::WebRTCRTPTransceiver>()
                .expect("Invalid argument")
                .unwrap();
            let mlineindex = transceiver.get_property_mlineindex();
            info!("New transceiver added; mlineindex = {}", mlineindex);

            None
        })
        .unwrap();

    return Ok((pipeline, webrtcbin, ice_rx));
}