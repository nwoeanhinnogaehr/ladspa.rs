use std::{mem, ptr};
use libc::{self, c_char, c_ulong};
use std::slice;
use std::cell::RefCell;
use vec_map::VecMap;
use std::ffi::CString;
use std::panic::{catch_unwind, AssertUnwindSafe};

use super::PluginDescriptor;
use super::get_ladspa_descriptor;

macro_rules! call_user_code {
    ($code:expr, $name:expr) => {
        match catch_unwind(move || $code) {
            Ok(x) => x,
            Err(_) => {
                println!("ladspa.rs: panic in {} suppressed.", $name);
                None
            }
        }
    }
}

// essentially ladspa.h API translated to rust.
pub mod ladspa_h {
    use libc::{c_void, c_char, c_int, c_ulong, c_float};

    pub type Data = c_float;
    pub type Properties = c_int;
    pub type PortDescriptor = c_int;
    pub type PortRangeHintDescriptor = c_int;

    pub type Handle = *mut c_void;

    #[repr(C)]
    #[derive(Copy, Clone)]
    pub struct PortRangeHint {
        pub hint_descriptor: PortRangeHintDescriptor,
        pub lower_bound: Data,
        pub upper_bound: Data,
    }

    #[repr(C)]
    #[allow(missing_copy_implementations)] // Remove this for a fun warning/suggestion cycle!
    pub struct Descriptor {
        pub unique_id: c_ulong,
        pub label: *mut c_char,
        pub properties: Properties,
        pub name: *mut c_char,
        pub maker: *mut c_char,
        pub copyright: *mut c_char,
        pub port_count: c_ulong,
        pub port_descriptors: *mut PortDescriptor,
        pub port_names: *mut *mut c_char,
        pub port_range_hints: *mut PortRangeHint,
        pub implementation_data: *mut c_void,
        pub instantiate: extern "C" fn(descriptor: *const Descriptor, sample_rate: c_ulong) -> Handle,
        pub connect_port: extern "C" fn(instance: Handle, port: c_ulong, data_location: *mut Data),
        pub activate: Option<extern "C" fn(instance: Handle)>,
        pub run: extern "C" fn(instance: Handle, sample_count: c_ulong),
        pub run_adding: Option<extern "C" fn(instance: Handle, sample_count: c_ulong)>,
        pub set_run_adding_gain: Option<extern "C" fn(instance: Handle, gain: Data)>,
        pub deactivate: Option<extern "C" fn(instance: Handle)>,
        pub cleanup: extern "C" fn(instance: Handle),
    }

    pub const PROPERTY_REALTIME: Properties = 0x1;
    pub const PROPERTY_INPLACE_BROKEN: Properties = 0x2;
    pub const PROPERTY_HARD_RT_CAPABLE: Properties = 0x4;

    pub const PORT_INPUT: PortDescriptor = 0x1;
    pub const PORT_OUTPUT: PortDescriptor = 0x2;
    pub const PORT_CONTROL: PortDescriptor = 0x4;
    pub const PORT_AUDIO: PortDescriptor = 0x8;

    pub const HINT_BOUNDED_BELOW: PortRangeHintDescriptor = 0x1;
    pub const HINT_BOUNDED_ABOVE: PortRangeHintDescriptor = 0x2;
    pub const HINT_TOGGLED: PortRangeHintDescriptor = 0x4;
    pub const HINT_SAMPLE_RATE: PortRangeHintDescriptor = 0x8;
    pub const HINT_LOGARITHMIC: PortRangeHintDescriptor = 0x10;
    pub const HINT_INTEGER: PortRangeHintDescriptor = 0x20;
    pub const HINT_DEFAULT_MINIMUM: PortRangeHintDescriptor = 0x40;
    pub const HINT_DEFAULT_LOW: PortRangeHintDescriptor = 0x80;
    pub const HINT_DEFAULT_MIDDLE: PortRangeHintDescriptor = 0xC0;
    pub const HINT_DEFAULT_HIGH: PortRangeHintDescriptor = 0x100;
    pub const HINT_DEFAULT_MAXIMUM: PortRangeHintDescriptor = 0x140;
    pub const HINT_DEFAULT_0: PortRangeHintDescriptor = 0x200;
    pub const HINT_DEFAULT_1: PortRangeHintDescriptor = 0x240;
    pub const HINT_DEFAULT_100: PortRangeHintDescriptor = 0x280;
    pub const HINT_DEFAULT_440: PortRangeHintDescriptor = 0x2C0;
}

static mut DESCRIPTORS: *mut Vec<*mut ladspa_h::Descriptor> =
    0 as *mut Vec<*mut ladspa_h::Descriptor>;

