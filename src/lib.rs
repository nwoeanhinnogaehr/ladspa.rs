/*!
 * The ```ladspa``` crate provides an interface for writing [LADSPA](http://www.ladspa.org/)
 * plugins safely in Rust.
 *
 * ##Creating the project
 *
 * Run ```cargo new my_ladspa_plugin``` to generate a Cargo project for your plugin, then add
 * the following to the generated Cargo.toml:
 *
 * ```
 * [dependencies]
 * ladspa = "*"
 *
 * [lib]
 * name = "my_ladspa_plugin"
 * crate-type = ["dylib"]
 * ```
 * This will pull in the correct dependency and ensure that the library generated when you build
 * your plugin is compatible with LADSPA hosts.
 *
 * ## Writing the code
 * You'll want to implement
 * ```get_ladspa_descriptor``` in your src/lib.rs. This function is expected to return 1 or more
 * ```PluginDescriptor```s describing the plugins exposed by your library. See the documentation
 * for ```get_ladspa_descriptor``` and the examples
 * [on Github](https://github.com/nwoeanhinnogaehr/ladspa.rs/tree/master/examples) for more
 * information.
 *
 * ## Testing it out
 * There is a list of host software supporting LADSPA on the
 * [LADSPA home page](http://www.ladspa.org/). In order for a host to find your plugin, you will
 * either need to copy the *.so file from target/ after building to /usr/lib/ladspa/ (on most
 * systems, it may be different on your system) or set the enviornment variable ```LADSPA_PATH```
 * to equal the directory where you store your plugins.
 */

extern crate libc;
#[macro_use] extern crate bitflags;
extern crate vec_map;

#[doc(hidden)]
pub mod ffi;

use ffi::ladspa_h;

#[doc(hidden)]
pub use ffi::ladspa_descriptor;

use std::cell::{RefCell, RefMut};
use std::default::Default;

#[allow(improper_ctypes)]
extern {
    /**
     * Your plugin must implement this function.
     * ```get_ladspa_descriptor``` returns a description of a supported plugin for a given plugin
     * index. When the index is out of bounds for the number of plugins supported by your library,
     * you are expected to return ```None```.
     *
     * Example no-op implementation:
     *
     * ```rust{.ignore}
     * #[no_mangle]
     * pub extern fn get_ladspa_descriptor(index: u64) -> Option<ladspa::PluginDescriptor> {
     *     None
     * }
     * ```
     */
    pub fn get_ladspa_descriptor(index: u64) -> Option<PluginDescriptor>;
}

/// The data type used internally by LADSPA for audio and control ports.
pub type Data = f32;

/// Describes the properties of a ```Plugin``` to be exposed as a LADSPA plugin.
pub struct PluginDescriptor {
    /// Unique IDs are an unfortunate remnant of the LADSPA API. During development, it is
    /// suggested to pick one under 1000, but it should be changed before release. More information
    /// is available here: http://www.ladspa.org/ladspa_sdk/unique_ids.html
    pub unique_id: u64,

    /// Plugin labels are expected to be a unique descriptor string for this specific plugin within
    /// the library. Labels are case sensitive and expected not to contain spaces.
    pub label: &'static str,

    /// The properties of a plugin describe restrictions and features for it's use. See
    /// documentation for ```Properties``` for info on available options.
    pub properties: Properties,

    /// The name of the plugin. This is usually how it is identified.
    pub name: &'static str,

    /// The maker of the plugin. Can be empty.
    pub maker: &'static str,

    /// Indicates copyright of the plugin. If no copyright applies, "None" should be used.
    pub copyright: &'static str,

    /// A vector of input and output ports exposed by the plugin. See the documentation for
    /// ```Port``` for more information.
    pub ports: Vec<Port>,

    /// A function which creates a new instance of the plugin.
    ///
    /// Note: Initialization, such as resetting plugin state, should go in ```Plugin::activate``` rather
    /// than here. This should just return a basic instance, ready to be activated.
    /// If your plugin has no internal state, you may optionally not implement ```Plugin::activate```
    /// and do everything here.
    pub new: fn(desc: &PluginDescriptor, sample_rate: u64) -> Box<Plugin + Send>,
}

#[derive(Copy, Clone, Default)]
/// Represents an input or output to the plugin representing either audio or
/// control data.
pub struct Port {
    /// The name of the port. For control ports, this will likely be shown by the host in an
    /// automatically generated GUI next to the control. For audio ports, it is mostly just
    /// for identification in your code but some hosts may display it.
    pub name: &'static str,

    /// Describes the type of port: audio or control, input or output.
    pub desc: PortDescriptor,

    /// Most useful on control inputs but can be used on any type of port.
    pub hint: Option<ControlHint>,

