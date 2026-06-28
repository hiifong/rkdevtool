use rkdevtool_lib::firmware::{extract_firmware_file, parse_firmware_info};
use std::env;

fn main() {
    let mut args = env::args().skip(1);
    let path = args.next().expect("usage: parse_firmware [--unpack <outdir>] <path-to-update.img>");

    if path == "--unpack" {
        let output = args.next().expect("usage: parse_firmware --unpack <outdir> <path>");
        let input = args.next().expect("usage: parse_firmware --unpack <outdir> <path>");
        let log = extract_firmware_file(&input, &output).expect("unpack failed");
        print!("{log}");
        println!("unpacked to {output}");
        return;
    }

    match parse_firmware_info(&path) {
        Ok(info) => {
            println!("format:           {}", info.format);
            println!("firmware_version: {}", info.firmware_version);
            println!("loader_version:   {}", info.loader_version);
            println!("chip_family:      {}", info.chip_family);
        }
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(1);
        }
    }
}