// It seems that ladspa_descriptor is deleted during link time optimization unless we
// call it from somewhere.
#[allow(dead_code)]
unsafe fn _lto_workaround() {
    ladspa_descriptor(0);
}

#[no_mangle]
// Exported so the plugin is recognised by ladspa hosts.
pub unsafe extern "C" fn ladspa_descriptor(index: c_ulong) -> *mut ladspa_h::Descriptor {
    if DESCRIPTORS == ptr::null_mut() {
        libc::atexit(global_destruct);
        DESCRIPTORS = mem::transmute(Box::new(Vec::<*mut ladspa_h::Descriptor>::new()));
    }

    // If it's already been generated, return the cached copy.
    if (index as usize) < (*DESCRIPTORS).len() {
        return mem::transmute(&*(*DESCRIPTORS)[index as usize]);
    }

    let descriptor = call_user_code!(get_ladspa_descriptor(index as u64), "get_ladspa_descriptor");

    match descriptor {
        Some(plugin) => {
            let desc = mem::transmute(Box::new(ladspa_h::Descriptor {
                unique_id: plugin.unique_id as c_ulong,
                label: CString::new(plugin.label).unwrap().into_raw(),
                properties: plugin.properties.bits(),
                name: CString::new(plugin.name).unwrap().into_raw(),
                maker: CString::new(plugin.maker).unwrap().into_raw(),
                copyright: CString::new(plugin.copyright).unwrap().into_raw(),

                port_count: plugin.ports.len() as c_ulong,
                port_descriptors: mem::transmute::<_, &mut [i32]>(
                    plugin.ports.iter().map(|port|
                                            port.desc as i32
                                           ).collect::<Vec<_>>().into_boxed_slice()).as_mut_ptr(),
                port_names: mem::transmute::<_, &mut [*mut c_char]>(
                    plugin.ports.iter().map(|port|
                                            CString::new(port.name).unwrap().into_raw()
                                           ).collect::<Vec<_>>().into_boxed_slice()).as_mut_ptr(),
                port_range_hints: mem::transmute::<_, &mut [ladspa_h::PortRangeHint]>(
                    plugin.ports.iter().map(|port|
                                            ladspa_h::PortRangeHint {
                                                hint_descriptor: port.hint.map(|x| x.bits()).unwrap_or(0) |
                                                    port.default.map(|x| x as i32).unwrap_or(0) |
                                                    port.lower_bound.map(|_| ladspa_h::HINT_BOUNDED_BELOW)
                                                    .unwrap_or(0) |
                                                    port.upper_bound.map(|_| ladspa_h::HINT_BOUNDED_ABOVE)
                                                    .unwrap_or(0),
                                                 lower_bound: port.lower_bound.unwrap_or(0_f32),
                                                 upper_bound: port.upper_bound.unwrap_or(0_f32),
                                            }
                                         ).collect::<Vec<_>>().into_boxed_slice()).as_mut_ptr(),
                implementation_data: mem::transmute(Box::new(plugin)),
                instantiate: instantiate,
                connect_port: connect_port,
                run: run,
                cleanup: cleanup,
                run_adding: None,
                set_run_adding_gain: None,
                activate: Some(activate),
                deactivate: Some(deactivate),
            }));

            // store in global descriptor table
            (*DESCRIPTORS).push(desc);
            desc
        }
        None => ptr::null_mut(),
    }
}

extern "C" fn global_destruct() {
    unsafe {
        let descs: Box<Vec<*mut ladspa_h::Descriptor>> = mem::transmute(DESCRIPTORS);
        for desc in descs.iter() {
            drop_descriptor(mem::transmute(*desc));
        }
    }
}

unsafe fn drop_descriptor(desc: &mut ladspa_h::Descriptor) {
    CString::from_raw(desc.label);
    CString::from_raw(desc.name);
    CString::from_raw(desc.maker);
    CString::from_raw(desc.copyright);
    Vec::from_raw_parts(desc.port_descriptors,
                        desc.port_count as usize,
                        desc.port_count as usize);
    Vec::from_raw_parts(desc.port_names,
                        desc.port_count as usize,
                        desc.port_count as usize)
        .iter()
        .map(|&x| CString::from_raw(x))
        .collect::<Vec<_>>();
    Vec::from_raw_parts(desc.port_range_hints,
                        desc.port_count as usize,
                        desc.port_count as usize);
    mem::transmute::<_, Box<PluginDescriptor>>(desc.implementation_data);
}