    /// Most useful on control inputs but can be used on any type of port.
    pub default: Option<DefaultValue>,

    /// The lower bound of values to accepted by default (the host may ignore this).
    pub lower_bound: Option<Data>,

    /// The upper bound of values to accepted by default (the host may ignore this).
    pub upper_bound: Option<Data>,
}

#[derive(Copy, Clone)]
/// Represents the 4 types of ports: audio or control, input or output.
pub enum PortDescriptor {
    Invalid = 0,
    AudioInput = (ladspa_h::PORT_AUDIO | ladspa_h::PORT_INPUT) as isize,
    AudioOutput = (ladspa_h::PORT_AUDIO | ladspa_h::PORT_OUTPUT) as isize,
    ControlInput = (ladspa_h::PORT_CONTROL | ladspa_h::PORT_INPUT) as isize,
    ControlOutput = (ladspa_h::PORT_CONTROL | ladspa_h::PORT_OUTPUT) as isize,
}

impl Default for PortDescriptor {
    fn default() -> PortDescriptor {
        PortDescriptor::Invalid
    }
}

bitflags!(
    #[doc="Represents the special properties a control port may hold. These are merely hints as to the
    use of the port and may be completely ignored by the host. For audio ports, use ```CONTROL_HINT_NONE```.
    To attach multiple properties, bitwise-or them together.
    See documentation for the constants beginning with HINT_ for the more information."]
    pub flags ControlHint: i32 {
        #[doc="Indicates that this is a toggled port. Toggled ports may only have default values
        of zero or one, although the host may send any value, where <= 0 is false and > 0 is true."]
        const HINT_TOGGLED = ::ffi::ladspa_h::HINT_TOGGLED,

        #[doc="Indicates that all values related to the port will be multiplied by the sample rate by
        the host before passing them to your plugin. This includes the lower and upper bounds. If you
        want an upper bound of 22050 with this property and a sample rate of 44100, set the upper bound
        to 0.5"]
        const HINT_SAMPLE_RATE = ::ffi::ladspa_h::HINT_SAMPLE_RATE,

        #[doc="Indicates that the data passed through this port would be better represented on a
        logarithmic scale"]
        const HINT_LOGARITHMIC = ::ffi::ladspa_h::HINT_LOGARITHMIC,

        #[doc="Indicates that the data passed through this port should be represented as integers. Bounds
        may be interpreted exclusively depending on the host"]
        const HINT_INTEGER = ::ffi::ladspa_h::HINT_INTEGER,
    }
);

#[derive(Copy, Clone)]
/// The default values that a control port may hold. For audio ports, use DefaultControlValue::None.
pub enum DefaultValue {
    /// Equal to the ```lower_bound``` of the ```Port```.
    Minimum = ladspa_h::HINT_DEFAULT_MINIMUM as isize,
    /// For ports with
    /// ```LADSPA_HINT_LOGARITHMIC```, this should be ```exp(log(lower_bound) * 0.75 +
    /// log(upper_bound) * 0.25)```. Otherwise, this should be ```(lower_bound * 0.75 +
    /// upper_bound * 0.25)```.
    Low = ladspa_h::HINT_DEFAULT_LOW as isize,
    /// For ports with
    /// ```CONTROL_HINT_LOGARITHMIC```, this should be ```exp(log(lower_bound) * 0.5 +
    /// log(upper_bound) * 0.5)```. Otherwise, this should be ```(lower_bound * 0.5 +
    /// upper_bound * 0.5)```.
    Middle = ladspa_h::HINT_DEFAULT_MIDDLE as isize,
    /// For ports with
    /// ```LADSPA_HINT_LOGARITHMIC```, this should be ```exp(log(lower_bound) * 0.25 +
    /// log(upper_bound) * 0.75)```. Otherwise, this should be ```(lower_bound * 0.25 +
    /// upper_bound * 0.75)```.
    High = ladspa_h::HINT_DEFAULT_HIGH as isize,
    /// Equal to the ```upper_bound``` of the ```Port```.
    Maximum = ladspa_h::HINT_DEFAULT_MAXIMUM as isize,

    /// Equal to 0 or false for toggled values.
    Value0 = ladspa_h::HINT_DEFAULT_0 as isize,
    /// Equal to 1 or true for toggled values.
    Value1 = ladspa_h::HINT_DEFAULT_1 as isize,
    /// Equal to 100.
    Value100 = ladspa_h::HINT_DEFAULT_100 as isize,
    /// Equal to 440, concert A. This may be off by a few Hz if the host is using an alternate
    /// tuning.
    Value440 = ladspa_h::HINT_DEFAULT_440 as isize,
}

