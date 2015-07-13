//! An extended version of the [env_logger](http://rust-lang.github.io/log/env_logger/), which
//! can write the log to standard error <i>or to a fresh file</i>
//! and allows custom logline formats.
//!
//! It plugs into the logging facade given by the [log crate](http://rust-lang.github.io/log/log/).
//! See there to learn how to write traces from your code using simple macros.
//!
//! Specifying the log levels that you really want to see in a specific program run
//! happens in the same great way as with the
//! [env_logger](http://rust-lang.github.io/log/env_logger/)
//! (from where this functionality was ruthlessly copied),
//! i.e., using the environment variable RUST_LOG.
//!
//!  Only the initialization is a bit more chatty due to the configurability.
//!
//!
//! ## Example: Initialization
//!
//! If you initialize flexi_logger with default settings, then it behaves like the well-known env_logger:
//!
//! ```
//! use flexi_logger::{detailed_format, LogConfig};
//!
//!     flexi_logger::init( LogConfig::new(), None )
//!             .unwrap_or_else(|e|{panic!("Logger initialization failed with {}",e)});
//! ```
//!
//!
//! Here we configure flexi_logger to write log entries with fine-grained
//! time and location info into a trace file, and we provide the loglevel-specification
//! programmatically as a ```Some<String>```, which fits well to what docopt provides,
//! if you have e.g. a command-line option ```--loglevelspec```:
//!
//! ```
//! use flexi_logger::{detailed_format, LogConfig};
//!
//!     flexi_logger::init( LogConfig {
//!                             log_to_file: true,
//!                             format: flexi_logger::detailed_format,
//!                             .. LogConfig::new()  // use defaults for all other options
//!                         },
//!                         args.flag_loglevelspec
//!     ).unwrap_or_else(|e|{panic!("Logger initialization failed with {}",e)});
//! ```
//!
//! Flexi_logger comes with two predefined format variants, ```default_format()``` and ```detailed_format()```,
//! but you can easily create and use your own format function with the signature ```fn(&LogRecord) -> String```.
//!


extern crate log;
extern crate regex;
extern crate time;

use log::{Log, LogLevel, LogLevelFilter, LogMetadata};
pub use log::LogRecord;
use regex::Regex;
use std::env;
use std::fmt;
use std::fs::File;
use std::io;
use std::io::{LineWriter, Write};
use std::ops::Add;
use std::path::Path;
use std::sync::{Arc, Mutex};


struct FlexiLogger{
    directives: Vec<LogDirective>,
    filter: Option<Regex>,
    line_writer: Arc<Mutex<LineWriter<File>>>,
    config: LogConfig
}
impl FlexiLogger {
    fn new( directives: Vec<LogDirective>, filter: Option<Regex>,
            logfile_path:&str, config: LogConfig) -> FlexiLogger  {
        // we die hard if the log file cannot be opened
        let line_writer = Arc::new(Mutex::new( LineWriter::new(File::create(logfile_path.clone()).unwrap()) ));
        FlexiLogger {directives: directives,filter: filter, line_writer: line_writer, config: config }
    }

    fn ml_enabled(&self, level: LogLevel, target: &str) -> bool {
        // Search for the longest match, the vector is assumed to be pre-sorted.
        for directive in self.directives.iter().rev() {
            match directive.name {
                Some(ref name) if !target.starts_with(&**name) => {},
                Some(..) | None => {
                    return level <= directive.level
                }
            }
        }
        false
    }
}
impl Log for FlexiLogger {
    fn enabled(&self, metadata: &LogMetadata) -> bool {
        self.ml_enabled(metadata.level(), metadata.target())
    }

    fn log(&self, record: &LogRecord) {
        if !Log::enabled(self, record.metadata()) {
            return;
        }

        if let Some(filter) = self.filter.as_ref() {
            if filter.is_match(&*record.args().to_string()) {
                return;
            }
        }

        let mut msg = (self.config.format)(record);
        msg.push('\n');
        if self.config.log_to_file {
            if self.config.duplicate_error && record.level() == LogLevel::Error
            || self.config.duplicate_info  && record.level() == LogLevel::Info {
                println!("{}",&record.args());
            }
            let msgb = msg.as_bytes();
            let lw = self.line_writer.clone();
            let mut lw1 = lw.lock().unwrap(); // FIXME correct error handling
            lw1.write(msgb).unwrap_or_else( |e|{panic!("File logger: write failed with {}",e)} );
        } else {
            let _ = writeln!(&mut io::stderr(), "{}", msg );
        }
    }
}

/// Describes all kinds of errors in the initialization of FlexiLogger.
#[derive(Debug)]
pub struct FlexiLoggerError {
    message: &'static str
}
impl FlexiLoggerError {
    pub fn new(s: &'static str) -> FlexiLoggerError {
        FlexiLoggerError {message: s}
    }
}
impl fmt::Display for  FlexiLoggerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// Allows influencing the behavior of the FlexiLogger.
pub struct LogConfig {
    /// If `true`, the log is written to a file. Default is `false`, the log is then
    /// written to stderr.
    /// If `true`, a new file in the current directory is created and written to.
    /// The name of the file is chosen as '\<program_name\>\_\<date\>\_\<time\>.trc', e.g. `myprog_2015-07-08_10-44-11.trc`
    pub log_to_file: bool,
    /// If `true` (which is default), and if `log_to_file` is `true`, the name of the tracefile is documented in a message to stdout.
    pub print_message: bool,
    /// If `true` (which is default), and if `log_to_file` is `true`, all error messages are written also to stdout.
    pub duplicate_error: bool,
    /// If `true` (which is default), and if `log_to_file` is `true`, also info messages are written also to stdout.
    pub duplicate_info: bool,
    /// Allows providing a custom logline format; default is flexi_logger::default_format.
    pub format: fn(&LogRecord) -> String,
}
impl LogConfig {
    pub fn new() -> LogConfig {
        LogConfig {
            log_to_file: false,
            print_message: true,
            duplicate_error: true,
            duplicate_info: false,
            format: default_format,
        }
    }
}

