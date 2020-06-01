// Copyright (C) 2019-2020 Motorola Solutions, Inc. All rights reserved.

#[macro_use]
extern crate log;

mod api;
mod domain;
mod gstlib;
mod moz_ice;

use gstreamer as gst;

use actix_web::{web, App, HttpServer};

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();
    gst::init().expect("Could not initialize gstreamer.");

    HttpServer::new(|| {
        let state = api::AppState::new();

        App::new()
            .data(state)
            .route("/{filename:.*}", web::get().to(api::index))
            .route("/request_offer", web::post().to(api::request_offer))
            .route("/provide_answer", web::post().to(api::provide_answer))
            .route(
                "/add_ice_candidate/{mline}",
                web::post().to(api::add_ice_candidate),
            )
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
