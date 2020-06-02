// Copyright (C) 2019-2020 Motorola Solutions, Inc. All rights reserved.

use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use gst::prelude::*;
use gstreamer as gst;

use gstreamer_sdp as gst_sdp;
use gstreamer_webrtc as gst_webrtc;

use anyhow::{anyhow, Result};

use super::gstlib::*;
use super::moz_ice;

type StdResult<L, R> = std::result::Result<L, R>;

#[derive(Debug, Clone)]
pub struct Peer {
    pub pipeline: gst::Pipeline,
    pub webrtcbin: gst::Element,
}

#[derive(Debug, Clone)]
pub struct IceCandidate {
    pub media_line_index: u32,
    pub candidate_str: String,
}

const PREFIX_ATTRIBUTE: &str = "a=";
const PREFIX_ATTRIBUTE_CANDIDATE: &str = "a=candidate";

impl IceCandidate {
    pub fn new(media_line_index: u32, candidate_str: String) -> Self {
        IceCandidate {
            media_line_index: media_line_index,
            candidate_str: candidate_str,
        }
    }

    pub fn get_media_line_index(&self) -> u32 {
        self.media_line_index
    }

    pub fn get_candidate_str(&self) -> String {
        self.candidate_str.clone()
    }
}

pub fn create_pipeline(source: &str) -> Result<gst::Pipeline> {
    let pipeline = gst::parse_launch(source)?.to_pipeline();

    Ok(pipeline)
}

pub fn auto_create_offer(webrtcbin: &gst::Element) -> Result<()> {
    let webrtcclone = webrtcbin.clone();
    let promise = gst::Promise::new_with_change_func(move |reply| {
        if let Ok(r) = reply {        
            let offer = r
                .get_value("offer")
                .unwrap()
                .get::<gst_webrtc::WebRTCSessionDescription>()
                .expect("Invalid argument")
                .unwrap();

            let raw_offer = offer.get_sdp().as_text().unwrap();
            info!("Webrtcbin emitted offer {}", raw_offer);

            info!("Setting local description from SDP Offer");
            webrtcclone
                .emit("set-local-description", &[&offer, &None::<gst::Promise>])
                .unwrap();
            
        }
    });
        
    webrtcbin
        .emit("create-offer", &[&None::<gst::Structure>, &promise])
        .unwrap();

    Ok(())
}

pub fn get_offer(webrtcbin: &gst::Element, ice_receiver: Receiver<IceCandidate>) -> Result<String> {
    let offer = block_get_local_description(webrtcbin)?;

    let raw_offer = offer.get_sdp().as_text().unwrap();

    let local_candidates =
        block_gather_local_candidates(ice_receiver, 16, Duration::from_millis(100));
    let adjusted_offer = insert_local_candidates_into_sdp(&raw_offer, &local_candidates)?
        .to_string()
        .replace("\r\n\r\n", "\r\n");

    Ok(adjusted_offer)
}

/// Processes an sdp offer, and provides an sdp answer
/// with pre-gathered ice candidates
pub fn process_sdp_offer_no_trickle(webrtcbin: &gst::Element, raw_sdp: String) -> Result<String> {
    
    // Gstreamer can actually panic and crash the process if we give it invalid sdp, or
    // sdp invalid for webrtc; for example, sdp that is valid for sip and audio will
    // cause a gstreamer panic. So here we are doing a simple sanity check with the moz
    // library. If it parses there, we'll assume gstreamer will handle it
    info!("Processing sdp offer: {}", raw_sdp);
    let parsed_sdp = validate_sdp(&raw_sdp)?;
    
    // setup the ice candidate channels
    let (ice_tx, ice_rx): (Sender<IceCandidate>, Receiver<IceCandidate>) =
        mpsc::channel();

    // bind and listen for candidates; gathered candidates will be sent on this channel
    listen_for_local_candidates(webrtcbin, ice_tx);

    let parsed_sdp = gst_sdp::SDPMessage::parse_buffer(raw_sdp.as_bytes())
        .map_err(|_| anyhow!("Failed to parse SDP offer"))?;

    let offer =
        gst_webrtc::WebRTCSessionDescription::new(gst_webrtc::WebRTCSDPType::Offer, parsed_sdp);

    info!("Setting remote description from SDP Offer");
    webrtcbin
        .emit("set-remote-description", &[&offer, &None::<gst::Promise>])
        .unwrap();

    extract_candidates(&raw_sdp).iter().for_each(|c| {
        add_remote_candidate(webrtcbin, 0, c);
    });

    let webrtcclone = webrtcbin.clone();

    // the channel lets us send the answer from inside the promise to this current thread
    let (answer_tx, answer_rx): (Sender<Result<String>>, Receiver<Result<String>>) =
        mpsc::channel();

    // this promise is our event handler for when webrtcbin generates an answering sdp
    // We will just take the raw sdp and send it on the channel to communicate with ourselves
    // outside of the promise
    let promise = gst::Promise::new_with_change_func(move |reply| {
        match reply {
            Ok(reply) => {
                let answer = reply
                    .get_value("answer")
                    .unwrap()
                    .get::<gst_webrtc::WebRTCSessionDescription>()
                    .expect("Invalid argument")
                    .unwrap();

                let raw_answer = answer.get_sdp().as_text().unwrap();
                info!("Webrtcbin emitted answer {}", raw_answer);

                info!("Setting local description from SDP Answer");
                webrtcclone
                    .emit("set-local-description", &[&answer, &None::<gst::Promise>])
                    .unwrap();

                answer_tx.send(Ok(raw_answer));
            }
            Err(_) => {
                answer_tx.send(Err(anyhow!("No answer provided")));
            }
        };
    });

    webrtcbin
        .emit("create-answer", &[&None::<gst::Structure>, &promise])
        .unwrap();

    // read the answer
    let answer_result = answer_rx
        .recv()
        .expect("Could not read answer sdp from webrtcbin")
        .expect("No answer result");

    let local_candidates = block_gather_local_candidates(ice_rx, 16, Duration::from_millis(100));
    let adjusted_answer =
        insert_local_candidates_into_sdp(&answer_result, &local_candidates)?
        .to_string()
        .replace("\r\n\r\n", "\r\n");

    info!("Answer with ice candidates: {}", adjusted_answer);
    
    // since we have an offer *and* an answer, try to find the 
    //gst::debug_bin_to_dot_file(&pipeline, gst::DebugGraphDetails::all(), "webrtc_pipeline");
    Ok(adjusted_answer)
}

