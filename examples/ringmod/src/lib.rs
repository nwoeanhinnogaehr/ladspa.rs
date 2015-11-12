extern crate ladspa;

use ladspa::{Plugin, PluginDescriptor, Port, PortConnection, Data};
use std::default::Default;

struct RingMod {
    time: u64,
    sample_rate: u64,
}

fn new_ringmod(_: &PluginDescriptor, sample_rate: u64) -> Box<Plugin + Send> {
    Box::new(RingMod {
        time: 0,
        sample_rate: sample_rate,
    })
}

impl Plugin for RingMod {
    fn run<'a>(&mut self, sample_count: usize, ports: &[&'a PortConnection<'a>]) {
        let input = ports[0].unwrap_audio();
        let mut output = ports[1].unwrap_audio_mut();
        let freq = *ports[2].unwrap_control();
        for i in 0..sample_count {
            output[i] = input[i];

            let time = (i as Data + self.time as Data) / self.sample_rate as Data;
            output[i] *= (2.0*3.14159*freq*time).sin();
        }
        self.time += sample_count as u64;
    }
    fn activate(&mut self) {
        self.time = 0;
    }
}

#[no_mangle]
pub extern fn get_ladspa_descriptor(index: u64) -> Option<PluginDescriptor> {
    match index {
        0 => {
            Some(PluginDescriptor {
                unique_id: 401,
                label: "ring_mod",
                properties: ladspa::PROP_NONE,
                name: "Mono Ring Modulator",
                maker: "Noah Weninger",
                copyright: "None",
                ports: vec![Port {
                    name: "Audio In",
                    desc: ladspa::PortDescriptor::AudioInput,
                    .. Default::default()
                }, Port {
                    name: "Audio Out",
                    desc: ladspa::PortDescriptor::AudioOutput,
                    .. Default::default()
                }, Port {
                    name: "Frequency",
                    desc: ladspa::PortDescriptor::ControlInput,
                    hint: Some(ladspa::HINT_SAMPLE_RATE | ladspa::HINT_LOGARITHMIC),
                    default: Some(ladspa::DefaultValue::Value440),
                    lower_bound: Some(0.0),
                    upper_bound: Some(0.5),
                }],
                new: new_ringmod
            })
        },
        _ => None
    }
}
