use std::fs::File;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use crate::constants::*;
use crate::ppu::{CURRENT_CYCLE, CURRENT_SCANLINE};

pub fn init_logging(log_to_file: Option<String>) {
    use tracing_subscriber::EnvFilter;
    use tracing_subscriber::fmt::{format::Writer, FmtContext, FormatEvent, FormatFields};
    use tracing_subscriber::registry::LookupSpan;
    use tracing::{Event, Subscriber};
    use std::fmt;
    struct MyCustomFormat;
    impl<S, N> FormatEvent<S, N> for MyCustomFormat
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
        N: for<'a> FormatFields<'a> + 'static,
    {
        fn format_event(
            &self,
            ctx: &FmtContext<'_, S, N>,
            mut writer: Writer<'_>,
            event: &Event<'_>,
        ) -> fmt::Result
        {
            let metadata = event.metadata();
            let mut fields_buf = String::new();
            {
                let buf_writer = Writer::new(&mut fields_buf);
                ctx.format_fields(buf_writer, event)?;
            }

            // Format the log message with level and fields
            write!(writer, "{}:{} - {:03},{:03} - {}\n",
                metadata.level(), metadata.target(),
                *CURRENT_CYCLE.read().unwrap(),
                *CURRENT_SCANLINE.read().unwrap(),
                fields_buf)
        }
    }

    // Create a filter to allow various trace levels
    // The format is: target[span{field=value}]=level
    // Allow info, debug, and trace levels for most modules, and enable specific targets
    // Disable all logs from iced, wgpu, and related graphics/GPU modules
    let ice_logs_off = "iced=off,::iced=off,wgpu=off,::wgpu=off,wgpu_core=off,::wgpu_core=off,wgpu_hal=off,::wgpu_hal=off,gpu=off,graphics=off,vulkan=off,adapter=off";
    let ir_s = if IR { "ir=debug" } else { "ir=off" };
    let vram_s = if VRAM { "vram=debug" } else { "vram=off" };
    let rom_s = if ROM { "rom=debug" } else { "rom=off" };
    let mapper_s = if MAPPER { "mapper=debug" } else { "mapper=off" };
    let vbl_s = if VBL { "vbl=debug" } else { "vbl=off" };
    let filter = EnvFilter::new(format!(
        "info,{vbl_s},{ir_s},{vram_s},{rom_s},{mapper_s},ppu=off,sleep=off,oam=off,4014=off,frame=off,{ice_logs_off}"));

    if let Some(file_name) = log_to_file {
        // Create the directory if it doesn't exist
        let dir = format!("{}\\t", dirs::home_dir().unwrap().to_str().unwrap());
        std::fs::create_dir_all(&dir).unwrap();
        let full_path = &format!("{}/{}", dir, file_name);
        let file = File::create(full_path).unwrap();

        let file_layer = tracing_subscriber::fmt::layer()
            .event_format(MyCustomFormat)
            .with_writer(file)
            .with_ansi(false);
        // .without_time()
        // .with_level(false)
        // .with_target(false)
        // .compact();

        // Initialize with both layers and the filter
        // let stdout_layer = tracing_subscriber::fmt::layer()
        //     .event_format(MyCustomFormat);

        tracing_subscriber::registry()
            // .with(stdout_layer)
            .with(file_layer)
            .with(filter)
            .init();
    } else {
        // Initialize with just the stdout layer and the filter
        // Create a stdout layer with ANSI colors and custom format (skipping first two fields)
        let stdout_layer = tracing_subscriber::fmt::layer()
            .event_format(MyCustomFormat);

        tracing_subscriber::registry()
            .with(stdout_layer)
            .with(filter)
            .init();
    }
}
