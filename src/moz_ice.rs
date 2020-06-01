// Copyright (C) 2019-2020 Motorola Solutions, Inc. All rights reserved.

use moz::attribute_type::{
    SdpAttributeCandidate as Candidate, SdpAttributeCandidateTransport as Transport,
    SdpAttributeCandidateType as CandidateType,
};
use std::str::FromStr;
use webrtc_sdp as moz;
use webrtc_sdp::address::Address;

use anyhow::{anyhow, Result};

// Parses the given raw candidate string into a webrtc_sdp::attribute_type::SdpAttributeCandidate
// that can be injected into an sdp payload.
pub fn to_moz_candidate(raw: &String) -> Result<Candidate> {
    let parts = raw
        .trim_start_matches("candidate:")
        .split(' ')
        .collect::<Vec<_>>();

    match parts.as_slice() {
        [foundation, component_id, transport, priority, address, port, "typ", "host", ..] => {
            assemble_host_candidate(foundation, component_id, transport, priority, address, port)
        }
        [foundation, component_id, transport, priority, address, port, "typ", candidate_type, "raddr", connection_address, "rport", connection_port, ..] => {
            assemble_extern_candidate(
                foundation,
                component_id,
                transport,
                priority,
                address,
                port,
                candidate_type,
                connection_address,
                connection_port,
            )
        }
        _ => Err(anyhow!("Could not parse line into valid ice candidate.")),
    }
}

fn assemble_host_candidate(
    foundation: &str,
    component_id: &str,
    transport: &str,
    priority: &str,
    address: &str,
    port: &str,
) -> Result<Candidate> {
    let parsed_component_id = component_id.parse::<u32>()?;
    let parsed_priority = priority.parse::<u64>()?;
    let parsed_port = port.parse::<u32>()?;
    let parsed_transport = match transport.to_lowercase().as_ref() {
        "udp" => Transport::Udp,
        "tcp" => Transport::Tcp,
        _ => return Err(anyhow!("Unknown ice transport type")),
    };

    let parsed_address = Address::from_str(address)?;
    let candidate_type = CandidateType::Host;

    let c = Candidate::new(
        foundation.to_string(),
        parsed_component_id,
        parsed_transport,
        parsed_priority,
        parsed_address,
        parsed_port,
        candidate_type,
    );
    Ok(c)
}

fn assemble_extern_candidate(
    foundation: &str,
    component_id: &str,
    transport: &str,
    priority: &str,
    address: &str,
    port: &str,
    candidate_type: &str,
    connection_address: &str,
    connection_port: &str,
) -> Result<Candidate> {
    let parsed_component_id = component_id.parse::<u32>()?;
    let parsed_priority = priority.parse::<u64>()?;
    let parsed_port = port.parse::<u32>()?;
    let parsed_transport = match transport.to_lowercase().as_ref() {
        "udp" => Transport::Udp,
        "tcp" => Transport::Tcp,
        _ => return Err(anyhow!("Unknown ice transport type")),
    };

    let parsed_address = Address::from_str(address)?;
    let candidate_type = match candidate_type.to_lowercase().as_ref() {
        "srflx" => CandidateType::Srflx,
        "prflx" => CandidateType::Prflx,
        "relay" => CandidateType::Relay,
        _ => return Err(anyhow!("Unknow candidate type value")),
    };

    let mut c = Candidate::new(
        foundation.to_string(),
        parsed_component_id,
        parsed_transport,
        parsed_priority,
        parsed_address,
        parsed_port,
        candidate_type,
    );

    let parsed_remote_address = Address::from_str(connection_address)?;
    let parsed_remote_port = connection_port.parse::<u32>()?;

    c.raddr = Some(parsed_remote_address);
    c.rport = Some(parsed_remote_port);
    Ok(c)
}