// The handle that is given to ladspa.
struct Handle<'a> {
    descriptor: &'static super::PluginDescriptor,
    plugin: Box<super::Plugin + Send + 'static>,
    port_map: VecMap<super::PortConnection<'a>>,
    ports: Vec<&'a super::PortConnection<'a>>,
}

extern "C" fn instantiate(descriptor: *const ladspa_h::Descriptor,
                          sample_rate: c_ulong)
                          -> ladspa_h::Handle {
    unsafe {
        let desc: &mut ladspa_h::Descriptor = mem::transmute(descriptor);

        let rust_desc: &super::PluginDescriptor = mem::transmute(desc.implementation_data);
        let rust_plugin = match call_user_code!(Some((rust_desc.new)(rust_desc, sample_rate as u64)),
                                                "PluginDescriptor::run") {
            Some(plug) => plug,
            None => return ptr::null_mut(),
        };
        let port_map: VecMap<super::PortConnection> = VecMap::new();
        let ports: Vec<&super::PortConnection> = Vec::new();

        mem::transmute(Box::new(Handle {
            descriptor: rust_desc,
            plugin: rust_plugin,
            port_map: port_map,
            ports: ports,
        }))
    }
}

extern "C" fn connect_port(instance: ladspa_h::Handle,
                           port_num: c_ulong,
                           data_location: *mut ladspa_h::Data) {
    unsafe {
        let handle: &mut Handle = mem::transmute(instance);

        let port = handle.descriptor.ports[port_num as usize];

        // Create appropriate pointers to port data. Mutable locations are wrapped in refcells.
        let data = match port.desc {
            super::PortDescriptor::AudioInput => {
                // Initially create a size 0 slice because we don't know how big it will be yet.
                super::PortData::AudioInput(slice::from_raw_parts(data_location, 0))
            }
            super::PortDescriptor::AudioOutput => {
                // Same here.
                super::PortData::AudioOutput(RefCell::new(slice::from_raw_parts_mut(data_location,
                                                                                    0)))
            }
            super::PortDescriptor::ControlInput => {
                super::PortData::ControlInput(mem::transmute(data_location))
            }
            super::PortDescriptor::ControlOutput => {
                super::PortData::ControlOutput(RefCell::new(mem::transmute(data_location)))
            }
            super::PortDescriptor::Invalid => panic!("Invalid port descriptor!"),
        };

        let conn = super::PortConnection {
            port: port,
            data: data,
        };
        handle.port_map.insert(port_num as usize, conn);

        // Depends on the assumption that ports will be recreated whenever port_map changes
        let handle_ptr: &mut Handle = mem::transmute(instance);
        if handle.port_map.len() == handle.descriptor.ports.len() {
            handle_ptr.ports = handle.port_map.values().collect();
        }
    }
}

extern "C" fn run(instance: ladspa_h::Handle, sample_count: c_ulong) {
    unsafe {
        let handle: &mut Handle = mem::transmute(instance);
        for (_, port) in handle.port_map.iter_mut() {
            match port.data {
                super::PortData::AudioOutput(ref mut data) => {
                    let ptr = data.borrow_mut().as_mut_ptr();
                    *data.borrow_mut() = slice::from_raw_parts_mut(ptr, sample_count as usize);
                }
                super::PortData::AudioInput(ref mut data) => {
                    let ptr = data.as_ptr();
                    *data = slice::from_raw_parts(ptr, sample_count as usize);
                }
                _ => {}
            }
        }
        let mut handle = AssertUnwindSafe(handle);
        call_user_code!(Some({
                            let ref mut handle = *handle;
                            handle.plugin.run(sample_count as usize, &handle.ports)
                        }),
                        "Plugin::run");
    }
}

extern "C" fn activate(instance: ladspa_h::Handle) {
    unsafe {
        let handle: &mut Handle = mem::transmute(instance);
        let mut handle = AssertUnwindSafe(handle);
        call_user_code!(Some(handle.plugin.activate()), "Plugin::activate");
    }
}
extern "C" fn deactivate(instance: ladspa_h::Handle) {
    unsafe {
        let handle: &mut Handle = mem::transmute(instance);
        let mut handle = AssertUnwindSafe(handle);
        call_user_code!(Some(handle.plugin.deactivate()), "Plugin::deactivate");
    }
}

// extern "C" fn run_adding(instance: ladspa_h::Handle, sample_count: c_ulong) {
// }
// extern "C" fn set_run_adding_gain(instance: ladspa_h::Handle, gain: ladspa_h::Data) {
// }

extern "C" fn cleanup(instance: ladspa_h::Handle) {
    unsafe {
        mem::transmute::<_, Box<Handle>>(instance);
    }
}
