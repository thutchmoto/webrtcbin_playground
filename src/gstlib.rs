// Copyright (C) 2019-2020 Motorola Solutions, Inc. All rights reserved.

use gst::prelude::*;
use gstreamer as gst;

pub trait ToPipeline {
    fn to_pipeline(&self) -> gst::Pipeline;
}

pub trait ToBin {
    fn to_bin(&self) -> gst::Bin;
}

pub trait ToElement {
    fn to_element(&self) -> gst::Element;
}

pub trait HasPipeline {
    fn get_pipeline(&self) -> gst::Pipeline;
}

impl ToPipeline for gst::Element {
    fn to_pipeline(&self) -> gst::Pipeline {
        self.clone()
            .downcast::<gst::Pipeline>()
            .expect("Could not downcast element to Pipeline.")
    }
}

impl ToBin for gst::Element {
    fn to_bin(&self) -> gst::Bin {
        self.clone()
            .downcast::<gst::Bin>()
            .expect("Could not downcast element to Bin.")
    }
}

impl ToBin for gst::Pipeline {
    fn to_bin(&self) -> gst::Bin {
        self.clone().upcast::<gst::Bin>()
    }
}

impl ToElement for gst::Pipeline {
    fn to_element(&self) -> gst::Element {
        self.clone().upcast::<gst::Element>()
    }
}

impl ToElement for gst::Bin {
    fn to_element(&self) -> gst::Element {
        self.clone().upcast::<gst::Element>()
    }
}
