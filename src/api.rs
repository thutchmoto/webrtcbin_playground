// Copyright (C) 2019-2020 Motorola Solutions, Inc. All rights reserved.

use std::path::PathBuf;
use std::sync::Mutex;

use log::info;

use actix_files::NamedFile;
use actix_web::web;
use actix_web::{HttpRequest, Result};

use super::domain::*;

pub struct AppState {
    peer: Mutex<Option<Peer>>,
}

impl AppState {
    pub fn new() -> Self {
        AppState {
            peer: Mutex::new(None),
        }
    }
}

pub async fn index(req: HttpRequest) -> Result<NamedFile> {
    let path: PathBuf = req.match_info().query("filename").parse().unwrap();
    Ok(NamedFile::open(path)?)
}

pub async fn request_offer(state: web::Data<AppState>) -> Result<String> {
    info!("Receiver requested sdp offer");

    let (pipeline, webrtcbin, rx) =
        create_send_receive_pipeline().expect("Could not create pipeline");

    let offer = get_offer(&webrtcbin, rx).expect("Expected to generate offer");

    let p = Peer {
        pipeline,
        webrtcbin,
    };

    let mut peer = state.peer.lock().unwrap();
    *peer = Some(p);

    Ok(offer)
}

pub async fn provide_answer(body: String, state: web::Data<AppState>) -> Result<String> {
    info!("Received answer for video receiver: \r\n{}", body);

    if let Some(s) = state.peer.lock().unwrap().as_ref() {
        process_sdp_answer(&s.webrtcbin, body);
    }

    Ok("ok".to_string())
}

pub async fn add_ice_candidate(
    req: web::HttpRequest,
    body: String,
    state: web::Data<AppState>,
) -> Result<String> {
    let mline = req
        .match_info()
        .get("mline")
        .map(|s| s.parse::<u32>())
        .expect("Expected mline in the path")
        .expect("Could not parse mline index");

    info!("Received ice candidate.: {}, {}", mline, body);

    if let Some(s) = state.peer.lock().unwrap().as_ref() {
        add_remote_candidate(&s.webrtcbin, mline, &body);
    }

    Ok("ok".to_string())
}