pub fn process_sdp_answer(webrtcbin: &gst::Element, raw_sdp: String) -> Result<()> {
    info!("Processing sdp answer: {}", raw_sdp);
    validate_sdp(&raw_sdp)?;

    let ret = gst_sdp::SDPMessage::parse_buffer(raw_sdp.as_bytes())
        .map_err(|_| anyhow!("Failed to parse SDP answer"))?;
    let answer = gst_webrtc::WebRTCSessionDescription::new(gst_webrtc::WebRTCSDPType::Answer, ret);

    webrtcbin
        .emit("set-remote-description", &[&answer, &None::<gst::Promise>])
        .unwrap();

    let candidates = extract_candidates(&raw_sdp);
    candidates.iter().for_each(|candidate| {
        add_remote_candidate(&webrtcbin, 0, candidate)
            .unwrap();
    });

    Ok(())
}

pub fn add_remote_candidate(
    webrtcbin: &gst::Element,
    media_line_index: u32,
    candidate_str: &str,
) -> Result<()> {
    webrtcbin
        .emit("add-ice-candidate", &[&media_line_index, &candidate_str])
        .unwrap();

    Ok(())
}

/// Uses mozilla's webrtc_sdp library to parse the sdp, kind of an extra layer of protection
/// around the builting gstreamer webrtc sdp support
fn validate_sdp(sdp: &String) -> Result<webrtc_sdp::SdpSession> {
    webrtc_sdp::parse_sdp(sdp, false).map_err(|_| anyhow!("The provided sdp is not valid."))
}

/// Listens for the gathering of local ice candidates
pub fn listen_for_local_candidates(webrtcbin: &gst::Element, sender: Sender<IceCandidate>) {
    let shared_sender = Mutex::new(sender);

    // wire up a candidate receiver
    webrtcbin
        .connect("on-ice-candidate", false, move |values| {
            let _webrtc = values[0]
                .get::<gst::Element>()
                .expect("Invalid argument")
                .expect("Should never be null.");
            let mlineindex = values[1].get_some::<u32>().expect("Invalid argument");
            let candidate_raw = values[2]
                .get::<String>()
                .expect("Invalid argument")
                .unwrap();

            let candidate = IceCandidate::new(mlineindex, candidate_raw.clone());

            info!(
                "Gathered local ice candidate. mlineindex={}, candidate={}",
                mlineindex, candidate_raw
            );

            let send_result = shared_sender.lock().unwrap().send(candidate);

            if let Err(_) = send_result {
                debug!("Could not send ice candidate. Receiver closed?");
            }

            None
        })
        .unwrap();
}

/// Waits until we have gathered all of the candidates for the local description
fn block_gather_local_candidates(
    rx: Receiver<IceCandidate>,
    max_items: usize,
    timeout: Duration,
) -> Vec<IceCandidate> {
    let mut received = vec![];
    let mut continue_polling = true;

    while continue_polling && (received.len() < max_items) {
        match rx.recv_timeout(timeout) {
            Ok(c) => received.push(c),
            _ => {
                continue_polling = false;
            }
        };
    }

    received
}