/// Represents a connection between a port and the data attached to the port by the plugin
/// host.
pub struct PortConnection<'a> {
    /// The port which the data is connected to.
    pub port: Port,

    /// The data connected to the port. It's usually simpler to use the various unwrap_* functions
    /// than to interface with this directly.
    pub data: PortData<'a>,
}

/// Represents the four types of data a port can hold.
pub enum PortData<'a> {
    AudioInput(&'a [Data]),
    AudioOutput(RefCell<&'a mut [Data]>),
    ControlInput(&'a Data),
    ControlOutput(RefCell<&'a mut Data>),
}

unsafe impl<'a> Sync for PortData<'a> { }

impl<'a> PortConnection<'a> {
    /// Returns a slice pointing to the internal data of an audio input port. Panics if this port
    /// is not an ```AudioIn``` port.
    pub fn unwrap_audio(&'a self) -> &'a [Data] {
        if let PortData::AudioInput(ref data) = self.data {
            data
        } else {
            panic!("PortConnection::unwrap_audio called on a non audio input port!")
        }
    }

    /// Returns a mutable slice pointing to the internal data of an audio output port. Panics if
    /// this port is not an ```AudioOut``` port.
    pub fn unwrap_audio_mut(&'a self) -> RefMut<'a, &'a mut [Data]> {
        if let PortData::AudioOutput(ref data) = self.data {
            data.borrow_mut()
        } else {
            panic!("PortConnection::unwrap_audio_mut called on a non audio output port!")
        }
    }

    /// Returns a refrence to the internal data of an control input port. Panics if this port
    /// is not an ```ControlIn``` port.
    pub fn unwrap_control(&'a self) -> &'a Data {
        if let PortData::ControlInput(data) = self.data {
            data
        } else {
            panic!("PortConnection::unwrap_control called on a non control input port!")
        }
    }

    /// Returns a mutable refrence to the internal data of an audio output port. Panics if
    /// this port is not an ```ControlOut``` port.
    pub fn unwrap_control_mut(&'a self) -> RefMut<'a, &'a mut Data> {
        if let PortData::ControlOutput(ref data) = self.data {
            data.borrow_mut()
        } else {
            panic!("PortConnection::unwrap_control called on a non control output port!")
        }
    }
}

bitflags!(
    #[doc="Represents the special properties a LADSPA plugin can have.
    To attach multiple properties, bitwise-or them together, for example
    ```PROP_REALTIME | PROP_INPLACE_BROKEN```.
    See documentation for the constants beginning with PROP_ for the more information."]
    pub flags Properties: i32 {

        #[doc="No properties."]
        const PROP_NONE = 0,

        #[doc="Indicates that the plugin has a realtime dependency so it's output may not be cached."]
        const PROP_REALTIME = ::ffi::ladspa_h::PROPERTY_REALTIME,

        #[doc="Indicates that the plugin will not function correctly if the input and output audio
        data has the same memory location. This could be an issue if you copy input to output
        then refer back to previous values of the input as they will be overwritten. It is
        recommended that you avoid using this flag if possible as it can decrease the speed of
        the plugin."]
        const PROP_INPLACE_BROKEN = ::ffi::ladspa_h::PROPERTY_INPLACE_BROKEN,

        #[doc="Indicates that the plugin is capable of running not only in a conventional host but
        also in a 'hard real-time' environment. To qualify for this the plugin must
        satisfy all of the following:

        * The plugin must not use malloc(), free() or other heap memory
        management within its run() function. All new
        memory used in run() must be managed via the stack. These
        restrictions only apply to the run() function.

        * The plugin will not attempt to make use of any library
        functions with the exceptions of functions in the ANSI standard C
        and C maths libraries, which the host is expected to provide.

        * The plugin will not access files, devices, pipes, sockets, IPC
        or any other mechanism that might result in process or thread
        blocking.

        * The plugin will take an amount of time to execute a run()
        call approximately of form (A+B*SampleCount) where A
        and B depend on the machine and host in use. This amount of time
        may not depend on input signals or plugin state. The host is left
        the responsibility to perform timings to estimate upper bounds for
        A and B."]
        const PROP_HARD_REALTIME_CAPABLE = ::ffi::ladspa_h::PROPERTY_HARD_RT_CAPABLE,
    }
);

/// Represents an instance of a plugin which may be exposed as a LADSPA plugin using
/// ```get_ladspa_descriptor```. It is not necessary to implement activate to deactivate.
pub trait Plugin {
    /// The plugin instance must reset all state information dependent
    /// on the history of the plugin instance here.
    /// Will be called before `run` is called for the first time.
    fn activate(&mut self) { }

    /// Runs the plugin on a number of samples, given the connected ports.
    fn run<'a>(&mut self, sample_count: usize, ports: &[&'a PortConnection<'a>]);

    /// Indicates the plugin is no longer live.
    fn deactivate(&mut self) { }
}