/// A logline-formatter that produces lines like <br>
/// ```INFO [my_prog::some_submodel] Task successfully read from conf.json```
pub fn default_format(record: &LogRecord) -> String {
    format!( "{} [{}] {}", record.level(), record.location().module_path(), record.args() )
}

/// A logline-formatter that produces lines like <br>
/// ```[2015-07-08 12:12:32:639785] INFO [my_prog::some_submodel] src/some_submodel.rs:26: Task successfully read from conf.json```
#[allow(unused)]
pub fn detailed_format(record: &LogRecord) -> String {
    let timespec = time::get_time(); // high-precision now
    let tm = time::at(timespec);     // formattable. but low-precision now
    let mut time: String = time::strftime("%Y-%m-%d %H:%M:%S:", &tm).unwrap();
    // ugly code to format milli and micro seconds
    let tmp = 1000000000 + timespec.nsec;
    let mut s = tmp.to_string();
    s.remove(9);s.remove(8);s.remove(7);s.remove(0);
    time = time.add(&s);
    format!( "[{}] {} [{}] {}:{}: {}",
                &time,
                record.level(),
                record.location().module_path(),
                record.location().file(),
                record.location().line(),
                &record.args())
}

struct LogDirective {
    name: Option<String>,
    level: LogLevelFilter,
}

/// Initializes the global logger with a flexi logger.
///
/// This should be called early in the execution of a Rust program. Note that the
/// global logger may only be initialized once, subsequent initialization attempts
/// will return an error.
pub fn init(config: LogConfig, loglevelspec: Option<String>) -> Result<(),FlexiLoggerError> {
    log::set_logger( |max_level| {
        let (mut directives, filter) =
            match loglevelspec {
                Some(ref llspec) => {let spec: &str = llspec; parse_logging_spec(&spec)},
                None => {
                    match env::var("RUST_LOG") {
                        Ok(spec) => parse_logging_spec(&spec),
                        Err(..) => (vec![LogDirective { name: None, level: LogLevelFilter::Error }], None),
                    }
                }
            };

        // Sort the provided directives by length of their name, this allows a
        // little more efficient lookup at runtime.
        directives.sort_by(|a, b| {
            let alen = a.name.as_ref().map(|a| a.len()).unwrap_or(0);
            let blen = b.name.as_ref().map(|b| b.len()).unwrap_or(0);
            alen.cmp(&blen)
        });

        let level = {
            let max = directives.iter().map(|d| d.level).max();
            max.unwrap_or(LogLevelFilter::Off)
        };
        max_level.set(level);
        let arg0 = env::args().next().unwrap();
        let filename = Path::new(&arg0).file_stem().unwrap().to_string_lossy();
        let s_timestamp = time::strftime("_%Y-%m-%d_%H-%M-%S",&time::now()).unwrap();
        let s_path = String::with_capacity(50).add(&filename).add(&s_timestamp).add(".trc");
        if config.print_message {
            println!("Trace is written to {}", &s_path);
        }
        Box::new(FlexiLogger::new(directives,filter,&s_path,config))
    }).map_err(|_|{FlexiLoggerError::new("Logger initialization failed")})
}

/// Parse a logging specification string (e.g: "crate1,crate2::mod3,crate3::x=error/foo")
/// and return a vector with log directives.
fn parse_logging_spec(spec: &str) -> (Vec<LogDirective>, Option<Regex>) {
    let mut dirs = Vec::new();

    let mut parts = spec.split('/');
    let mods = parts.next();
    let filter = parts.next();
    if parts.next().is_some() {
        println!("warning: invalid logging spec '{}', \
                 ignoring it (too many '/'s)", spec);
        return (dirs, None);
    }
    mods.map(|m| { for s in m.split(',') {
        if s.len() == 0 { continue }
        let mut parts = s.split('=');
        let (log_level, name) = match (parts.next(), parts.next().map(|s| s.trim()), parts.next()) {
            (Some(part0), None, None) => {
                // if the single argument is a log-level string or number, treat that as a global fallback
                match part0.parse() {
                    Ok(num) => (num, None),
                    Err(_) => (LogLevelFilter::max(), Some(part0)),
                }
            }
            (Some(part0), Some(""), None) => (LogLevelFilter::max(), Some(part0)),
            (Some(part0), Some(part1), None) => {
                match part1.parse() {
                    Ok(num) => (num, Some(part0)),
                    _ => {
                        println!("warning: invalid logging spec '{}', \
                                 ignoring it", part1);
                        continue
                    }
                }
            },
            _ => {
                println!("warning: invalid logging spec '{}', \
                         ignoring it", s);
                continue
            }
        };
        dirs.push(LogDirective {
            name: name.map(|s| s.to_string()),
            level: log_level,
        });
    }});

    let filter = filter.map_or(None, |filter| {
        match Regex::new(filter) {
            Ok(re) => Some(re),
            Err(e) => {
                println!("warning: invalid regex filter - {}", e);
                None
            }
        }
    });

    return (dirs, filter);
}