/// Gets the local description from the provided webrtcbin. The local description may have
/// been set as part of sdp negotiation, but the actual setting is async, so it may not show up
/// in the "local-description" property. This function blocks indefinitely until the value is non-none
/// Internally it does a thread::sleep for 1 ms; I'm sure there's a better rust way of doing this
fn block_get_local_description(
    webrtcbin: &gst::Element,
) -> Result<gst_webrtc::WebRTCSessionDescription> {
    let mut local = None;

    let delay = Duration::from_millis(1);
    while local.is_none() {
        local = webrtcbin
            .get_property("local-description")
            .expect("Requires local description")
            .get::<gst_webrtc::WebRTCSessionDescription>()
            .expect("Actual sdp");

        // TODO - find better mechanism. Park? Condvar?
        thread::sleep(delay)
    }

    Ok(local.unwrap())
}

/// Inserts the provided ice candidates into an sdp payload, respecting the relative media lines.
fn insert_local_candidates_into_sdp(
    raw_sdp: &String,
    local_candidates: &Vec<IceCandidate>,
) -> Result<webrtc_sdp::SdpSession> {
    let mut session = validate_sdp(&raw_sdp)?;

    local_candidates.iter().for_each(|c| {
        let index = c.get_media_line_index() as usize;
        let mut media = session
            .media
            .get(index)
            .cloned()
            .expect("Could not find matching media for ice candidate");

        if let Ok(parsed) = moz_ice::to_moz_candidate(&c.get_candidate_str()) {
            let attribute = webrtc_sdp::attribute_type::SdpAttribute::Candidate(parsed);

            if let Ok(_) = media.add_attribute(attribute) {
                std::mem::replace(&mut session.media[index], media);
            }
        }
    });

    Ok(session)
}

/// Parses the given sdp for all ice candidates
/// Returns a vec of simple strings, with the leading a= removed. 
/// For, example, the following sdp:
///     a=rtcp:9 IN IP4 0.0.0.0
///     a=candidate:3719404024 1 udp 2122260223 192.168.0.91 55827 typ host generation 0 network-id 1 network-cost 10
///     a=ice-ufrag:avZ6
/// yields vec!["candidate:3719404024 1 udp 2122260223 192.168.0.91 55827 typ host generation 0 network-id 1 network-cost 10"]
/// Ice candidates need to be added to webrtcbin with a media line index, but we're forcing max-bundle, which means
/// all streams are transmitted over the same connection, which ultimately will be the first media; media line index will be always
/// be zero in max-bundle
fn extract_candidates(sdp: &String) -> Vec<String> {
    let lines = sdp.lines().collect::<Vec<_>>();
    lines
        .iter()
        .filter(|l| l.starts_with(PREFIX_ATTRIBUTE_CANDIDATE))
        .map(|l| l.trim_start_matches(PREFIX_ATTRIBUTE).to_string())
        .collect::<Vec<_>>()
}

/// Called by the pad-added event on webrtcbin; only *after* successful ice negotiation
pub fn on_incoming_stream(pipeline: &gst::Pipeline, pad: &gst::Pad) -> Result<()>{
    if pad.get_direction() != gst::PadDirection::Src {
        return Ok(());
    }

    let decodebin = gst::ElementFactory::make("decodebin", None).unwrap();
    let pipeclone = pipeline.clone();
    decodebin.connect_pad_added(move |_decodebin, pad| {
        add_stream_destination(&pipeclone, pad).expect("Could not add stream destination.");
    });

    pipeline.add(&decodebin).unwrap();
    decodebin.sync_state_with_parent().unwrap();

    let sinkpad = decodebin.get_static_pad("sink").unwrap();
    pad.link(&sinkpad).unwrap();

    Ok(())
}

/// creates a destination where audio or video will be dumped, depending on the
/// pad's capabilities.
/// 
/// The gstwebrtc-demos dumps the media to autovideosink or autoaudiosink, which
/// demonstrates that media is actually flowing bidirectionally. 
/// Here's we're just going to dump to fake sinks instead
fn add_stream_destination(pipeline: &gst::Pipeline, pad: &gst::Pad) -> Result<()> {
    let caps = pad.get_current_caps().unwrap();
    let name = caps.get_structure(0).unwrap().get_name();

    let sink = if name.starts_with("video/") {
        gst::parse_bin_from_description(
            "queue ! videoconvert ! videoscale ! fakesink",
            true,
        )?
    } else if name.starts_with("audio/") {
        gst::parse_bin_from_description(
            "queue ! audioconvert ! audioresample ! fakesink",
            true,
        )?
    } else {
        println!("Unknown pad {:?}, ignoring", pad);
        return Ok(());
    };

    pipeline.add(&sink).unwrap();
    sink.sync_state_with_parent()?;

    let sinkpad = sink.get_static_pad("sink").unwrap();
    pad.link(&sinkpad)?;

    Ok(())
}